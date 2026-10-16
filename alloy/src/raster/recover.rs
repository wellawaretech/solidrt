//! Context-loss recovery (okf/done/gpu-context-loss.md): the raster thread
//! trades its dead context for a fresh one from the platform side and
//! rebuilds every GPU object behind the same registry ids, so the UI thread's
//! ids stay valid and the app is asked only for the content it alone holds.
//!
//! Nothing of the dead context is deleted by hand: its objects go with the
//! context when the platform destroys it (GL's contract), and on a lost
//! context every delete is a no-op anyway. The one ordering that matters is
//! Impeller's: its context is dropped while the context it was created on is
//! still current, before the binding is released. The old registries stay
//! alive through the rebuild as the blueprint - a texture's size, shape,
//! format and sampling, a buffer's size, a program's sources, a target's draw
//! list - and drop afterwards.

use std::collections::HashSet;
use std::rc::Rc;
use std::sync::atomic::Ordering;
use std::sync::mpsc;

use impellers::ISize;

use super::{CameraTapEntry, RasterState};
use crate::backend::FrameOutput;
use crate::gl::{self, GpuBuffer, GpuTexture, RebuildRegistries};
use crate::gpu::{LostContent, SamplerState, TextureFormat, TextureShape};

impl RasterState {
  /// Recover from a lost context: this state, rebuilt on a fresh one. Err
  /// when the platform cannot provide one (the caller exits, as a confirmed
  /// loss always did). On return the state awaits the UI thread's resync
  /// (`RasterCmd::Resync`), and the main loop holds the restore report.
  pub(crate) fn recover(self, detail: String) -> Result<RasterState, String> {
    log::error!("[alloy] GPU context lost: {detail}; recovering");
    self.stats.context_losses.fetch_add(1, Ordering::Relaxed);
    let RasterState {
      gl,
      impeller_ctx,
      binding,
      surface_size,
      last_size,
      sink,
      stats,
      tx,
      wake,
      demand_latch,
      textures,
      yuv_latches,
      camera_taps,
      unadopted,
      shaders,
      node_shader_passes,
      node_shader_issue_micros,
      node_shader_exec_micros,
      node_shader_vertices,
      regions,
      target_depths,
      depth_owners,
      programs,
      render_pipelines,
      stages,
      buffers,
      window_shader,
      pass_only_frames,
      adopted_snapshots,
      ..
    } = self;

    // Impeller first, on the context it was created on; then no context on
    // this thread, so the platform may destroy it.
    drop(impeller_ctx);
    if !binding.release() {
      log::warn!("[alloy] releasing the lost GL context failed: {}", binding.error());
    }
    drop(binding);
    drop(gl);

    let (reply_tx, reply_rx) = mpsc::channel();
    tx.send(FrameOutput::ContextLost { detail, reply: reply_tx })
      .map_err(|_| "the platform loop is gone".to_string())?;
    if let Some(wake) = &wake {
      wake();
    }
    let binding = reply_rx
      .recv()
      .map_err(|_| "the platform loop dropped the recovery request".to_string())?
      .ok_or_else(|| "the platform could not recreate the GL context".to_string())?;
    if !binding.bind() {
      return Err(format!("GL make-current failed on the recovered context: {}", binding.error()));
    }
    if !sink.headless() && !binding.set_swap_interval() {
      log::warn!("[alloy] set swap interval failed: {}", binding.error());
    }
    let gl = gl::create_gl_context(&*binding);
    let impeller_ctx = gl::create_impeller_context(&*binding);
    unsafe {
      use glow::HasContext;
      let vendor = gl.get_parameter_string(glow::VENDOR);
      let renderer = gl.get_parameter_string(glow::RENDERER);
      let version = gl.get_parameter_string(glow::VERSION);
      let limits = gl::query_limits(&gl);
      log::info!("[alloy] GPU ready: {vendor} | {renderer} | {version} (recovered context)");
      crate::set_gpu_info(crate::GpuInfo { vendor, renderer, version, limits });
    }
    gl::forget_window_samples();
    let mut state = RasterState::new(gl, impeller_ctx, binding, surface_size, sink, stats, tx, wake);
    state.demand_latch = demand_latch;
    state.last_size = last_size;
    state.pass_only_frames = pass_only_frames;
    state.node_shader_passes = node_shader_passes;
    state.node_shader_issue_micros = node_shader_issue_micros;
    state.node_shader_exec_micros = node_shader_exec_micros;
    state.node_shader_vertices = node_shader_vertices;

    let mut handles: Vec<(u64, impellers::Texture)> = Vec::new();
    let mut lost = LostContent::default();
    // What the engine refills itself: a camera's stream texture at the next
    // frame, a video's planes at the next latch.
    let mut engine_owned: HashSet<u64> = camera_taps.keys().copied().collect();
    for (id, entry) in &yuv_latches {
      engine_owned.insert(*id);
      for set in &entry.sets {
        for (_, plane, _) in set {
          engine_owned.insert(*plane);
        }
      }
    }

    // Uploaded textures: storage at the declared size, content undefined.
    // Targets and depth textures come back with their targets below; a
    // snapshot boundary's adoption is the UI side's to publish again.
    for (id, old) in &textures {
      if shaders.contains_key(id) || depth_owners.contains_key(id) || adopted_snapshots.contains(id) {
        continue;
      }
      let gpu = match GpuTexture::allocate_like(&state.gl, old) {
        Ok(gpu) => gpu,
        Err(e) => {
          log::warn!("[alloy] context recovery: texture {id} not rebuilt: {e}");
          continue;
        }
      };
      if gpu.shape == TextureShape::D2 {
        let size = ISize::new(gpu.width as i64, gpu.height as i64);
        match gl::adopt_texture(&gpu, &state.impeller_ctx, size) {
          Some(handle) => handles.push((*id, handle)),
          None => {
            log::warn!("[alloy] context recovery: texture {id} not adopted");
            unsafe { glow::HasContext::delete_texture(&state.gl, gpu.gl_texture) };
            continue;
          }
        }
      }
      state.textures.insert(*id, gpu);
      if !engine_owned.contains(id) {
        lost.textures.push(*id);
      }
    }

    // Buffers at their size, zeroed: the content is the app's.
    for (id, old) in &buffers {
      match GpuBuffer::new(&state.gl, &vec![0u8; old.size], old.label.clone()) {
        Ok(buffer) => {
          state.buffers.insert(*id, Rc::new(buffer));
          lost.buffers.push(*id);
        }
        Err(e) => log::warn!("[alloy] context recovery: buffer {id} not rebuilt: {e}"),
      }
    }

    // Stages, programs and pipelines, from their retained sources and
    // descriptors, in dependency order.
    for (id, old) in &stages {
      match old.recompile(&state.gl) {
        Ok(stage) => {
          state.stages.insert(*id, stage);
        }
        Err(e) => log::warn!("[alloy] context recovery: shader stage {id} not rebuilt: {e}"),
      }
    }
    for (id, old) in &programs {
      match old.relink(&state.gl) {
        Ok(program) => {
          state.programs.insert(*id, Rc::new(program));
        }
        Err(e) => log::warn!("[alloy] context recovery: program {id} not rebuilt: {e}"),
      }
    }
    for (id, old) in &render_pipelines {
      let rebuilt = match old.program_id().and_then(|pid| state.programs.get(&pid).cloned()) {
        Some(program) => old.rebuilt_over(program),
        None => old.relink(&state.gl),
      };
      match rebuilt {
        Ok(pipeline) => {
          state.render_pipelines.insert(*id, Rc::new(pipeline));
        }
        Err(e) => log::warn!("[alloy] context recovery: pipeline {id} not rebuilt: {e}"),
      }
    }

    // Targets: the storage owners first, their tiles over them. A manual
    // target comes back cleared and is the app's to render again; every
    // other one is dirty and re-renders at the next flush.
    let registries = RebuildRegistries { pipelines: &state.render_pipelines, buffers: &state.buffers };
    for (id, old) in shaders.iter().filter(|(_, s)| s.region().is_none()) {
      let shader = match old.rebuild(&state.gl, &registries) {
        Ok(shader) => shader,
        Err(e) => {
          log::warn!("[alloy] context recovery: target {id} not rebuilt: {e}");
          continue;
        }
      };
      let Some(meta) = textures.get(id) else {
        log::warn!("[alloy] context recovery: target {id} has no texture entry");
        shader.destroy(&state.gl);
        continue;
      };
      let (width, height) = shader.size();
      let size = ISize::new(width as i64, height as i64);
      let gpu = GpuTexture {
        gl_texture: shader.gl_texture(),
        width,
        height,
        depth: 1,
        shape: meta.shape,
        sampler: shader.sampler(),
        format: shader.format(),
        label: meta.label.clone(),
      };
      let adopted = meta.shape == TextureShape::D2 && !unadopted.contains(id);
      if adopted {
        match gl::adopt_texture(&gpu, &state.impeller_ctx, size) {
          Some(handle) => handles.push((*id, handle)),
          None => {
            log::warn!("[alloy] context recovery: target {id} not adopted");
            shader.destroy(&state.gl);
            unsafe { glow::HasContext::delete_texture(&state.gl, gpu.gl_texture) };
            continue;
          }
        }
      } else if unadopted.contains(id) {
        state.unadopted.insert(*id);
      }
      if let (Some(depth_name), Some(depth_id)) = (shader.depth_texture(), target_depths.get(id)) {
        let depth_gpu = GpuTexture {
          gl_texture: depth_name,
          width,
          height,
          depth: 1,
          shape: TextureShape::D2,
          sampler: SamplerState::DEPTH,
          format: TextureFormat::Depth24,
          label: textures.get(depth_id).and_then(|t| t.label.clone()),
        };
        match gl::adopt_texture(&depth_gpu, &state.impeller_ctx, size) {
          Some(handle) => {
            handles.push((*depth_id, handle));
            state.textures.insert(*depth_id, depth_gpu);
            state.target_depths.insert(*id, *depth_id);
            state.depth_owners.insert(*depth_id, *id);
          }
          None => log::warn!("[alloy] context recovery: depth texture {depth_id} not adopted"),
        }
      }
      if shader.manual() {
        shader.clear(&state.gl);
        lost.targets.push(*id);
      } else {
        state.dirty.insert(*id);
      }
      state.textures.insert(*id, gpu);
      state.shaders.insert(*id, shader);
    }
    for (id, old) in shaders.iter().filter(|(_, s)| s.region().is_some()) {
      let parent_id = old.region().map(|r| r.parent).expect("filtered on region");
      let rebuilt = match state.shaders.get(&parent_id) {
        Some(parent) => old.rebuild_sub(&state.gl, parent, &registries),
        None => Err(format!("parent target {parent_id} was not rebuilt")),
      };
      match rebuilt {
        Ok(shader) => {
          state.shaders.insert(*id, shader);
          state.dirty.insert(*id);
        }
        Err(e) => log::warn!("[alloy] context recovery: sub-target {id} not rebuilt: {e}"),
      }
    }
    for (parent, tiles) in &regions {
      let kept: Vec<u64> = tiles.iter().copied().filter(|t| state.shaders.contains_key(t)).collect();
      if !kept.is_empty() {
        state.regions.insert(*parent, kept);
      }
    }

    // The window shader over the rebuilt programs. The overlay is not
    // carried: its display list belongs to the dead context, and the UI
    // thread declares it again after the resync.
    if let Some(window_shader) = window_shader {
      state.set_window_shader(Some(window_shader.spec));
    }
    // A camera's texture is refilled from its tap at the next frame (the
    // sequence restarts so the newest frame uploads whatever its number); a
    // video's planes at the next latch.
    for (id, entry) in camera_taps {
      state.camera_taps.insert(id, CameraTapEntry { tap: entry.tap, uploaded_seq: 0 });
    }
    for (id, entry) in yuv_latches {
      state.yuv_latches.insert(id, entry);
    }
    state.content_dirty = true;
    state.damage.invalidated();
    state.awaiting_resync = true;

    lost.textures.sort_unstable();
    lost.buffers.sort_unstable();
    lost.targets.sort_unstable();
    log::info!(
      "[alloy] GPU context restored: {} textures, {} buffers, {} programs, {} targets rebuilt; the app re-supplies {} textures, {} buffers, {} manual targets",
      state.textures.len(),
      state.buffers.len(),
      state.programs.len(),
      state.shaders.len(),
      lost.textures.len(),
      lost.buffers.len(),
      lost.targets.len()
    );
    state
      .tx
      .send(FrameOutput::ContextRestored { handles, lost })
      .map_err(|_| "the platform loop is gone".to_string())?;
    if let Some(wake) = &state.wake {
      wake();
    }
    Ok(state)
  }
}
