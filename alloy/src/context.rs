use impellers::{DisplayList, ISize, Texture};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::mpsc;

use crate::audio::AudioRegistry;
use crate::camera::CameraRegistry;
use crate::microphone::MicrophoneRegistry;
use crate::raster::{PipelineSpecOwned, RasterCmd};
use crate::texture::{TextureEntry, TextureRegistry};

// All GL work - texture uploads, shader passes, offscreen rasterization,
// compositing, present - runs on the raster thread, which owns the process's
// single GL context and single Impeller context (see raster.rs for why).
// Context is the UI thread's handle on it: methods marshal into RasterCmds,
// either fire-and-forget sends or blocking RPCs. The UI side keeps just enough
// bookkeeping (texture dims, shader kinds, buffer sizes) to validate ids and
// answer size queries without a round trip.

pub struct Context {
  raster_tx: mpsc::Sender<RasterCmd>,
  pub textures: TextureRegistry,
  // UI-side mirror of the raster thread's shader map: id -> is_pipeline.
  // Enough to validate params/draw-count updates without an RPC.
  shader_kinds: RefCell<HashMap<u64, bool>>,
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

/// A point-in-time inventory of the GPU bookkeeping, for resource
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

// Safety: Context is asserted Send + Sync (its Arc crosses into the closure
// the UI thread runs), but its interior (Rc entries, RefCell maps) is only
// ever accessed from the UI thread. The raster thread shares nothing with
// this struct beyond the command channel, whose Sender is Send.
unsafe impl Send for Context {}
unsafe impl Sync for Context {}

impl Context {
  pub(crate) fn new(raster_tx: mpsc::Sender<RasterCmd>) -> Self {
    Context {
      raster_tx,
      textures: TextureRegistry::new(),
      shader_kinds: RefCell::new(HashMap::new()),
      buffer_sizes: RefCell::new(HashMap::new()),
      next_buffer_id: Cell::new(1),
      cameras: CameraRegistry::default(),
      microphones: MicrophoneRegistry::default(),
      audio: AudioRegistry::default(),
      capture_requests: RefCell::new(HashMap::new()),
      capture_ready: RefCell::new(Vec::new()),
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
    self.raster_tx.send(RasterCmd::Frame(dl)).map_err(|_| ())
  }

  pub fn get_or_create_texture(&self, id: u64, size: ISize, make_pixels: impl FnOnce() -> Vec<u8>) -> Rc<TextureEntry> {
    if self.textures.get(id).is_none() {
      let pixels = make_pixels();
      self.create_texture_at(id, size.width as u32, size.height as u32, &pixels);
    }
    self.textures.get(id).expect("texture must exist after insert")
  }

  pub fn get_or_update_texture(&self, id: u64, size: ISize, make_pixels: impl FnOnce() -> Vec<u8>) -> Rc<TextureEntry> {
    let pixels = make_pixels();
    if self.textures.get(id).is_none() {
      self.create_texture_at(id, size.width as u32, size.height as u32, &pixels);
    } else if let Err(e) = self.update_texture(id, &pixels, 0) {
      log::warn!("[alloy] texture {id} update failed: {e}");
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
    let impeller = self
      .rpc(|reply| RasterCmd::CreateTexture { id, width, height, pixels: pixels.to_vec(), reply })
      .and_then(std::convert::identity)
      .expect("adopt texture failed");
    self.textures.insert(id, TextureEntry { impeller, width, height });
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
    self.create_texture_at(id, width, height, &pixels[..frame_size]);
    Ok(())
  }

  /// Recreate a shader/pipeline target at a new size under the same id: the
  /// compiled program, sampler bindings, last-applied params, and draw state
  /// carry over, and the output re-renders at the new size immediately.
  /// Lookups pick up the new target right away; in-flight users of the old
  /// one keep it alive until released. The caller must request a frame.
  pub fn resize_shader_texture(&self, id: u64, width: u32, height: u32) -> Result<(), String> {
    if !self.shader_kinds.borrow().contains_key(&id) {
      return Err(format!("shader texture {id} not found"));
    }
    let impeller = self.rpc(|reply| RasterCmd::ResizeShaderTexture { id, width, height, reply })??;
    self.textures.insert(id, TextureEntry { impeller, width, height });
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
    let id = self.textures.allocate_id();
    let impeller = self.rpc(|reply| RasterCmd::CreateShaderTexture {
      id,
      width,
      height,
      fragment_src: fragment_src.to_string(),
      params: params.to_vec(),
      textures: textures.to_vec(),
      reply,
    })??;
    self.textures.insert(id, TextureEntry { impeller, width, height });
    self.shader_kinds.borrow_mut().insert(id, false);
    Ok(id)
  }

  /// Re-render an existing shader texture with new params. The output keeps its
  /// id and Impeller texture (no re-adoption); only the GL contents change, so
  /// the caller must request a frame for the new pixels to reach the screen.
  /// Sampler inputs are re-resolved, so updated source textures are picked up.
  pub fn update_shader_params(&self, id: u64, params: &[(String, f32)]) -> Result<(), String> {
    if !self.shader_kinds.borrow().contains_key(&id) {
      return Err(format!("shader texture {id} not found"));
    }
    self.send(RasterCmd::UpdateShaderParams { id, params: params.to_vec() });
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
  /// buffer's original size), then re-render every pipeline drawing from it
  /// with its last-applied params, so geometry-only changes reach the screen
  /// even when no new params arrive. The caller must request a frame.
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
  /// `destroy_texture`, and `<texture src>` all apply).
  pub fn create_pipeline_texture(&self, spec: &PipelineSpec) -> Result<u64, String> {
    let id = self.textures.allocate_id();
    let owned = PipelineSpecOwned {
      width: spec.width,
      height: spec.height,
      vertex_src: spec.vertex_src.to_string(),
      fragment_src: spec.fragment_src.to_string(),
      params: spec.params.to_vec(),
      textures: spec.textures.to_vec(),
      attributes: spec.attributes.to_vec(),
      buffer_id: spec.buffer_id,
      topology: spec.topology.to_string(),
      draw_count: spec.draw_count,
      depth: spec.depth,
      clear_color: spec.clear_color,
    };
    let impeller = self.rpc(|reply| RasterCmd::CreatePipelineTexture { id, spec: owned, reply })??;
    self.textures.insert(id, TextureEntry { impeller, width: spec.width, height: spec.height });
    self.shader_kinds.borrow_mut().insert(id, true);
    Ok(id)
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
  /// bookkeeping (draw state, layout, bindings, last-applied params). Sorted
  /// by id for stable output.
  pub fn gpu_resources(&self) -> GpuResources {
    self
      .rpc(|reply| RasterCmd::Resources { reply })
      .unwrap_or_else(|_| GpuResources { textures: Vec::new(), buffers: Vec::new(), pipelines: Vec::new() })
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
  /// handle), not by the registry.
  pub fn render_display_list_to_texture(&self, dl: &DisplayList, width: u32, height: u32) -> Result<Texture, String> {
    self.rpc(|reply| RasterCmd::RasterizeDl { dl: dl.clone(), width, height, reply })?
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
    self.rpc(|reply| RasterCmd::ReadTexture { texture: texture.clone(), width, height, reply })?
  }

  /// Free a texture created via `create_texture_from_pixels`, `create_texture_at`,
  /// or `create_shader_texture`. Removes the entry from the texture registry so
  /// in-flight display list references keep the texture alive until they drop.
  /// For shader textures also destroys the GL program and FBO.
  pub fn destroy_texture(&self, id: u64) {
    self.textures.remove(id);
    self.shader_kinds.borrow_mut().remove(&id);
    self.send(RasterCmd::DestroyTexture { id });
  }
}
