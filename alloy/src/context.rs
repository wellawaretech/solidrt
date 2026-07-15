use glow::HasContext;
use impellers::{Context as ImpellerContext, DisplayList, ISize, Texture};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::mpsc;

use crate::audio::AudioRegistry;
use crate::backend::{Backend, Frame, GpuFence};
use crate::camera::CameraRegistry;
use crate::gl;
use crate::microphone::MicrophoneRegistry;
use crate::texture::{GpuTexture, TextureEntry, TextureRegistry};

pub struct Context {
  backend: Backend,
  // GLES bindings + the Impeller context. Both are tied to the UI thread's GL
  // context and only ever touched from the UI thread. RefCell on impeller_ctx
  // because wrap_fbo needs &mut while Context is shared behind Arc.
  gl: glow::Context,
  impeller_ctx: RefCell<ImpellerContext>,
  pub textures: TextureRegistry,
  // Compiled shader targets (fullscreen fragment passes and vertex+fragment
  // pipelines), keyed by the texture id their output is registered under, so
  // update_shader_params can re-render into the same id.
  shaders: RefCell<HashMap<u64, crate::shader::ShaderTexture>>,
  // Vertex buffers pipelines draw from, in their own id space (not texture
  // ids). A write re-renders every pipeline referencing the buffer.
  buffers: RefCell<HashMap<u64, crate::shader::GpuBuffer>>,
  next_buffer_id: std::cell::Cell<u64>,
  pub(crate) cameras: CameraRegistry,
  pub(crate) microphones: MicrophoneRegistry,
  pub(crate) audio: AudioRegistry,
  tx: mpsc::Sender<Frame>,
  // Wakes the main thread's event wait after a frame is queued, so a submitted
  // frame presents immediately instead of at the next wait timeout. None in
  // playback mode, whose capture loop blocks on the channel directly.
  wake: Option<Box<dyn Fn() + Send + Sync>>,
  // On-demand node captures (captureSnapshot, dev-server snapshot). Requests are
  // keyed by node id (many requests may target one node) and drained by the
  // paint walk when it visits that node; each carries its own completion
  // callback. Serviced outcomes wait in `capture_ready` and are delivered once,
  // after the walk, by `deliver_captures`. The callback is a plain `FnOnce` - no
  // engine types - so each consumer (a JS-promise settler, a dev-server reply)
  // supplies its own and the rendertree stays engine-independent.
  capture_requests: RefCell<HashMap<u64, Vec<CaptureDone>>>,
  capture_ready: RefCell<Vec<(CaptureDone, Result<CaptureInfo, String>)>>,
}

/// Everything `create_pipeline_texture` needs to build a vertex+fragment
/// pipeline target. `attributes` is (name, format) with the string formats
/// `AttrFormat::parse` accepts, describing one interleaved vertex in the
/// buffer `buffer_id` (0 = attributeless rendering via gl_VertexID). A
/// negative `draw_count` derives the count from buffer size / vertex stride.
pub struct PipelineSpec<'a> {
  pub width: u32,
  pub height: u32,
  pub vertex_src: &'a str,
  pub fragment_src: &'a str,
  pub params: &'a [(String, f32)],
  pub textures: &'a [(String, u64)],
  pub attributes: &'a [(String, String)],
  pub buffer_id: u64,
  pub topology: &'a str,
  pub draw_count: i32,
  pub depth: bool,
  pub clear_color: [f32; 4],
}

/// A point-in-time inventory of the Context's GPU bookkeeping, for resource
/// introspection (the dev server's gpu query). Plain data only, so consumers
/// stay free of GL types.
pub struct GpuResources {
  pub textures: Vec<GpuTextureInfo>,
  pub buffers: Vec<GpuBufferInfo>,
  pub pipelines: Vec<GpuPipelineInfo>,
}

pub struct GpuTextureInfo {
  pub id: u64,
  pub width: u32,
  pub height: u32,
  /// A shader or pipeline renders into this texture (vs a sampled upload).
  pub target: bool,
}

pub struct GpuBufferInfo {
  pub id: u64,
  pub byte_length: usize,
}

pub struct GpuPipelineInfo {
  /// The registry id its output texture is sampleable under.
  pub texture_id: u64,
  /// "pipeline" (vertex+fragment over a buffer) or "fragment" (fullscreen pass).
  pub kind: &'static str,
  pub buffer_id: Option<u64>,
  pub topology: Option<&'static str>,
  pub draw_count: Option<i32>,
  pub depth: bool,
  /// (name, format string) of the declared interleaved vertex layout.
  pub attributes: Vec<(String, String)>,
  /// sampler2D uniform name -> source texture id.
  pub textures: Vec<(String, u64)>,
  /// The float uniforms applied on the most recent render.
  pub params: Vec<(String, f32)>,
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

// Safety: Context is asserted Send + Sync but is only ever accessed from the UI
// thread, where its GL context is current. glow::Context and Impeller::Context
// both rely on that thread-local GL state; we never touch them concurrently.
unsafe impl Send for Context {}
unsafe impl Sync for Context {}

impl Context {
  pub fn new(
    backend: Backend,
    gl: glow::Context,
    impeller_ctx: ImpellerContext,
    tx: mpsc::Sender<Frame>,
    wake: Option<Box<dyn Fn() + Send + Sync>>,
  ) -> Self {
    Context {
      backend,
      gl,
      impeller_ctx: RefCell::new(impeller_ctx),
      textures: TextureRegistry::new(),
      shaders: RefCell::new(HashMap::new()),
      buffers: RefCell::new(HashMap::new()),
      next_buffer_id: std::cell::Cell::new(1),
      cameras: CameraRegistry::default(),
      microphones: MicrophoneRegistry::default(),
      audio: AudioRegistry::default(),
      tx,
      wake,
      capture_requests: RefCell::new(HashMap::new()),
      capture_ready: RefCell::new(Vec::new()),
    }
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

  pub fn submit(&self, dl: DisplayList) -> Result<(), ()> {
    // Fence every bit of GPU work this UI-thread frame queued (shader renders,
    // texture uploads, offscreen draws) so the render thread can order its
    // sampling after that work on the GPU timeline, instead of the UI thread
    // blocking on glFinish. A sync object is only waitable from the render
    // context once the producing context has flushed it.
    let fence = unsafe { self.gl.fence_sync(glow::SYNC_GPU_COMMANDS_COMPLETE, 0) }.ok().map(GpuFence);
    unsafe { self.gl.flush() };
    self.tx.send(Frame { dl, fence }).map_err(|_| ())?;
    // Wake only after the frame is in the channel, so the woken loop finds it.
    if let Some(wake) = &self.wake {
      wake();
    }
    Ok(())
  }

  pub fn get_or_create_texture(&self, id: u64, size: ISize, make_pixels: impl FnOnce() -> Vec<u8>) -> Rc<TextureEntry> {
    if self.textures.get(id).is_none() {
      let pixels = make_pixels();
      let gpu = GpuTexture::new(&self.gl, self.backend, size);
      gpu.upload(&self.gl, &pixels, size);
      let impeller = self.adopt_texture(&gpu, size).expect("adopt texture failed");
      self.textures.insert(id, TextureEntry { gpu, impeller });
    }
    self.textures.get(id).expect("texture must exist after insert")
  }

  pub fn get_or_update_texture(&self, id: u64, size: ISize, make_pixels: impl FnOnce() -> Vec<u8>) -> Rc<TextureEntry> {
    let pixels = make_pixels();
    if self.textures.get(id).is_none() {
      let gpu = GpuTexture::new(&self.gl, self.backend, size);
      gpu.upload(&self.gl, &pixels, size);
      let impeller = self.adopt_texture(&gpu, size).expect("adopt texture failed");
      self.textures.insert(id, TextureEntry { gpu, impeller });
    } else {
      let entry = self.textures.get(id).expect("texture must exist in else branch");
      entry.gpu.upload(&self.gl, &pixels, size);
    }
    self.textures.get(id).expect("texture must exist after insert or update")
  }

  /// Create a sampleable texture from RGBA8 pixels and adopt into Impeller.
  /// Returns the registry id assigned to the new texture.
  pub fn create_texture_from_pixels(&self, width: u32, height: u32, pixels: &[u8]) -> u64 {
    let id = self.textures.allocate_id();
    self.create_texture_at(id, width, height, pixels);
    id
  }

  /// Create (or replace) the texture stored at `id`, e.g. to resize a stream
  /// texture without invalidating the id handed out to consumers. Lookups pick
  /// up the new texture immediately; in-flight users of the old entry keep it
  /// alive until released.
  pub fn create_texture_at(&self, id: u64, width: u32, height: u32, pixels: &[u8]) {
    let size = ISize::new(width as i64, height as i64);
    let gpu = GpuTexture::new(&self.gl, self.backend, size);
    gpu.upload(&self.gl, pixels, size);
    let impeller = self.adopt_texture(&gpu, size).expect("adopt texture failed");
    self.textures.insert(id, TextureEntry { gpu, impeller });
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
    let size = ISize::new(width as i64, height as i64);
    entry.gpu.upload(&self.gl, &pixels[offset..end], size);
    // Shader targets sampling this texture show stale output until their next
    // params update; re-render them now (same contract as write_gpu_buffer, so
    // data-texture changes are visible without a params change).
    let shaders = self.shaders.borrow();
    for shader in shaders.values() {
      if shader.sampler_bindings().iter().any(|(_, tex)| *tex == id) {
        let resolved = self.resolve_sampler_bindings(shader);
        shader.render(&self.gl, &shader.last_params(), &resolved);
      }
    }
    Ok(())
  }

  /// Compile a GLSL ES fragment shader, render it once into a new RGBA8 target
  /// texture, and register the output in the texture registry. Returns the id
  /// the output is sampleable under (usable anywhere a normal texture id is).
  /// The compiled program is retained so update_shader_params can re-render the
  /// same texture without recompiling or re-adopting.
  pub fn create_shader_texture(
    &self,
    width: u32,
    height: u32,
    fragment_src: &str,
    params: &[(String, f32)],
    textures: &[(String, u64)],
  ) -> Result<u64, String> {
    let shader = crate::shader::ShaderTexture::new(&self.gl, width, height, fragment_src, textures.to_vec())?;
    let resolved = self.resolve_sampler_bindings(&shader);
    shader.render(&self.gl, params, &resolved);

    let size = ISize::new(width as i64, height as i64);
    let gpu = GpuTexture { gl_texture: shader.gl_texture(), backend: self.backend, width, height };
    let impeller = self.adopt_texture(&gpu, size).ok_or_else(|| "adopt shader texture failed".to_string())?;

    let id = self.textures.allocate_id();
    self.textures.insert(id, TextureEntry { gpu, impeller });
    self.shaders.borrow_mut().insert(id, shader);
    Ok(id)
  }

  /// Re-render an existing shader texture with new params. The output keeps its
  /// id and Impeller texture (no re-adoption); only the GL contents change, so
  /// the caller must request a frame for the new pixels to reach the screen.
  /// Sampler inputs are re-resolved, so updated source textures are picked up.
  pub fn update_shader_params(&self, id: u64, params: &[(String, f32)]) -> Result<(), String> {
    let shaders = self.shaders.borrow();
    let shader = shaders.get(&id).ok_or_else(|| format!("shader texture {id} not found"))?;
    let resolved = self.resolve_sampler_bindings(shader);
    shader.render(&self.gl, params, &resolved);
    Ok(())
  }

  /// Create an interleaved vertex buffer from raw bytes, returning its id.
  /// Buffer ids are their own space (not texture ids); pipelines reference the
  /// buffer via `PipelineSpec::buffer_id`.
  pub fn create_gpu_buffer(&self, data: &[u8]) -> Result<u64, String> {
    let buffer = crate::shader::GpuBuffer::new(&self.gl, data)?;
    let id = self.next_buffer_id.get();
    self.next_buffer_id.set(id + 1);
    self.buffers.borrow_mut().insert(id, buffer);
    Ok(id)
  }

  /// Overwrite part of a vertex buffer (`data` at `byte_offset`, within the
  /// buffer's original size), then re-render every pipeline drawing from it
  /// with its last-applied params, so geometry-only changes reach the screen
  /// even when no new params arrive. The caller must request a frame.
  pub fn write_gpu_buffer(&self, id: u64, data: &[u8], byte_offset: usize) -> Result<(), String> {
    {
      let buffers = self.buffers.borrow();
      let buffer = buffers.get(&id).ok_or_else(|| format!("buffer {id} not found"))?;
      buffer.write(&self.gl, data, byte_offset)?;
    }
    let shaders = self.shaders.borrow();
    for shader in shaders.values() {
      if shader.buffer_id() == Some(id) {
        let resolved = self.resolve_sampler_bindings(shader);
        shader.render(&self.gl, &shader.last_params(), &resolved);
      }
    }
    Ok(())
  }

  /// Free a vertex buffer. Destroy pipelines drawing from it first: the VAO
  /// reference keeps the GL storage alive so they keep rendering stale
  /// geometry, but further writes to the id error.
  pub fn destroy_gpu_buffer(&self, id: u64) {
    if let Some(buffer) = self.buffers.borrow_mut().remove(&id) {
      buffer.destroy(&self.gl);
    }
  }

  /// Compile a vertex+fragment pipeline, render it once into a new RGBA8
  /// target texture, and register the output exactly like
  /// `create_shader_texture` (same id space; `update_shader_params`,
  /// `destroy_texture`, and `<texture src>` all apply).
  pub fn create_pipeline_texture(&self, spec: &PipelineSpec) -> Result<u64, String> {
    let mut attrs = Vec::with_capacity(spec.attributes.len());
    for (name, fmt) in spec.attributes {
      attrs.push((name.clone(), crate::shader::AttrFormat::parse(fmt)?));
    }
    let topology = crate::shader::parse_topology(spec.topology)?;

    let shader = {
      let buffers = self.buffers.borrow();
      let vbo = if spec.buffer_id != 0 {
        Some(buffers.get(&spec.buffer_id).ok_or_else(|| format!("buffer {} not found", spec.buffer_id))?)
      } else {
        None
      };
      // A negative draw count means "the whole buffer": derived from the
      // buffer size and the interleaved stride.
      let draw_count = if spec.draw_count >= 0 {
        spec.draw_count
      } else {
        let stride = crate::shader::vertex_stride(&attrs);
        match vbo {
          Some(b) if stride > 0 => (b.size / stride as usize) as i32,
          _ => 0,
        }
      };
      crate::shader::ShaderTexture::new_pipeline(
        &self.gl,
        spec.width,
        spec.height,
        spec.vertex_src,
        spec.fragment_src,
        spec.textures.to_vec(),
        &attrs,
        vbo.map(|b| b.vbo),
        spec.buffer_id,
        topology,
        draw_count,
        spec.depth,
        spec.clear_color,
      )?
    };
    let resolved = self.resolve_sampler_bindings(&shader);
    shader.render(&self.gl, spec.params, &resolved);

    let size = ISize::new(spec.width as i64, spec.height as i64);
    let gpu =
      GpuTexture { gl_texture: shader.gl_texture(), backend: self.backend, width: spec.width, height: spec.height };
    let impeller = self.adopt_texture(&gpu, size).ok_or_else(|| "adopt pipeline texture failed".to_string())?;

    let id = self.textures.allocate_id();
    self.textures.insert(id, TextureEntry { gpu, impeller });
    self.shaders.borrow_mut().insert(id, shader);
    Ok(id)
  }

  /// Set a pipeline texture's vertex draw count and re-render it with its
  /// last-applied params. The caller must request a frame.
  pub fn set_draw_count(&self, id: u64, count: i32) -> Result<(), String> {
    let shaders = self.shaders.borrow();
    let shader = shaders.get(&id).ok_or_else(|| format!("shader texture {id} not found"))?;
    shader.set_draw_count(count)?;
    let resolved = self.resolve_sampler_bindings(shader);
    shader.render(&self.gl, &shader.last_params(), &resolved);
    Ok(())
  }

  /// Inventory the GPU resources this Context tracks: registered textures,
  /// vertex buffers, and shader/pipeline targets with their bookkeeping (draw
  /// state, layout, bindings, last-applied params). Sorted by id for stable
  /// output.
  pub fn gpu_resources(&self) -> GpuResources {
    let shaders = self.shaders.borrow();

    let mut textures: Vec<GpuTextureInfo> = self
      .textures
      .list()
      .into_iter()
      .map(|(id, width, height)| GpuTextureInfo { id, width, height, target: shaders.contains_key(&id) })
      .collect();
    textures.sort_by_key(|t| t.id);

    let mut buffers: Vec<GpuBufferInfo> =
      self.buffers.borrow().iter().map(|(id, b)| GpuBufferInfo { id: *id, byte_length: b.size }).collect();
    buffers.sort_by_key(|b| b.id);

    let mut pipelines: Vec<GpuPipelineInfo> = shaders
      .iter()
      .map(|(texture_id, shader)| GpuPipelineInfo {
        texture_id: *texture_id,
        kind: if shader.is_pipeline() { "pipeline" } else { "fragment" },
        buffer_id: shader.buffer_id(),
        topology: shader.topology_name(),
        draw_count: shader.draw_count(),
        depth: shader.has_depth(),
        attributes: shader.attributes().iter().map(|(name, fmt)| (name.clone(), fmt.name().to_string())).collect(),
        textures: shader.sampler_bindings().to_vec(),
        params: shader.last_params(),
      })
      .collect();
    pipelines.sort_by_key(|p| p.texture_id);

    GpuResources { textures, buffers, pipelines }
  }

  /// Read back part of a vertex buffer's contents by registry id.
  pub fn read_gpu_buffer(&self, id: u64, byte_offset: usize, len: usize) -> Result<Vec<u8>, String> {
    let buffers = self.buffers.borrow();
    let buffer = buffers.get(&id).ok_or_else(|| format!("buffer {id} not found"))?;
    buffer.read(&self.gl, byte_offset, len)
  }

  /// Byte length of a vertex buffer by registry id.
  pub fn gpu_buffer_len(&self, id: u64) -> Result<usize, String> {
    let buffers = self.buffers.borrow();
    buffers.get(&id).map(|b| b.size).ok_or_else(|| format!("buffer {id} not found"))
  }

  /// Map a shader's (name -> source texture id) bindings to live GL textures,
  /// dropping any id no longer in the registry (it samples as unbound/black).
  fn resolve_sampler_bindings(&self, shader: &crate::shader::ShaderTexture) -> Vec<(String, glow::Texture)> {
    shader
      .sampler_bindings()
      .iter()
      .filter_map(|(name, src_id)| self.textures.get(*src_id).map(|e| (name.clone(), e.gpu.gl_texture)))
      .collect()
  }

  /// Rasterize a display list into a new GPU texture of the given pixel size,
  /// ready for sampling. The texture is owned by Impeller (and the caller's
  /// handle), not by wgpu and not by the registry.
  pub fn render_display_list_to_texture(&self, dl: &DisplayList, width: u32, height: u32) -> Result<Texture, String> {
    let size = ISize::new(width as i64, height as i64);
    match self.backend {
      Backend::Gl => {
        // A wrapped FBO is treated like a window backbuffer, which GL stores
        // bottom-up; pre-flip the content so the texture ends up upright.
        let mut flipped = impellers::DisplayListBuilder::new(None);
        flipped.translate(0.0, height as f32);
        flipped.scale(1.0, -1.0);
        flipped.draw_display_list(dl, 1.0);
        let flipped = flipped.build().ok_or_else(|| "failed to build flipped display list".to_string())?;
        gl::render_display_list_to_texture(&self.gl, &mut self.impeller_ctx.borrow_mut(), &flipped, size)
      }
      Backend::Vulkan => panic!("Vulkan backend not yet implemented"),
      Backend::Metal => panic!("Metal backend not yet implemented"),
    }
  }

  /// Rasterize a display list into a new *registered* texture cropped to
  /// exactly `width` x `height`, returning its registry id. Composes the
  /// offscreen rasterize + content-extent readback + exact-size upload:
  /// `render_display_list_to_texture` over-allocates the render target to a
  /// 64px tile boundary (an Android requirement), but the content sits at the
  /// origin, so reading back only `width` x `height` yields the tightly-packed
  /// content with the padding excluded. The re-uploaded texture is therefore
  /// unpadded, so `read_texture_by_id` (and any `<texture src>` sampling) sees
  /// exact dimensions with no origin-specific knowledge. The intermediate
  /// padded texture drops here, so Impeller frees its GL name.
  pub fn capture_node_texture(&self, dl: &DisplayList, width: u32, height: u32) -> Result<u64, String> {
    let texture = self.render_display_list_to_texture(dl, width, height)?;
    let pixels = self.read_texture(&texture, width, height)?;
    Ok(self.create_texture_from_pixels(width, height, &pixels))
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
    let size = ISize::new(width as i64, height as i64);
    match self.backend {
      Backend::Gl => gl::read_texture_pixels(&self.gl, texture, size),
      Backend::Vulkan => panic!("Vulkan backend not yet implemented"),
      Backend::Metal => panic!("Metal backend not yet implemented"),
    }
  }

  /// Free a texture created via `create_texture_from_pixels`, `create_texture_at`,
  /// or `create_shader_texture`. Removes the entry from the texture registry so
  /// in-flight display list references keep the texture alive until they drop.
  /// For shader textures also destroys the GL program and FBO.
  pub fn destroy_texture(&self, id: u64) {
    self.textures.remove(id);
    if let Some(shader) = self.shaders.borrow_mut().remove(&id) {
      shader.destroy(&self.gl);
    }
  }

  pub fn adopt_texture(&self, gpu_texture: &GpuTexture, size: ISize) -> Option<Texture> {
    match gpu_texture.backend {
      Backend::Gl => gl::adopt_texture(gpu_texture, &self.impeller_ctx.borrow(), size),
      Backend::Vulkan => {
        panic!("Vulkan backend not yet implemented");
      }
      Backend::Metal => {
        panic!("Metal backend not yet implemented");
      }
    }
  }
}
