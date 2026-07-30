use impellers::{DisplayList, ISize, Texture};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::atomic::Ordering;
use std::sync::{mpsc, Arc};

use crate::audio::AudioRegistry;
use crate::camera::CameraRegistry;
use crate::gpu::{GpuResources, ParamValue, PipelineDesc, PipelineSpec, ShaderStage, TargetSpec, WindowShader};
use crate::microphone::MicrophoneRegistry;
use crate::raster::{RasterCmd, RasterSender, RasterStats};
use crate::texture::{SamplerState, TextureEntry, TextureRegistry};

// All GL work - texture uploads, shader passes, offscreen rasterization,
// compositing, present - runs on the raster thread, which owns the process's
// single GL context and single Impeller context (see raster.rs for why).
// Context is the UI thread's handle on it: methods marshal into RasterCmds,
// either fire-and-forget sends or blocking RPCs. The UI side keeps just enough
// bookkeeping (texture dims, shader kinds, buffer sizes) to validate ids and
// answer size queries without a round trip.

pub struct Context {
  raster_tx: RasterSender,
  // Shared live counters (queue depth, idle ticks, fence timeouts, pass
  // count/time), exposed through the accessors below for diagnostics. See
  // RasterStats for what each one means.
  stats: Arc<RasterStats>,
  pub textures: TextureRegistry,
  // UI-side mirror of the raster thread's shader map: id -> is_pipeline.
  // Enough to validate params/draw-count updates without an RPC.
  shader_kinds: RefCell<HashMap<u64, bool>>,
  // UI-side mirror of each shader target's sampler graph: target id ->
  // (uniform name -> source texture id). Lets update_shader_textures reject
  // sampling cycles synchronously; the raster thread walks the same edges to
  // propagate re-renders through target chains, and a cycle there would
  // under-render, so it must never form. One divergence: a rebind naming an
  // unknown uniform is warn-and-ignored raster-side but still recorded here
  // (the name is only known to the compiled program), which can at worst
  // falsely reject a later rebind - after the dev error was already warned.
  shader_sources: RefCell<HashMap<u64, HashMap<String, u64>>>,
  // UI-side mirror of the raster thread's program registry. Programs are
  // their own id space (like buffers), separate from texture ids.
  program_ids: RefCell<HashSet<u64>>,
  next_program_id: Cell<u64>,
  // UI-side mirror of the raster thread's render pipeline registry, its own
  // id space again.
  pipeline_ids: RefCell<HashSet<u64>>,
  next_pipeline_id: Cell<u64>,
  // UI-side mirror of the raster thread's raw stage registry: id -> stage,
  // for validating link_program's arguments without an RPC.
  stage_kinds: RefCell<HashMap<u64, ShaderStage>>,
  next_stage_id: Cell<u64>,
  // UI-side mirror of the raster thread's buffer sizes, for bounds validation
  // and gpu_buffer_len.
  buffer_sizes: RefCell<HashMap<u64, usize>>,
  next_buffer_id: Cell<u64>,
  pub(crate) cameras: CameraRegistry,
  pub(crate) microphones: MicrophoneRegistry,
  pub(crate) audio: AudioRegistry,
  // On-demand node captures (captureSnapshot, dev-server snapshot). Requests are
  // keyed by node id (many requests may target one node) and drained by the
  // paint walk when it visits that node; each carries its own completion
  // callback. Serviced outcomes wait in `capture_ready` and are delivered once,
  // after the walk, by `deliver_captures`. The callback is a plain `FnOnce` - no
  // engine types - so each consumer (a JS-promise settler, a dev-server reply)
  // supplies its own and the rendertree stays engine-independent.
  capture_requests: RefCell<HashMap<u64, Vec<CaptureDone>>>,
  capture_ready: RefCell<Vec<(CaptureDone, Result<CaptureInfo, String>)>>,
  // Ids handed to destroy_texture, awaiting reclamation. Actual destruction
  // happens in reclaim_destroyed, once the live render tree no longer
  // references the id (see destroy_texture for why deferral is the contract).
  pending_destroys: RefCell<Vec<u64>>,
}

/// The successful outcome of a node capture: the registry id of the texture the
/// node's subtree was rasterized into, and its device-pixel dimensions. The
/// caller owns the texture and must `destroy_texture` it.
pub struct CaptureInfo {
  pub texture_id: u64,
  pub width: u32,
  pub height: u32,
}

/// A capture completion callback, invoked exactly once with the outcome after
/// the paint pass that serviced (or failed to service) the request. Runs on the
/// UI thread, out of the tree walk (see `deliver_captures`).
pub type CaptureDone = Box<dyn FnOnce(Result<CaptureInfo, String>)>;

// Safety: Context is asserted Send + Sync (its Arc crosses into the closure
// the UI thread runs), but its interior (Rc entries, RefCell maps) is only
// ever accessed from the UI thread. The raster thread shares nothing with
// this struct beyond the command channel, whose Sender is Send.
unsafe impl Send for Context {}
unsafe impl Sync for Context {}

impl Context {
  pub(crate) fn new(raster_tx: RasterSender, stats: Arc<RasterStats>) -> Self {
    Context {
      raster_tx,
      stats,
      textures: TextureRegistry::new(),
      shader_kinds: RefCell::new(HashMap::new()),
      shader_sources: RefCell::new(HashMap::new()),
      program_ids: RefCell::new(HashSet::new()),
      next_program_id: Cell::new(1),
      pipeline_ids: RefCell::new(HashSet::new()),
      next_pipeline_id: Cell::new(1),
      stage_kinds: RefCell::new(HashMap::new()),
      next_stage_id: Cell::new(1),
      buffer_sizes: RefCell::new(HashMap::new()),
      next_buffer_id: Cell::new(1),
      cameras: CameraRegistry::default(),
      microphones: MicrophoneRegistry::default(),
      audio: AudioRegistry::default(),
      capture_requests: RefCell::new(HashMap::new()),
      capture_ready: RefCell::new(Vec::new()),
      pending_destroys: RefCell::new(Vec::new()),
    }
  }

  /// Fire-and-forget command. A send after the raster thread exited (engine
  /// shutdown) is dropped silently; `submit` is where shutdown is detected.
  fn send(&self, cmd: RasterCmd) {
    self.raster_tx.send(cmd).ok();
  }

  /// Blocking RPC: send a command carrying a reply sender and wait for the
  /// reply. Err only when the raster thread is gone (engine shutdown).
  fn rpc<T>(&self, make: impl FnOnce(mpsc::Sender<T>) -> RasterCmd) -> Result<T, String> {
    let (reply_tx, reply_rx) = mpsc::channel();
    self.raster_tx.send(make(reply_tx)).map_err(|_| "raster thread exited".to_string())?;
    reply_rx.recv().map_err(|_| "raster thread exited".to_string())
  }

  /// Raster commands sent but not yet executed (queued plus the one in hand).
  /// 0 means the raster thread is genuinely idle.
  pub fn raster_queue_depth(&self) -> usize {
    self.stats.queue_depth.load(Ordering::Acquire)
  }

  /// Cumulative idle Ticks the frame loop has emitted.
  pub fn idle_ticks(&self) -> u64 {
    self.stats.idle_ticks.load(Ordering::Relaxed)
  }

  /// Cumulative present-fence timeouts on the raster thread. Nonzero means
  /// the GPU has been over budget; climbing means it still is.
  pub fn fence_timeouts(&self) -> u64 {
    self.stats.fence_timeouts.load(Ordering::Relaxed)
  }

  /// Cumulative shader/pipeline target passes executed on the raster thread.
  /// Diffed against presented frames, this exposes redundant target
  /// re-renders (passes-per-frame far above the target count).
  pub fn passes(&self) -> u64 {
    self.stats.passes.load(Ordering::Relaxed)
  }

  /// Cumulative raster-thread wall time spent executing those passes, in
  /// microseconds. Occupancy, not GPU-side duration (see RasterStats).
  pub fn pass_micros(&self) -> u64 {
    self.stats.pass_micros.load(Ordering::Relaxed)
  }

  /// Cumulative raster-thread wall time spent executing non-Frame commands
  /// (uploads, readbacks, rasterizations, compiles, param writes), in
  /// microseconds - the work no frame-phase timing sees (see RasterStats).
  pub fn cmd_micros(&self) -> u64 {
    self.stats.cmd_micros.load(Ordering::Relaxed)
  }

  /// Queue a capture of `node_id`'s subtree, serviced on the next paint pass
  /// that visits the node. `done` is invoked once with the outcome after that
  /// pass. If the node is never visited (not in the live tree), the request is
  /// failed by `fail_unserviced_captures`.
  pub fn request_capture(&self, node_id: u64, done: CaptureDone) {
    self.capture_requests.borrow_mut().entry(node_id).or_default().push(done);
  }

  /// Whether any capture is queued. Checked per visited node on the paint hot
  /// path, so it stays a cheap borrow with no allocation.
  pub fn has_pending_captures(&self) -> bool {
    !self.capture_requests.borrow().is_empty()
  }

  /// Take (removing) the completion callbacks queued for `node_id`, called by
  /// the paint walk when it reaches the node.
  pub fn take_node_captures(&self, node_id: u64) -> Vec<CaptureDone> {
    self.capture_requests.borrow_mut().remove(&node_id).unwrap_or_default()
  }

  /// Record a serviced capture's outcome for delivery at the end of the paint
  /// pass (see `deliver_captures`), rather than invoking the callback mid-walk.
  pub fn complete_capture(&self, done: CaptureDone, result: Result<CaptureInfo, String>) {
    self.capture_ready.borrow_mut().push((done, result));
  }

  /// Fail every still-queued request: the paint walk finished without visiting
  /// their nodes, so they are not in the live tree. Called at end of paint.
  pub fn fail_unserviced_captures(&self) {
    let leftover = std::mem::take(&mut *self.capture_requests.borrow_mut());
    for done in leftover.into_values().flatten() {
      self.complete_capture(done, Err("capture node is not in the live render tree".to_string()));
    }
  }

  /// Invoke every serviced capture's completion callback with its outcome.
  /// Called once at the end of the paint pass, out of the tree walk, so a
  /// callback (which may read back or free textures) never re-enters the walk.
  pub fn deliver_captures(&self) {
    let ready = std::mem::take(&mut *self.capture_ready.borrow_mut());
    for (done, result) in ready {
      done(result);
    }
  }

  /// Hand the frame's display list to the raster thread, which draws and
  /// presents it (or reads it back in playback mode) and then notifies the
  /// main loop. Returns immediately; the UI thread is free to build the next
  /// frame while this one is on the GPU. Err means the raster thread is gone
  /// and the engine should shut down.
  pub fn submit(&self, dl: DisplayList) -> Result<(), ()> {
    self.raster_tx.send(RasterCmd::Frame { dl, tree_clean: false }).map_err(|_| ())
  }

  /// Submit a frame whose display list is the same list as the previous
  /// submit, unchanged (the present-only reuse path). While a window shader
  /// is active the raster side may then skip re-rasterizing the tree and run
  /// only the shader pass over the retained layer - unless content-bearing
  /// commands (texture uploads, target renders) arrived since the last
  /// resolve; it tracks that itself. Never send this for a rebuilt list:
  /// a wrong clean claim presents a stale frame.
  pub fn submit_clean(&self, dl: DisplayList) -> Result<(), ()> {
    self.raster_tx.send(RasterCmd::Frame { dl, tree_clean: true }).map_err(|_| ())
  }

  /// Rebind the raster thread's context to the window's current EGL surface.
  /// Call on return-to-visible, before the resume repaint's frame: Android
  /// destroys the surface on background and the stale binding would fail the
  /// next present with EGL_BAD_SURFACE. Ordered ahead of that frame on the
  /// command channel; harmless when the surface never changed.
  pub fn rebind_window_surface(&self) {
    self.send(RasterCmd::RebindWindowSurface);
  }

  pub fn get_or_create_texture(&self, id: u64, size: ISize, make_pixels: impl FnOnce() -> Vec<u8>) -> Rc<TextureEntry> {
    if self.textures.get(id).is_none() {
      let pixels = make_pixels();
      self.create_texture_at(id, size.width as u32, size.height as u32, &pixels, SamplerState::default());
    }
    self.textures.get(id).expect("texture must exist after insert")
  }

  pub fn get_or_update_texture(&self, id: u64, size: ISize, make_pixels: impl FnOnce() -> Vec<u8>) -> Rc<TextureEntry> {
    let pixels = make_pixels();
    if self.textures.get(id).is_none() {
      self.create_texture_at(id, size.width as u32, size.height as u32, &pixels, SamplerState::default());
    } else if let Err(e) = self.update_texture(id, &pixels, 0) {
      log::warn!("[alloy] texture {id} update failed: {e}");
    }
    self.textures.get(id).expect("texture must exist after insert or update")
  }

  /// Create a sampleable texture from RGBA8 pixels and adopt into Impeller,
  /// with the given sampling (how every consumer - shader passes and
  /// `<texture>` display - samples it). Returns the registry id assigned to
  /// the new texture.
  pub fn create_texture_from_pixels(&self, width: u32, height: u32, pixels: &[u8], sampler: SamplerState) -> u64 {
    let id = self.textures.allocate_id();
    self.create_texture_at(id, width, height, pixels, sampler);
    id
  }

  /// Create (or replace) the texture stored at `id`, e.g. to resize a stream
  /// texture without invalidating the id handed out to consumers. Lookups pick
  /// up the new texture immediately; in-flight users of the old entry keep it
  /// alive until released.
  pub fn create_texture_at(&self, id: u64, width: u32, height: u32, pixels: &[u8], sampler: SamplerState) {
    let impeller = self
      .rpc(|reply| RasterCmd::CreateTexture { id, width, height, pixels: pixels.to_vec(), sampler, reply })
      .and_then(std::convert::identity)
      .expect("adopt texture failed");
    self.textures.insert(id, TextureEntry { impeller, width, height, sampler });
  }

  /// Re-upload RGBA8 pixels into an existing texture. `pixels` may be a larger
  /// buffer holding multiple frames; `offset` selects the frame start. The
  /// frame must match the texture's dimensions exactly.
  pub fn update_texture(&self, id: u64, pixels: &[u8], offset: usize) -> Result<(), String> {
    let entry = self.textures.get(id).ok_or_else(|| format!("texture {id} not found"))?;
    let (width, height) = (entry.width(), entry.height());
    let frame_size = (width as usize) * (height as usize) * 4;
    let end = offset.checked_add(frame_size).ok_or_else(|| "offset overflow".to_string())?;
    if end > pixels.len() {
      return Err(format!(
        "need {frame_size} bytes at offset {offset} for {width}x{height}, buffer has {}",
        pixels.len()
      ));
    }
    self.send(RasterCmd::UpdateTexture { id, pixels: pixels[offset..end].to_vec() });
    Ok(())
  }

  /// Replace a registered pixel texture with one of a new size at the same id
  /// (an id-stable resize): lookups and shader sampler bindings pick up the
  /// new texture immediately (shaders sampling it re-render), in-flight users
  /// of the old entry keep it alive until released. `pixels` seeds the new
  /// contents and must hold at least one width*height*4 frame. Rejects
  /// shader/pipeline target ids - resize those with `resize_shader_texture`,
  /// which carries the compiled program along. The caller must request a
  /// frame.
  pub fn resize_texture(&self, id: u64, width: u32, height: u32, pixels: &[u8]) -> Result<(), String> {
    if self.textures.get(id).is_none() {
      return Err(format!("texture {id} not found"));
    }
    if self.shader_kinds.borrow().contains_key(&id) {
      return Err(format!("texture {id} is a shader target; use resize_shader_texture"));
    }
    let frame_size = (width as usize) * (height as usize) * 4;
    if pixels.len() < frame_size {
      return Err(format!("need {frame_size} bytes for {width}x{height}, buffer has {}", pixels.len()));
    }
    // Sampling is a property of the id and survives the id-stable resize.
    let sampler = self.textures.get(id).map(|e| e.sampler()).unwrap_or_default();
    self.create_texture_at(id, width, height, &pixels[..frame_size], sampler);
    Ok(())
  }

  /// Recreate a shader/pipeline target at a new size under the same id: the
  /// compiled program, sampler bindings, last-applied params, and draw state
  /// carry over, and the output re-renders at the new size at the next dirty
  /// flush. Lookups pick up the new target right away; in-flight users of
  /// the old one keep it alive until released. The caller must request a
  /// frame.
  pub fn resize_shader_texture(&self, id: u64, width: u32, height: u32) -> Result<(), String> {
    if !self.shader_kinds.borrow().contains_key(&id) {
      return Err(format!("shader texture {id} not found"));
    }
    let impeller = self.rpc(|reply| RasterCmd::ResizeShaderTexture { id, width, height, reply })??;
    let sampler = self.textures.get(id).map(|e| e.sampler()).unwrap_or_default();
    self.textures.insert(id, TextureEntry { impeller, width, height, sampler });
    Ok(())
  }

  /// Compile a GLSL ES fragment shader into a new RGBA8 target texture and
  /// register the output in the texture registry. Returns the id the output
  /// is sampleable under (usable anywhere a normal texture id is); the first
  /// render happens at the raster thread's next dirty flush, before anything
  /// observes the pixels. The compiled program is retained so
  /// update_shader_params can re-render the same texture without recompiling
  /// or re-adopting.
  pub fn create_shader_texture(
    &self,
    width: u32,
    height: u32,
    fragment_src: &str,
    params: &[(String, ParamValue)],
    textures: &[(String, u64)],
    sampler: SamplerState,
  ) -> Result<u64, String> {
    let id = self.textures.allocate_id();
    let impeller = self.rpc(|reply| RasterCmd::CreateShaderTexture {
      id,
      width,
      height,
      fragment_src: fragment_src.to_string(),
      params: params.to_vec(),
      textures: textures.to_vec(),
      sampler,
      reply,
    })??;
    self.textures.insert(id, TextureEntry { impeller, width, height, sampler });
    self.shader_kinds.borrow_mut().insert(id, false);
    self.shader_sources.borrow_mut().insert(id, textures.iter().cloned().collect());
    Ok(id)
  }

  /// Update an existing shader texture's params; it re-renders (sampler
  /// inputs re-resolved) at the raster thread's next dirty flush, as do any
  /// targets sampling it, transitively. The output keeps its id and Impeller
  /// texture (no re-adoption); only the GL contents change, so the caller
  /// must request a frame for the new pixels to reach the screen.
  pub fn update_shader_params(&self, id: u64, params: &[(String, ParamValue)]) -> Result<(), String> {
    if !self.shader_kinds.borrow().contains_key(&id) {
      return Err(format!("shader texture {id} not found"));
    }
    self.send(RasterCmd::UpdateShaderParams { id, params: params.to_vec() });
    Ok(())
  }

  /// Rebind an existing shader texture's sampler2D inputs by uniform name;
  /// bindings not named keep their current source. The target re-renders
  /// against the new sources at the raster thread's next dirty flush, and
  /// keeps its id and Impeller texture (no re-adoption), so the caller must
  /// request a frame, same as `update_shader_params`. Errors if the shader or
  /// any source texture id is unknown, or a binding would create a sampling
  /// cycle among targets (self-binding is the length-1 case; a cycle is a GL
  /// feedback loop and would break re-render propagation). An unknown
  /// uniform name is reported by the raster thread as a warning, leaving all
  /// bindings unchanged.
  pub fn update_shader_textures(&self, id: u64, textures: &[(String, u64)]) -> Result<(), String> {
    if !self.shader_kinds.borrow().contains_key(&id) {
      return Err(format!("shader texture {id} not found"));
    }
    {
      let sources = self.shader_sources.borrow();
      for (name, src_id) in textures {
        if self.textures.get(*src_id).is_none() {
          return Err(format!("texture {src_id} (sampler '{name}') not found"));
        }
        // The current graph is acyclic, and this call only changes `id`'s own
        // outgoing edges, so any new cycle runs through one of the updated
        // bindings: per binding, reject if the target can already be reached
        // from the new source. The walk never needs `id`'s own edges (it
        // stops on reaching `id`), so the pre-update graph is the right one.
        if samples_transitively(&sources, *src_id, id) {
          return Err(format!("sampler '{name}' would create a sampling cycle back to shader texture {id}"));
        }
      }
    }
    let mut sources = self.shader_sources.borrow_mut();
    let entry = sources.entry(id).or_default();
    for (name, src_id) in textures {
      entry.insert(name.clone(), *src_id);
    }
    drop(sources);
    self.send(RasterCmd::UpdateShaderTextures { id, textures: textures.to_vec() });
    Ok(())
  }

  /// Create an interleaved vertex buffer from raw bytes, returning its id.
  /// Buffer ids are their own space (not texture ids); pipelines reference the
  /// buffer via `PipelineSpec::buffer_id`.
  pub fn create_gpu_buffer(&self, data: &[u8]) -> Result<u64, String> {
    let id = self.next_buffer_id.get();
    self.rpc(|reply| RasterCmd::CreateBuffer { id, data: data.to_vec(), reply })??;
    self.next_buffer_id.set(id + 1);
    self.buffer_sizes.borrow_mut().insert(id, data.len());
    Ok(id)
  }

  /// Overwrite part of a vertex buffer (`data` at `byte_offset`, within the
  /// buffer's original size); every pipeline drawing from it re-renders with
  /// its last-applied params at the next dirty flush, so geometry-only
  /// changes reach the screen even when no new params arrive. The caller
  /// must request a frame.
  pub fn write_gpu_buffer(&self, id: u64, data: &[u8], byte_offset: usize) -> Result<(), String> {
    let size = *self.buffer_sizes.borrow().get(&id).ok_or_else(|| format!("buffer {id} not found"))?;
    let end = byte_offset.checked_add(data.len()).ok_or_else(|| "offset overflow".to_string())?;
    if end > size {
      return Err(format!("write of {} bytes at offset {byte_offset} exceeds buffer size {size}", data.len()));
    }
    self.send(RasterCmd::WriteBuffer { id, data: data.to_vec(), byte_offset });
    Ok(())
  }

  /// Free a vertex buffer. Destroy pipelines drawing from it first: the VAO
  /// reference keeps the GL storage alive so they keep rendering stale
  /// geometry, but further writes to the id error.
  pub fn destroy_gpu_buffer(&self, id: u64) {
    self.buffer_sizes.borrow_mut().remove(&id);
    self.send(RasterCmd::DestroyBuffer { id });
  }

  /// Compile a vertex+fragment pipeline, render it once into a new RGBA8
  /// target texture, and register the output exactly like
  /// `create_shader_texture` (same id space; `update_shader_params`,
  /// `destroy_texture`, and `<texture src>` all apply). The fused convenience
  /// over `create_render_pipeline` + `create_shader_target`; the anonymous
  /// program and pipeline die with the target.
  pub fn create_pipeline_texture(&self, spec: PipelineSpec) -> Result<u64, String> {
    let id = self.textures.allocate_id();
    let (width, height, sampler) = (spec.target.width, spec.target.height, spec.target.sampler);
    let sources: HashMap<String, u64> = spec.target.textures.iter().cloned().collect();
    let impeller = self.rpc(|reply| RasterCmd::CreatePipelineTexture { id, spec, reply })??;
    self.textures.insert(id, TextureEntry { impeller, width, height, sampler });
    self.shader_kinds.borrow_mut().insert(id, true);
    self.shader_sources.borrow_mut().insert(id, sources);
    Ok(id)
  }

  /// Compile a single raw shader stage, returning its stage id (its own id
  /// space). The source is complete GLSL ES unless `header` explicitly asks
  /// for the standard header (see `gpu::compile_stage`). Compile errors
  /// surface here, synchronously, at a call site the app chose. Free with
  /// `destroy_shader_stage` - safe right after linking.
  pub fn compile_shader_stage(
    &self,
    stage: ShaderStage,
    source: &str,
    header: bool,
  ) -> Result<u64, String> {
    let id = self.next_stage_id.get();
    self.rpc(|reply| RasterCmd::CompileStage { id, stage, source: source.to_string(), header, reply })??;
    self.next_stage_id.set(id + 1);
    self.stage_kinds.borrow_mut().insert(id, stage);
    Ok(id)
  }

  /// Link a compiled vertex and fragment stage into a shared program,
  /// returning its program id (a separate id space from textures, like
  /// buffers). The program backs any number of targets via
  /// `create_shader_target`, is freed with `destroy_shader_program`, and link
  /// errors surface here, synchronously. The stages remain usable for further
  /// links.
  pub fn link_shader_program(&self, vertex: u64, fragment: u64) -> Result<u64, String> {
    let kinds = self.stage_kinds.borrow();
    match kinds.get(&vertex) {
      None => return Err(format!("shader {vertex} not found")),
      Some(ShaderStage::Vertex) => {}
      Some(s) => return Err(format!("shader {vertex} is a {} stage, expected vertex", s.name())),
    }
    match kinds.get(&fragment) {
      None => return Err(format!("shader {fragment} not found")),
      Some(ShaderStage::Fragment) => {}
      Some(s) => return Err(format!("shader {fragment} is a {} stage, expected fragment", s.name())),
    }
    drop(kinds);
    let id = self.next_program_id.get();
    self.rpc(|reply| RasterCmd::LinkProgram { id, vertex, fragment, reply })??;
    self.next_program_id.set(id + 1);
    self.program_ids.borrow_mut().insert(id);
    Ok(id)
  }

  /// Delete a compiled stage and retire its id. Programs linked from it are
  /// unaffected: a linked program keeps its own compiled copies.
  pub fn destroy_shader_stage(&self, id: u64) {
    self.stage_kinds.borrow_mut().remove(&id);
    self.send(RasterCmd::DestroyStage { id });
  }

  /// Pair a program from `link_shader_program` with draw state, returning the
  /// pipeline id (its own id space, like programs and buffers). The pipeline
  /// is the draw-state object every target created from it shares; creating
  /// one compiles nothing. Free with `destroy_render_pipeline`.
  pub fn create_render_pipeline(&self, program: u64, desc: PipelineDesc) -> Result<u64, String> {
    if !self.program_ids.borrow().contains(&program) {
      return Err(format!("program {program} not found"));
    }
    let id = self.next_pipeline_id.get();
    self.rpc(|reply| RasterCmd::CreateRenderPipeline { id, program, desc, reply })??;
    self.next_pipeline_id.set(id + 1);
    self.pipeline_ids.borrow_mut().insert(id);
    Ok(id)
  }

  /// Drop a shared pipeline's registry entry and retire its id. Targets
  /// created from it keep rendering - they hold the pipeline until they are
  /// destroyed - so either destruction order is safe. The program it was
  /// created from is yours and unaffected.
  pub fn destroy_render_pipeline(&self, id: u64) {
    self.pipeline_ids.borrow_mut().remove(&id);
    self.send(RasterCmd::DestroyRenderPipeline { id });
  }

  /// Create a render target over a pipeline from `create_render_pipeline` and
  /// register the output exactly like `create_shader_texture` (same texture
  /// id space: params updates, `setShaderSize`, `<texture src>` and
  /// `destroy_texture` all apply). Many targets may share one pipeline, and
  /// creating a target compiles nothing.
  pub fn create_shader_target(&self, pipeline: u64, spec: TargetSpec) -> Result<u64, String> {
    if !self.pipeline_ids.borrow().contains(&pipeline) {
      return Err(format!("pipeline {pipeline} not found"));
    }
    let id = self.textures.allocate_id();
    let (width, height, sampler) = (spec.width, spec.height, spec.sampler);
    let sources: HashMap<String, u64> = spec.textures.iter().cloned().collect();
    let impeller = self.rpc(|reply| RasterCmd::CreateShaderTarget { id, pipeline, spec, reply })??;
    self.textures.insert(id, TextureEntry { impeller, width, height, sampler });
    self.shader_kinds.borrow_mut().insert(id, true);
    self.shader_sources.borrow_mut().insert(id, sources);
    Ok(id)
  }

  /// Drop a shared program's registry entry and retire its id. Pipelines
  /// created from it keep rendering - they hold the program until they are
  /// destroyed - and the GL program is deleted once the last user is gone, so
  /// either destruction order is safe.
  pub fn destroy_shader_program(&self, id: u64) {
    self.program_ids.borrow_mut().remove(&id);
    self.send(RasterCmd::DestroyProgram { id });
  }

  /// Declare (or clear, with None) the window shader: the frame then resolves
  /// into the runtime-owned layer and `shader.program` draws over it into the
  /// window as the last step before present (see `WindowShader`). Fire-and-
  /// forget on the ordered frame channel, so a change lands cleanly between
  /// two frames; the raster thread holds the program while declared, so
  /// destroying its handle keeps the effect running until it is re-declared
  /// or cleared. The caller must request a frame. Errs on an unknown program
  /// handle.
  pub fn set_window_shader(&self, shader: Option<WindowShader>) -> Result<(), String> {
    if let Some(ws) = &shader {
      if !self.program_ids.borrow().contains(&ws.program) {
        return Err(format!("program {} not found", ws.program));
      }
    }
    self.send(RasterCmd::SetWindowShader { shader });
    Ok(())
  }

  /// Set a pipeline texture's vertex draw count and re-render it with its
  /// last-applied params. The caller must request a frame.
  pub fn set_draw_count(&self, id: u64, count: i32) -> Result<(), String> {
    let kind = self.shader_kinds.borrow().get(&id).copied();
    match kind {
      None => Err(format!("shader texture {id} not found")),
      Some(false) => Err("not a pipeline texture".to_string()),
      Some(true) => {
        self.send(RasterCmd::SetDrawCount { id, count });
        Ok(())
      }
    }
  }

  /// Inventory the GPU resources the raster thread tracks: registered
  /// textures, vertex buffers, and shader/pipeline targets with their
  /// bookkeeping (draw state, layout, bindings, current params - the most
  /// recent writes, which the next flush renders with). Sorted by id for
  /// stable output.
  pub fn gpu_resources(&self) -> GpuResources {
    self.rpc(|reply| RasterCmd::Resources { reply }).unwrap_or_else(|_| GpuResources {
      textures: Vec::new(),
      buffers: Vec::new(),
      pipelines: Vec::new(),
      render_pipelines: Vec::new(),
      programs: Vec::new(),
      window_shader: None,
    })
  }

  /// Read back part of a vertex buffer's contents by registry id.
  pub fn read_gpu_buffer(&self, id: u64, byte_offset: usize, len: usize) -> Result<Vec<u8>, String> {
    if !self.buffer_sizes.borrow().contains_key(&id) {
      return Err(format!("buffer {id} not found"));
    }
    self.rpc(|reply| RasterCmd::ReadBuffer { id, byte_offset, len, reply })?
  }

  /// Byte length of a vertex buffer by registry id.
  pub fn gpu_buffer_len(&self, id: u64) -> Result<usize, String> {
    self.buffer_sizes.borrow().get(&id).copied().ok_or_else(|| format!("buffer {id} not found"))
  }

  /// Rasterize a display list into a new GPU texture of the given pixel size,
  /// ready for sampling. The texture is owned by Impeller (and the caller's
  /// handle), not by the registry. `aa: false` renders single-sample (no
  /// coverage AA), for boundaries that opted out.
  pub fn render_display_list_to_texture(
    &self,
    dl: &DisplayList,
    width: u32,
    height: u32,
    aa: bool,
  ) -> Result<Texture, String> {
    self.rpc(|reply| RasterCmd::RasterizeDl { dl: dl.clone(), width, height, aa, reply })?
  }

  /// Re-rasterize a display list into an existing texture from
  /// `render_display_list_to_texture`, reusing its storage (invalidated
  /// snapshot boundaries re-render this way instead of reallocating). The
  /// caller must ensure the texture's 64px-aligned backing allocation fits
  /// `width` x `height`.
  pub fn render_display_list_into_texture(
    &self,
    dl: &DisplayList,
    texture: &Texture,
    width: u32,
    height: u32,
    aa: bool,
  ) -> Result<(), String> {
    self
      .rpc(|reply| RasterCmd::RasterizeDlInto { dl: dl.clone(), texture: texture.clone(), width, height, aa, reply })?
  }

  /// Rasterize a display list into a new *registered* texture cropped to
  /// exactly `width` x `height`, returning its registry id. The raster thread
  /// rasterizes and reads back in one trip: `render_display_list_to_texture`
  /// over-allocates the render target to a 64px tile boundary (an Android
  /// requirement), but the content sits at the origin, so reading back only
  /// `width` x `height` yields the tightly-packed content with the padding
  /// excluded. The re-uploaded texture is therefore unpadded, so
  /// `read_texture_by_id` (and any `<texture src>` sampling) sees exact
  /// dimensions with no origin-specific knowledge. The intermediate padded
  /// texture never leaves the raster thread.
  pub fn capture_node_texture(&self, dl: &DisplayList, width: u32, height: u32) -> Result<u64, String> {
    let pixels = self.rpc(|reply| RasterCmd::RasterizeReadback { dl: dl.clone(), width, height, reply })??;
    Ok(self.create_texture_from_pixels(width, height, &pixels, SamplerState::default()))
  }

  /// Read back a registered texture's RGBA8 pixels by id, using the entry's
  /// own dimensions. Errors if the id is not in the registry.
  pub fn read_texture_by_id(&self, id: u64) -> Result<(u32, u32, Vec<u8>), String> {
    let entry = self.textures.get(id).ok_or_else(|| format!("texture {id} not found"))?;
    let (width, height) = (entry.width(), entry.height());
    let pixels = self.read_texture(&entry.impeller, width, height)?;
    Ok((width, height, pixels))
  }

  /// Read back a texture's RGBA8 pixels (tightly packed top-to-bottom rows).
  pub fn read_texture(&self, texture: &Texture, width: u32, height: u32) -> Result<Vec<u8>, String> {
    self.rpc(|reply| RasterCmd::ReadTexture { texture: texture.clone(), width, height, reply })?
  }

  /// Free a texture created via `create_texture_from_pixels`, `create_texture_at`,
  /// or `create_shader_texture`. Deferred, not immediate: the id is queued and
  /// actually reclaimed by `reclaim_destroyed` (run by the paint loop) once the
  /// live render tree no longer references it. Deferral makes the natural app
  /// pattern safe - destroy the old id in the same update that repoints
  /// `<texture src>` at its replacement - regardless of how the reactive flush
  /// interleaves with frames: any frame built before the swap lands still finds
  /// the entry and paints the old content instead of a blank. Until
  /// reclamation the id stays fully usable; afterwards the registry entry and
  /// raster-side resources (for shaders: GL program and FBO) are gone, while
  /// in-flight display lists keep the Impeller texture alive until they drop.
  pub fn destroy_texture(&self, id: u64) {
    let mut pending = self.pending_destroys.borrow_mut();
    if !pending.contains(&id) {
      pending.push(id);
    }
  }

  /// Whether any destroy is awaiting reclamation, so the paint loop can skip
  /// the tree scan entirely in the common no-destroys case.
  pub fn has_pending_destroys(&self) -> bool {
    !self.pending_destroys.borrow().is_empty()
  }

  /// Reclaim every pending destroy whose id is not in `referenced` (the ids
  /// the live render tree currently references, see
  /// `RenderTree::referenced_texture_ids`). Still-referenced ids stay queued -
  /// and stay alive - until a later sweep finds them unreferenced, so a
  /// destroyed-but-still-mounted texture keeps drawing rather than glitching
  /// to blank. Called by the paint loop after each painted frame.
  pub fn reclaim_destroyed(&self, referenced: &HashSet<u64>) {
    let mut pending = self.pending_destroys.borrow_mut();
    pending.retain(|&id| {
      if referenced.contains(&id) {
        return true;
      }
      self.textures.remove(id);
      self.shader_kinds.borrow_mut().remove(&id);
      self.shader_sources.borrow_mut().remove(&id);
      self.send(RasterCmd::DestroyTexture { id });
      false
    });
  }
}

/// Whether `to` is reachable from `from` (inclusive: `from == to` is a hit)
/// by following sampler edges in `sources` (target id -> its source id per
/// uniform name): the sampling-cycle test behind `update_shader_textures`.
/// Pure over the id graph, so it unit-tests without a Context.
pub(crate) fn samples_transitively(sources: &HashMap<u64, HashMap<String, u64>>, from: u64, to: u64) -> bool {
  let mut stack = vec![from];
  let mut visited: HashSet<u64> = HashSet::new();
  while let Some(node) = stack.pop() {
    if node == to {
      return true;
    }
    if visited.insert(node) {
      if let Some(srcs) = sources.get(&node) {
        stack.extend(srcs.values().copied());
      }
    }
  }
  false
}
