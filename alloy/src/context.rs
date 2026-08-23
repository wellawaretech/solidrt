use impellers::{DisplayList, ISize, Texture};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::{mpsc, Arc};

use crate::audio::AudioRegistry;
use crate::camera::CameraRegistry;
use crate::gpu::{
  resolve_draw_range, validate_draw_range, validate_order, validate_param_if_declared, validate_params,
  validate_texture_bindings, vertex_stride, BufferIds, DrawBounds, DrawRange, DrawSpec, DrawUpdate,
  GpuLimits, GpuResources,
  NodeShader, ParamValue, PipelineDesc, PipelineSpec, ShaderStage, TargetSpec, UniformKind, UniformTable, WindowShader,
  WriteLeases,
};
use crate::microphone::MicrophoneRegistry;
use crate::raster::{RasterCmd, RasterCounters, RasterSender, RasterStats};
use crate::texture::{SamplerState, TextureEntry, TextureFormat, TextureRegistry};
use crate::yuv::{self, YuvLayout, YuvMatrix, YuvRange};

// All GL work - texture uploads, shader passes, offscreen rasterization,
// compositing, present - runs on the raster thread, which owns the process's
// single GL context and single Impeller context (see raster.rs for why).
// Context is the UI thread's handle on it: methods marshal into RasterCmds,
// either fire-and-forget sends or blocking RPCs. The UI side keeps just enough
// bookkeeping (texture dims, shader kinds, buffer sizes) to validate ids and
// answer size queries without a round trip.

// UI-side mirror of one shader/pipeline target, seeded by its create reply
// (fused paths, whose program is anonymous) or derived from the pipeline
// mirror (the split path). What update-path validation reads.
struct TargetMirror {
  /// The program's active uniforms; Rc-shared with the program and pipeline
  /// mirrors on the split path. Empty (and unused) for a draw target, whose
  /// programs live per entry.
  uniforms: Rc<UniformTable>,
  /// The target's current resolved draw range, what set_draw merges partial
  /// updates against. None for a fullscreen fragment pass (no mesh draw) and
  /// for draw targets (whose ranges live per entry).
  draw: Option<DrawRange>,
  /// The fetch bounds and range vocabulary for set_draw, captured at create
  /// (see `DrawBounds` for why a captured bound stays correct).
  bounds: DrawBounds,
  /// The buffer ids the target's fixed-kind pass reads (vertex, index,
  /// instance), so a buffer write can name the targets whose pixels it
  /// changes (see `note_buffer_content`) and a buffer swap has a current
  /// value to merge into. All zero for draw targets, whose buffers live per
  /// entry.
  buffers: BufferIds,
  /// Some = a draw target: the mutable ordered draw list, mirrored per entry
  /// (the flat fields above then describe nothing). None for the fixed
  /// kinds, whose one pass the flat fields describe.
  entries: Option<DrawListMirror>,
}

// UI-side mirror of a draw target's entry list: stable id allocation plus
// per-entry validation state. Entry ids are target-scoped and never reused,
// so a stale id from a removed entry errors instead of aliasing.
struct DrawListMirror {
  /// Whether the target owns depth storage (the addDraw depth-compatibility
  /// check reads this against the pipeline's declared depth state).
  depth: bool,
  next_draw: u64,
  entries: HashMap<u64, EntryMirror>,
}

// UI-side mirror of one draw entry: what per-entry update validation reads
// (the same shape as TargetMirror's flat half, per entry).
struct EntryMirror {
  /// The entry's program's active uniforms, Rc-shared with the pipeline
  /// mirror.
  uniforms: Rc<UniformTable>,
  /// The entry's current resolved draw range, what set_draw_range merges
  /// partial updates against.
  draw: DrawRange,
  /// The entry's fetch bounds and range vocabulary (see `TargetMirror::bounds`).
  bounds: DrawBounds,
  /// The buffer ids the entry reads (see `TargetMirror::buffers`).
  buffers: BufferIds,
}

// UI-side mirror of a registered render pipeline: its program's uniforms, the
// record strides of its attribute layouts (vertex and per-instance), and
// whether it declares depth state, for deriving target/entry mirrors and
// validating adds without an RPC.
struct PipelineMirror {
  uniforms: Rc<UniformTable>,
  stride: usize,
  instance_stride: usize,
  depth: bool,
}

pub struct Context {
  raster_tx: RasterSender,
  // Shared live counters (queue depth, idle ticks, fence timeouts, pass
  // count/time), exposed through the accessors below for diagnostics. See
  // RasterStats for what each one means.
  stats: Arc<RasterStats>,
  pub textures: TextureRegistry,
  // UI-side mirror of the raster thread's shader map (see TargetMirror):
  // enough to validate the fire-and-forget updates - params, sampler
  // rebinds, draw counts - synchronously at the call site, without an RPC.
  targets: RefCell<HashMap<u64, TargetMirror>>,
  // UI-side mirror of each shader target's sampler graph: target id ->
  // ((draw entry id, uniform name) -> source texture id). The entry id keys
  // per-entry bindings apart - two entries may bind the same uniform name to
  // different sources - and key 0 is the TARGET-LEVEL slot on every kind:
  // the single pass of the fixed kinds, the shared bindings of a draw
  // target (whose entry ids start at 1). One meaning, which is what lets
  // set_target_textures route by kind over one record shape. Lets
  // the bind paths reject sampling cycles synchronously; the raster thread
  // walks the same edges (unioned per target) to propagate re-renders
  // through target chains, and a cycle there would under-render, so it must
  // never form. Recorded bindings are real: a rebind naming anything but an
  // active sampler2D is rejected against the right uniform table before it
  // is recorded or sent.
  shader_sources: RefCell<HashMap<u64, HashMap<(u64, String), u64>>>,
  // UI-side mirror of which shader targets are manual (TargetSpec::manual):
  // validates render_target synchronously, and relaxes the sampling-cycle
  // test - the flush never renders a manual target, so a cycle is only a
  // hazard when every member is flush-rendered (see set_target_textures).
  manual_targets: RefCell<HashSet<u64>>,
  // Texture ids whose pixels changed (or will, at the next dirty flush)
  // behind an unchanged id since the last drain: target mutations, uploads,
  // copies, and everything downstream through the sampler graph (see
  // note_content). GPU writes produce no rendertree damage of their own, and
  // baked snapshot boundaries are the one consumer that goes stale for it -
  // the frame build drains this set (take_content_changes) and turns it into
  // exactly that damage (RenderTree::texture_content_changed). A HashSet, so
  // a per-frame write burst on one target costs one insert probe per write.
  content_changes: RefCell<HashSet<u64>>,
  // UI-side mirror of the raster thread's program registry: program id ->
  // active uniforms (from the LinkProgram reply). Programs are their own id
  // space (like buffers), separate from texture ids.
  program_uniforms: RefCell<HashMap<u64, Rc<UniformTable>>>,
  next_program_id: Cell<u64>,
  // UI-side mirror of the raster thread's render pipeline registry (its own
  // id space again): per pipeline, its program's uniforms and vertex stride,
  // for deriving target mirrors without an RPC.
  pipeline_mirrors: RefCell<HashMap<u64, PipelineMirror>>,
  next_pipeline_id: Cell<u64>,
  // UI-side mirror of the raster thread's raw stage registry: id -> stage,
  // for validating link_program's arguments without an RPC.
  stage_kinds: RefCell<HashMap<u64, ShaderStage>>,
  next_stage_id: Cell<u64>,
  // UI-side mirror of the raster thread's buffer sizes, for bounds validation
  // and gpu_buffer_len.
  buffer_sizes: RefCell<HashMap<u64, usize>>,
  next_buffer_id: Cell<u64>,
  // Staging blocks for the zero-copy buffer write path (begin_buffer_write /
  // end_buffer_write): open leases and the recycled-block pool, plus the
  // channel the raster thread returns published blocks on. Drained lazily at
  // the next begin - there is no other party to wake.
  write_leases: RefCell<WriteLeases>,
  block_recycle_tx: mpsc::Sender<(u64, Vec<u8>)>,
  block_recycle_rx: mpsc::Receiver<(u64, Vec<u8>)>,
  // UI-side cache of the device ceilings (see gpu_limits): fetched over one
  // blocking RPC on first use, then a plain read on every validation site.
  limits: Cell<Option<GpuLimits>>,
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
  // Planar YUV textures (see yuv.rs): app-visible output id -> its plane
  // sets, for update_yuv, and for destroy_texture to take the planes down
  // with the output.
  yuv_groups: RefCell<HashMap<u64, YuvGroup>>,
}

// The composition behind one YUV texture id: TWO full plane sets, each plane
// as (uniform name, texture id, byte offset in a packed frame), plus the
// packed frame size for validation. The conversion shader samples the
// `front` set; update_yuv uploads into the other set and swaps. The double
// buffering exists for the raster thread: on a pipelined (tile-based) GPU
// the previous frame's conversion pass may still be sampling its planes when
// the next upload lands, and writing a texture with reads in flight makes
// the driver stall or ghost it. Alternating sets keeps every upload
// hazard-free.
struct YuvGroup {
  sets: [Vec<(&'static str, u64, usize)>; 2],
  front: usize,
  frame_size: usize,
}

/// The successful outcome of a node capture: the RGBA8 pixels the node's
/// subtree was rasterized into (tightly packed top-to-bottom rows, same
/// layout as `Context::read_texture`) and their device-pixel dimensions.
/// Nothing is registered; there is nothing for the caller to free.
pub struct CaptureInfo {
  pub pixels: Vec<u8>,
  pub width: u32,
  pub height: u32,
}

/// A capture completion callback, invoked exactly once with the outcome after
/// the paint pass that serviced (or failed to service) the request. Runs on the
/// UI thread, out of the tree walk (see `deliver_captures`).
pub type CaptureDone = Box<dyn FnOnce(Result<CaptureInfo, String>)>;

/// A stats-overlay declaration (see `Context::set_stats_overlay`): the
/// overlay's display list, drawn with its content at the origin, plus the
/// window-space rectangle it composites into - physical pixels, top-left
/// origin, `width` x `height` also being the rasterized layer's size.
pub struct StatsOverlay {
  pub dl: DisplayList,
  pub x: i32,
  pub y: i32,
  pub width: u32,
  pub height: u32,
}

// Safety: Context is asserted Send + Sync (its Arc crosses into the closure
// the UI thread runs), but its interior (Rc entries, RefCell maps) is only
// ever accessed from the UI thread. The raster thread shares nothing with
// this struct beyond the command channel, whose Sender is Send.
unsafe impl Send for Context {}
unsafe impl Sync for Context {}

impl Context {
  pub(crate) fn new(raster_tx: RasterSender, stats: Arc<RasterStats>) -> Self {
    let (recycle_tx, recycle_rx) = mpsc::channel();
    Context {
      raster_tx,
      stats,
      textures: TextureRegistry::new(),
      targets: RefCell::new(HashMap::new()),
      shader_sources: RefCell::new(HashMap::new()),
      manual_targets: RefCell::new(HashSet::new()),
      content_changes: RefCell::new(HashSet::new()),
      program_uniforms: RefCell::new(HashMap::new()),
      next_program_id: Cell::new(1),
      pipeline_mirrors: RefCell::new(HashMap::new()),
      next_pipeline_id: Cell::new(1),
      stage_kinds: RefCell::new(HashMap::new()),
      next_stage_id: Cell::new(1),
      buffer_sizes: RefCell::new(HashMap::new()),
      next_buffer_id: Cell::new(1),
      write_leases: RefCell::new(WriteLeases::new()),
      block_recycle_tx: recycle_tx,
      block_recycle_rx: recycle_rx,
      limits: Cell::new(None),
      cameras: CameraRegistry::default(),
      microphones: MicrophoneRegistry::default(),
      audio: AudioRegistry::default(),
      capture_requests: RefCell::new(HashMap::new()),
      capture_ready: RefCell::new(Vec::new()),
      pending_destroys: RefCell::new(Vec::new()),
      yuv_groups: RefCell::new(HashMap::new()),
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

  /// The device ceilings (see `GpuLimits`), fetched from the raster thread on
  /// first use and cached: one blocking RPC per process - at module-import
  /// time in practice, when the queue is empty - then a plain read on every
  /// validation site. After the raster thread exits (engine shutdown) the ES
  /// 3.0 floors come back.
  pub fn gpu_limits(&self) -> GpuLimits {
    if let Some(limits) = self.limits.get() {
      return limits;
    }
    let limits = self.rpc(|reply| RasterCmd::Limits { reply }).unwrap_or(GpuLimits::FLOOR);
    self.limits.set(Some(limits));
    limits
  }

  /// The raster thread's live counters (see RasterCounters), read at this
  /// instant rather than from any frame-latched snapshot: a backlogged
  /// raster thread produces no frames, so a latch goes stale exactly when
  /// these matter.
  pub fn raster_counters(&self) -> RasterCounters {
    self.stats.sample()
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

  pub fn get_or_create_texture(
    &self,
    id: u64,
    size: ISize,
    make_pixels: impl FnOnce() -> Vec<u8>,
  ) -> Result<Rc<TextureEntry>, String> {
    if self.textures.get(id).is_none() {
      let pixels = make_pixels();
      self.create_texture_at(id, size.width as u32, size.height as u32, &pixels, SamplerState::default(), TextureFormat::Rgba8, None)?;
    }
    Ok(self.textures.get(id).expect("texture must exist after insert"))
  }

  pub fn get_or_update_texture(
    &self,
    id: u64,
    size: ISize,
    make_pixels: impl FnOnce() -> Vec<u8>,
  ) -> Result<Rc<TextureEntry>, String> {
    let pixels = make_pixels();
    if self.textures.get(id).is_none() {
      self.create_texture_at(id, size.width as u32, size.height as u32, &pixels, SamplerState::default(), TextureFormat::Rgba8, None)?;
    } else if let Err(e) = self.update_texture(id, &pixels, 0) {
      log::warn!("[alloy] texture {id} update failed: {e}");
    }
    Ok(self.textures.get(id).expect("texture must exist after insert or update"))
  }

  /// Create a sampleable texture from pixels (RGBA8, or single-channel R8)
  /// and adopt into Impeller, with the given sampling (how every consumer -
  /// shader passes and `<texture>` display - samples it) and an optional
  /// debug label. Returns the registry id assigned to the new texture; errs
  /// on a size over the device limit (named in the message), checked here so
  /// the mistake throws at the call site.
  pub fn create_texture_from_pixels(
    &self,
    width: u32,
    height: u32,
    pixels: &[u8],
    sampler: SamplerState,
    format: TextureFormat,
    label: Option<String>,
  ) -> Result<u64, String> {
    let id = self.textures.allocate_id();
    self.create_texture_at(id, width, height, pixels, sampler, format, label)?;
    Ok(id)
  }

  /// Create (or replace) the texture stored at `id`, e.g. to resize a stream
  /// texture without invalidating the id handed out to consumers. Lookups pick
  /// up the new texture immediately; in-flight users of the old entry keep it
  /// alive until released. A `label` of None on a replace keeps the existing
  /// entry's label (the id-stable resize contract). Errs on a size over the
  /// device limit (checked here, before the RPC) or a failed adoption.
  pub fn create_texture_at(
    &self,
    id: u64,
    width: u32,
    height: u32,
    pixels: &[u8],
    sampler: SamplerState,
    format: TextureFormat,
    label: Option<String>,
  ) -> Result<(), String> {
    self.gpu_limits().check_texture_size(width, height)?;
    // A create at a fresh id cannot be referenced by anything yet; a replace
    // at a live id (stream resize, camera format change) is a content change
    // behind that id like any other.
    let replace = self.textures.get(id).is_some();
    let impeller = self
      .rpc(|reply| RasterCmd::CreateTexture { id, width, height, pixels: pixels.to_vec(), sampler, format, label, reply })??;
    self.textures.insert(id, TextureEntry { impeller, width, height, sampler, format });
    if replace {
      self.note_content(id);
    }
    Ok(())
  }

  /// Re-upload pixels into an existing texture, sized by the id's format
  /// (width*height*4 for rgba8, width*height for r8). `pixels` may be a
  /// larger buffer holding multiple frames; `offset` selects the frame start.
  /// The frame must match the texture's dimensions exactly.
  pub fn update_texture(&self, id: u64, pixels: &[u8], offset: usize) -> Result<(), String> {
    let entry = self.textures.get(id).ok_or_else(|| format!("texture {id} not found"))?;
    let (width, height, format) = (entry.width(), entry.height(), entry.format);
    let frame_size = (width as usize) * (height as usize) * format.bytes_per_pixel();
    let end = offset.checked_add(frame_size).ok_or_else(|| "offset overflow".to_string())?;
    if end > pixels.len() {
      return Err(format!(
        "need {frame_size} bytes at offset {offset} for {width}x{height} {}, buffer has {}",
        format.name(),
        pixels.len()
      ));
    }
    self.send(RasterCmd::UpdateTexture { id, pixels: pixels[offset..end].to_vec() });
    self.note_content(id);
    Ok(())
  }

  /// Replace a registered pixel texture with one of a new size at the same id
  /// (an id-stable resize): lookups and shader sampler bindings pick up the
  /// new texture immediately (shaders sampling it re-render), in-flight users
  /// of the old entry keep it alive until released. `pixels` seeds the new
  /// contents and must hold at least one frame at the id's format
  /// (width*height*4 for rgba8, width*height for r8). Rejects render target
  /// ids - resize those with `resize_target`, which carries the compiled
  /// program and draw state along. The caller must request a frame.
  pub fn resize_texture(&self, id: u64, width: u32, height: u32, pixels: &[u8]) -> Result<(), String> {
    let Some(entry) = self.textures.get(id) else {
      return Err(format!("texture {id} not found"));
    };
    if self.targets.borrow().contains_key(&id) {
      return Err(format!("texture {id} is a render target; resize it with setTargetSize"));
    }
    // Sampling and format are properties of the id and survive the id-stable
    // resize, as does the label (None here = keep, applied raster-side).
    let (sampler, format) = (entry.sampler(), entry.format);
    let frame_size = (width as usize) * (height as usize) * format.bytes_per_pixel();
    if pixels.len() < frame_size {
      return Err(format!("need {frame_size} bytes for {width}x{height} {}, buffer has {}", format.name(), pixels.len()));
    }
    self.create_texture_at(id, width, height, &pixels[..frame_size], sampler, format, None)
  }

  /// Create a planar YUV texture (see yuv.rs): plane textures for `layout`
  /// (two double-buffered sets, see YuvGroup) plus a conversion shader
  /// target sampling them, whose RGBA output id is returned - usable
  /// anywhere a texture id is. Feed packed frames with `update_yuv`; the
  /// output re-renders at the next dirty flush like any shader target. Color constants are baked at creation (fixed per
  /// stream; a standard change means a new texture), `sampler` is the
  /// OUTPUT's sampling (planes always sample linear/clamp for chroma
  /// upscaling), and the content starts black until the first frame.
  /// Destroying the returned id takes the planes down with it. There is no
  /// id-stable resize: a size change is a new texture (stream dimension
  /// changes replace the player's texture anyway).
  pub fn create_yuv_texture(
    &self,
    width: u32,
    height: u32,
    layout: YuvLayout,
    matrix: YuvMatrix,
    range: YuvRange,
    sampler: SamplerState,
    label: Option<String>,
  ) -> Result<u64, String> {
    if width == 0 || height == 0 {
      return Err(format!("yuv texture size {width}x{height} must be non-zero"));
    }
    self.gpu_limits().check_texture_size(width, height)?;
    let planes = yuv::planes(layout, width, height);
    let frame_size: usize = planes.iter().map(|p| p.byte_len()).sum();
    // Seed planes with black (Y floor, chroma midpoint; NV12's interleaved
    // UV seeds both bytes 128) - zeroed chroma would start the output green.
    let y_black = if range == YuvRange::Limited { 16u8 } else { 0u8 };
    // Two full plane sets, double buffered (see YuvGroup); the shader starts
    // bound to set 0.
    let mut sets: [Vec<(&'static str, u64, usize)>; 2] = [Vec::new(), Vec::new()];
    let mut failure: Option<String> = None;
    'create: for (set, ids) in sets.iter_mut().enumerate() {
      for plane in &planes {
        let value = if plane.name == "uY" { y_black } else { 128u8 };
        let plane_label = label.as_ref().map(|l| format!("{l}.{}{set}", plane.name[1..].to_lowercase()));
        match self.create_texture_from_pixels(
          plane.width,
          plane.height,
          &vec![value; plane.byte_len()],
          SamplerState::default(),
          plane.format,
          plane_label,
        ) {
          Ok(id) => ids.push((plane.name, id, plane.offset)),
          Err(e) => {
            failure = Some(e);
            break 'create;
          }
        }
      }
    }
    let result = match failure {
      Some(e) => Err(e),
      None => {
        let bindings: Vec<(String, u64)> = sets[0].iter().map(|&(name, id, _)| (name.to_string(), id)).collect();
        self.create_shader_texture(
          width,
          height,
          &yuv::fragment_src(layout, matrix, range),
          &[],
          &bindings,
          sampler,
          label,
        )
      }
    };
    match result {
      Ok(out) => {
        self.yuv_groups.borrow_mut().insert(out, YuvGroup { sets, front: 0, frame_size });
        Ok(out)
      }
      Err(e) => {
        for (_, id, _) in sets.into_iter().flatten() {
          self.destroy_texture(id);
        }
        Err(e)
      }
    }
  }

  /// Upload one tightly packed frame (every plane, laid out per
  /// `yuv::planes`) into a YUV texture. Takes the frame BY VALUE: the buffer
  /// crosses to the raster thread as-is - no per-plane copies - and the
  /// planes slice it there at their fixed offsets. The upload lands in the
  /// back plane set and the conversion target rebinds to it (double
  /// buffering, see YuvGroup), so planes a still-in-flight conversion pass
  /// samples are never written under it. The conversion target re-renders
  /// and content damage propagates exactly as for `update_texture`.
  pub fn update_yuv(&self, id: u64, frame: Vec<u8>) -> Result<(), String> {
    let (planes, bindings) = {
      let mut groups = self.yuv_groups.borrow_mut();
      let group = groups.get_mut(&id).ok_or_else(|| format!("yuv texture {id} not found"))?;
      if frame.len() < group.frame_size {
        return Err(format!("need {} bytes for a packed frame, buffer has {}", group.frame_size, frame.len()));
      }
      let back = 1 - group.front;
      group.front = back;
      let set = &group.sets[back];
      let planes: Vec<(u64, usize)> = set.iter().map(|&(_, plane, offset)| (plane, offset)).collect();
      let bindings: Vec<(String, u64)> = set.iter().map(|&(name, plane, _)| (name.to_string(), plane)).collect();
      (planes, bindings)
    };
    for &(plane, _) in &planes {
      self.note_content(plane);
    }
    self.send(RasterCmd::UpdateYuv { planes, frame });
    // Rebinding through the ordinary path keeps the sampler-graph mirror
    // honest and re-renders the conversion output at the next dirty flush;
    // channel order puts the rebind after the upload.
    self.set_target_textures(id, &bindings)
  }

  /// Recreate a render target of any kind at a new size under the same id:
  /// the compiled programs, sampler bindings, last-applied params, and draw
  /// state carry over, and the output re-renders at the new size at the next
  /// dirty flush. Lookups pick up the new target right away; in-flight users
  /// of the old one keep it alive until released. The caller must request a
  /// frame.
  pub fn resize_target(&self, id: u64, width: u32, height: u32) -> Result<(), String> {
    if !self.targets.borrow().contains_key(&id) {
      return Err(format!("target {id} not found"));
    }
    self.gpu_limits().check_texture_size(width, height)?;
    let impeller = self.rpc(|reply| RasterCmd::ResizeShaderTexture { id, width, height, reply })??;
    let sampler = self.textures.get(id).map(|e| e.sampler()).unwrap_or_default();
    self.textures.insert(id, TextureEntry { impeller, width, height, sampler, format: TextureFormat::Rgba8 });
    // The storage is regenerated whatever the kind, manual included, so this
    // notes unconditionally (unlike the pure-mutation paths).
    self.note_content(id);
    Ok(())
  }

  /// Compile a GLSL ES fragment shader into a new RGBA8 target texture and
  /// register the output in the texture registry. Returns the id the output
  /// is sampleable under (usable anywhere a normal texture id is); the first
  /// render happens at the raster thread's next dirty flush, before anything
  /// observes the pixels. The compiled program is retained so
  /// set_target_params can re-render the same texture without recompiling
  /// or re-adopting.
  #[allow(clippy::too_many_arguments)]
  pub fn create_shader_texture(
    &self,
    width: u32,
    height: u32,
    fragment_src: &str,
    params: &[(String, ParamValue)],
    textures: &[(String, u64)],
    sampler: SamplerState,
    label: Option<String>,
  ) -> Result<u64, String> {
    let limits = self.gpu_limits();
    limits.check_texture_size(width, height)?;
    limits.check_texture_units(textures.len())?;
    let id = self.textures.allocate_id();
    let (impeller, uniforms) = self.rpc(|reply| RasterCmd::CreateShaderTexture {
      id,
      width,
      height,
      fragment_src: fragment_src.to_string(),
      params: params.to_vec(),
      textures: textures.to_vec(),
      sampler,
      label,
      reply,
    })??;
    self.textures.insert(id, TextureEntry { impeller, width, height, sampler, format: TextureFormat::Rgba8 });
    self.targets.borrow_mut().insert(
      id,
      TargetMirror { uniforms: Rc::new(uniforms), draw: None, bounds: DrawBounds::default(), buffers: BufferIds::default(), entries: None },
    );
    self
      .shader_sources
      .borrow_mut()
      .insert(id, textures.iter().map(|(name, src)| ((0, name.clone()), *src)).collect());
    Ok(id)
  }

  /// The binding checks shared by every path that adds sampler edges to
  /// target `id` (pass `entry` 0 for the fixed kinds' single pass): the
  /// merged per-pass count must fit the device's texture units, every source
  /// must exist, and no binding may close a flush-rendered sampling cycle.
  fn validate_new_bindings(&self, id: u64, entry: u64, textures: &[(String, u64)]) -> Result<(), String> {
    let limits = self.gpu_limits();
    let sources = self.shader_sources.borrow();
    let manual = self.manual_targets.borrow();
    // The rebind merges into the entry's existing bindings, so it is the
    // merged count that must fit the device's texture units - per entry,
    // because units rebind per draw.
    let current = sources.get(&id);
    let current_count = current.map_or(0, |c| c.keys().filter(|(e, _)| *e == entry).count());
    let added = textures
      .iter()
      .filter(|(name, _)| current.is_none_or(|c| !c.contains_key(&(entry, name.clone()))))
      .count();
    limits.check_texture_units(current_count + added)?;
    for (name, src_id) in textures {
      if self.textures.get(*src_id).is_none() {
        return Err(format!("texture {src_id} (sampler '{name}') not found"));
      }
      if *src_id == id {
        return Err(format!("sampler '{name}' binds shader texture {id} to its own target (same-pass feedback)"));
      }
      // The flush-rendered subgraph is acyclic, and this call only changes
      // `id`'s own outgoing edges, so any new all-pure cycle runs through
      // one of the updated bindings: per binding, reject if the target can
      // already be reached from the new source without passing through a
      // manual target. A manual `id` needs no walk at all - every cycle
      // through it has a manual member (its direct self-bind was rejected
      // above). The walk never needs `id`'s own edges (it stops on reaching
      // `id`), so the pre-update graph is the right one.
      if !manual.contains(&id) && samples_transitively(&sources, &manual, *src_id, id) {
        return Err(format!("sampler '{name}' would create a sampling cycle back to shader texture {id}"));
      }
    }
    Ok(())
  }

  /// Create an interleaved vertex buffer from raw bytes, returning its id.
  /// Buffer ids are their own space (not texture ids); pipelines reference the
  /// buffer via `PipelineSpec::buffer_id`.
  pub fn create_gpu_buffer(&self, data: &[u8], label: Option<String>) -> Result<u64, String> {
    let id = self.next_buffer_id.get();
    self.rpc(|reply| RasterCmd::CreateBuffer { id, data: data.to_vec(), label, reply })??;
    self.next_buffer_id.set(id + 1);
    self.buffer_sizes.borrow_mut().insert(id, data.len());
    Ok(id)
  }

  /// Create a zeroed vertex buffer of `size` bytes - the natural create for
  /// buffers filled through the write lease (begin_buffer_write), where
  /// initial contents would be dead weight.
  pub fn create_gpu_buffer_zeroed(&self, size: usize, label: Option<String>) -> Result<u64, String> {
    let id = self.next_buffer_id.get();
    self.rpc(|reply| RasterCmd::CreateBuffer { id, data: vec![0u8; size], label, reply })??;
    self.next_buffer_id.set(id + 1);
    self.buffer_sizes.borrow_mut().insert(id, size);
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
    self.note_buffer_content(id);
    Ok(())
  }

  /// Open a zero-copy write into a vertex buffer: returns a staging block
  /// exactly the buffer's size for the caller to fill in place, published by
  /// `end_buffer_write`. Contents are UNSPECIFIED (a recycled block holds
  /// what was published the time before last), so fill everything you
  /// publish. The pointer stays valid until end/destroy for this id; no Rust
  /// reference into the block is formed while the lease is open - the caller
  /// owns the bytes exclusively.
  pub fn begin_buffer_write(&self, id: u64) -> Result<(*mut u8, usize), String> {
    let size = *self.buffer_sizes.borrow().get(&id).ok_or_else(|| format!("buffer {id} not found"))?;
    let mut leases = self.write_leases.borrow_mut();
    // Blocks the raster thread finished with, back into the pool (retired
    // ids drop). Lazy: nothing else needs to observe a recycle promptly.
    while let Ok((rid, block)) = self.block_recycle_rx.try_recv() {
      let sizes = self.buffer_sizes.borrow();
      leases.recycle(rid, block, |i| sizes.contains_key(&i));
    }
    leases.begin(id, size)
  }

  /// Publish the open lease's first `len` bytes at offset 0: the block moves
  /// to the raster thread (no copy) and comes back through the recycle
  /// channel. `len` 0 cancels - the lease closes, nothing is sent. Always
  /// closes the lease, error or not. The caller must request a frame on a
  /// non-zero publish (same contract as `write_gpu_buffer`).
  pub fn end_buffer_write(&self, id: u64, len: usize) -> Result<(), String> {
    let block = self.write_leases.borrow_mut().end(id)?;
    if len == 0 {
      self.write_leases.borrow_mut().cancel(id, block);
      return Ok(());
    }
    if len > block.len() {
      let size = block.len();
      self.write_leases.borrow_mut().cancel(id, block);
      return Err(format!("publish of {len} bytes exceeds buffer size {size}"));
    }
    self.send(RasterCmd::WriteBufferLease { id, block, len, recycle: self.block_recycle_tx.clone() });
    self.note_buffer_content(id);
    Ok(())
  }

  /// Free a vertex buffer: the id retires immediately (further writes error),
  /// while targets drawing from it hold their own reference - like their
  /// pipeline - so either destruction order is safe; the GL buffer is deleted
  /// once the last such target is destroyed.
  pub fn destroy_gpu_buffer(&self, id: u64) {
    self.buffer_sizes.borrow_mut().remove(&id);
    self.write_leases.borrow_mut().destroy(id);
    self.send(RasterCmd::DestroyBuffer { id });
  }

  /// Compile a vertex+fragment pipeline, render it once into a new RGBA8
  /// target texture, and register the output exactly like
  /// `create_shader_texture` (same id space; `set_target_params`,
  /// `destroy_texture`, and `<texture src>` all apply). The fused convenience
  /// over `create_render_pipeline` + `create_shader_target`; the anonymous
  /// program and pipeline die with the target.
  pub fn create_pipeline_texture(&self, mut spec: PipelineSpec) -> Result<u64, String> {
    let limits = self.gpu_limits();
    limits.check_texture_size(spec.target.width, spec.target.height)?;
    limits.check_texture_units(spec.entry.textures.len())?;
    limits.check_vertex_attribs(spec.pipeline.attributes.len() + spec.pipeline.instance_attributes.len())?;
    validate_load(&spec.target)?;
    let stride = vertex_stride(&spec.pipeline.attributes) as usize;
    let instance_stride = vertex_stride(&spec.pipeline.instance_attributes) as usize;
    let bounds = self.resolve_entry_range(&mut spec.entry, stride, instance_stride)?;
    let id = self.textures.allocate_id();
    let (width, height, sampler) = (spec.target.width, spec.target.height, spec.target.sampler);
    let manual = spec.target.manual;
    let draw = spec.entry.draw;
    let buffers = spec.entry.buffer_ids();
    let sources: HashMap<(u64, String), u64> =
      spec.entry.textures.iter().map(|(name, src)| ((0, name.clone()), *src)).collect();
    let (impeller, uniforms) = self.rpc(|reply| RasterCmd::CreatePipelineTexture { id, spec, reply })??;
    self.textures.insert(id, TextureEntry { impeller, width, height, sampler, format: TextureFormat::Rgba8 });
    self
      .targets
      .borrow_mut()
      .insert(id, TargetMirror { uniforms: Rc::new(uniforms), draw: Some(draw), bounds, buffers, entries: None });
    self.shader_sources.borrow_mut().insert(id, sources);
    if manual {
      self.manual_targets.borrow_mut().insert(id);
    }
    Ok(id)
  }

  /// The byte size of vertex buffer `id` from the UI-side size mirror: None
  /// for id 0 (no buffer bound), an error for an unknown id - caught here,
  /// before the create RPC, so the mistake throws at the call site. What the
  /// draw-range resolution and the captured draw bound (see
  /// `TargetMirror::bounds`) both read.
  fn buffer_size(&self, id: u64) -> Result<Option<usize>, String> {
    if id == 0 {
      return Ok(None);
    }
    self.buffer_sizes.borrow().get(&id).copied().map(Some).ok_or_else(|| format!("buffer {id} not found"))
  }

  /// Check an entry's buffers against the pipeline's declared layouts and
  /// resolve its draw range in place, capturing the bounds for the entry's
  /// mirror: the fetch bound against the vertex buffer at the pipeline's
  /// stride for a plain entry, against the index buffer at the format's
  /// element size for an indexed one - whose vertex fetch runs through the
  /// index VALUES and so cannot be bounds-checked here (raw GL semantics;
  /// robust drivers clamp) - and the instance bound against the instance
  /// buffer at the per-instance record stride. Shared by the two split
  /// creates, the fused create, and add_draw.
  fn resolve_entry_range(&self, entry: &mut DrawSpec, stride: usize, instance_stride: usize) -> Result<DrawBounds, String> {
    let size = self.buffer_size(entry.buffer)?;
    if stride > 0 && size.is_none() {
      return Err("pipeline declares attributes but no vertex buffer".to_string());
    }
    let instance_size = match entry.instance_buffer {
      0 => None,
      id => Some(
        self
          .buffer_sizes
          .borrow()
          .get(&id)
          .copied()
          .ok_or_else(|| format!("instance buffer {id} not found"))?,
      ),
    };
    if instance_stride > 0 && instance_size.is_none() {
      return Err("pipeline declares instanceAttributes but no instance buffer".to_string());
    }
    if instance_stride == 0 && instance_size.is_some() {
      return Err("pipeline declares no instanceAttributes; the instance buffer would never be read".to_string());
    }
    let (fetch, indexed) = match entry.index {
      Some((index_buffer, format)) => {
        let bytes = match index_buffer {
          0 => None,
          id => self.buffer_sizes.borrow().get(&id).copied(),
        }
        .ok_or_else(|| format!("index buffer {index_buffer} not found"))?;
        (Some((format.size() as usize, bytes)), true)
      }
      None => (size.filter(|_| stride > 0).map(|size| (stride, size)), false),
    };
    let bounds = DrawBounds { fetch, indexed, instance: instance_size.map(|size| (instance_stride, size)) };
    entry.draw = resolve_draw_range(entry.draw, bounds)?;
    Ok(bounds)
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
  pub fn link_shader_program(&self, vertex: u64, fragment: u64, label: Option<String>) -> Result<u64, String> {
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
    let uniforms = self.rpc(|reply| RasterCmd::LinkProgram { id, vertex, fragment, label, reply })??;
    self.next_program_id.set(id + 1);
    self.program_uniforms.borrow_mut().insert(id, Rc::new(uniforms));
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
  pub fn create_render_pipeline(&self, program: u64, desc: PipelineDesc, label: Option<String>) -> Result<u64, String> {
    self.gpu_limits().check_vertex_attribs(desc.attributes.len() + desc.instance_attributes.len())?;
    let uniforms = match self.program_uniforms.borrow().get(&program) {
      Some(uniforms) => uniforms.clone(),
      None => return Err(format!("program {program} not found")),
    };
    let stride = vertex_stride(&desc.attributes) as usize;
    let instance_stride = vertex_stride(&desc.instance_attributes) as usize;
    let depth = desc.depth.is_some();
    let id = self.next_pipeline_id.get();
    self.rpc(|reply| RasterCmd::CreateRenderPipeline { id, program, desc, label, reply })??;
    self.next_pipeline_id.set(id + 1);
    self.pipeline_mirrors.borrow_mut().insert(id, PipelineMirror { uniforms, stride, instance_stride, depth });
    Ok(id)
  }

  /// Drop a shared pipeline's registry entry and retire its id. Targets
  /// created from it keep rendering - they hold the pipeline until they are
  /// destroyed - so either destruction order is safe. The program it was
  /// created from is yours and unaffected.
  pub fn destroy_render_pipeline(&self, id: u64) {
    self.pipeline_mirrors.borrow_mut().remove(&id);
    self.send(RasterCmd::DestroyRenderPipeline { id });
  }

  /// Create a render target over a pipeline from `create_render_pipeline` and
  /// register the output exactly like `create_shader_texture` (same texture
  /// id space: params updates, `resize_target`, `<texture src>` and
  /// `destroy_texture` all apply). Many targets may share one pipeline, and
  /// creating a target compiles nothing.
  pub fn create_shader_target(&self, pipeline: u64, spec: TargetSpec, mut entry: DrawSpec) -> Result<u64, String> {
    let limits = self.gpu_limits();
    limits.check_texture_size(spec.width, spec.height)?;
    limits.check_texture_units(entry.textures.len())?;
    let (uniforms, stride, instance_stride) = match self.pipeline_mirrors.borrow().get(&pipeline) {
      Some(mirror) => (mirror.uniforms.clone(), mirror.stride, mirror.instance_stride),
      None => return Err(format!("pipeline {pipeline} not found")),
    };
    validate_load(&spec)?;
    entry.pipeline = pipeline;
    let bounds = self.resolve_entry_range(&mut entry, stride, instance_stride)?;
    let id = self.textures.allocate_id();
    let (width, height, sampler) = (spec.width, spec.height, spec.sampler);
    let manual = spec.manual;
    let draw = entry.draw;
    let buffers = entry.buffer_ids();
    let sources: HashMap<(u64, String), u64> =
      entry.textures.iter().map(|(name, src)| ((0, name.clone()), *src)).collect();
    let impeller = self.rpc(|reply| RasterCmd::CreateShaderTarget { id, spec, entry, reply })??;
    self.textures.insert(id, TextureEntry { impeller, width, height, sampler, format: TextureFormat::Rgba8 });
    self.targets.borrow_mut().insert(id, TargetMirror { uniforms, draw: Some(draw), bounds, buffers, entries: None });
    self.shader_sources.borrow_mut().insert(id, sources);
    if manual {
      self.manual_targets.borrow_mut().insert(id);
    }
    Ok(id)
  }

  /// Create a draw target: a render target whose contents are an ordered,
  /// mutable list of draws (see `add_draw`/`remove_draw`), over color storage
  /// plus optional target-owned `depth` storage shared by every entry. The
  /// output registers exactly like every shader target (same texture id
  /// space; `<texture src>`, resize, and destroy all apply). With no entries
  /// a render is the clear alone. Entry order is draw order; the purity
  /// contract is unchanged - the list is input data, so a flush-rendered
  /// draw target re-renders whenever its entries or their inputs change.
  pub fn create_draw_target(&self, spec: TargetSpec, depth: bool) -> Result<u64, String> {
    self.gpu_limits().check_texture_size(spec.width, spec.height)?;
    validate_load(&spec)?;
    let id = self.textures.allocate_id();
    let (width, height, sampler) = (spec.width, spec.height, spec.sampler);
    let manual = spec.manual;
    let impeller = self.rpc(|reply| RasterCmd::CreateDrawTarget { id, spec, depth, reply })??;
    self.textures.insert(id, TextureEntry { impeller, width, height, sampler, format: TextureFormat::Rgba8 });
    self.targets.borrow_mut().insert(
      id,
      TargetMirror {
        uniforms: Rc::new(UniformTable::default()),
        draw: None,
        bounds: DrawBounds::default(),
        buffers: BufferIds::default(),
        entries: Some(DrawListMirror { depth, next_draw: 1, entries: HashMap::new() }),
      },
    );
    self.shader_sources.borrow_mut().insert(id, HashMap::new());
    if manual {
      self.manual_targets.borrow_mut().insert(id);
    }
    Ok(id)
  }

  /// Add a draw entry to a draw target: `entry.pipeline` draws
  /// `entry.buffer` over the target's shared storage, with its own params
  /// and sampler inputs - appended (drawing last in list order), or
  /// inserted immediately before entry `before` when given. Returns the
  /// entry's stable draw id (target-scoped, never reused), the handle every
  /// per-entry update takes. Fire-and-forget after validation: everything is
  /// checked here against the mirrors - unknown ids, depth compatibility (a
  /// depth-testing pipeline needs a target created with depth), draw-range
  /// bounds, uniform names and arities, per-entry texture-unit count, and
  /// sampling cycles - so errors throw at the call site. The caller must
  /// request a frame.
  pub fn add_draw(&self, target: u64, mut entry: DrawSpec, before: Option<u64>) -> Result<u64, String> {
    let mut targets = self.targets.borrow_mut();
    let mirror = targets.get_mut(&target).ok_or_else(|| format!("shader texture {target} not found"))?;
    let Some(list) = mirror.entries.as_mut() else {
      return Err(format!("target {target} is not a draw target (create it with createDrawTarget)"));
    };
    if let Some(before_id) = before {
      if !list.entries.contains_key(&before_id) {
        return Err(format!("draw {before_id} (before) not found on target {target}"));
      }
    }
    let (uniforms, stride, instance_stride, depth) = match self.pipeline_mirrors.borrow().get(&entry.pipeline) {
      Some(pm) => (pm.uniforms.clone(), pm.stride, pm.instance_stride, pm.depth),
      None => return Err(format!("pipeline {} not found", entry.pipeline)),
    };
    if depth && !list.depth {
      return Err(format!(
        "pipeline {} tests depth but target {target} has no depth buffer (create the draw target with depth: true)",
        entry.pipeline
      ));
    }
    let bounds = self.resolve_entry_range(&mut entry, stride, instance_stride)?;
    validate_params(&uniforms, &entry.params)?;
    validate_texture_bindings(&uniforms, &entry.textures)?;
    let draw_id = list.next_draw;
    self.validate_new_bindings(target, draw_id, &entry.textures)?;
    // The entry's effective inputs include the shared names its program
    // declares and does not bind itself (shared bindings live under entry
    // key 0): the unit budget must hold for the combination, checked here so
    // an over-budget add throws at its call site instead of dropping inputs
    // raster-side. The one place existing shared state gates an add.
    {
      let sources = self.shader_sources.borrow();
      let shared_extra = sources.get(&target).map_or(0, |c| {
        c.keys()
          .filter(|(e, name)| {
            *e == 0
              && uniforms.get(name.as_str()).is_some_and(|s| s.kind == UniformKind::Sampler2D)
              && !entry.textures.iter().any(|(n, _)| n == name)
          })
          .count()
      });
      self.gpu_limits().check_texture_units(entry.textures.len() + shared_extra)?;
    }
    list.next_draw += 1;
    list.entries.insert(draw_id, EntryMirror { uniforms, draw: entry.draw, bounds, buffers: entry.buffer_ids() });
    drop(targets);
    let mut sources = self.shader_sources.borrow_mut();
    let record = sources.entry(target).or_default();
    for (name, src) in &entry.textures {
      record.insert((draw_id, name.clone()), *src);
    }
    drop(sources);
    self.send(RasterCmd::AddDraw { target, draw: draw_id, entry, before });
    self.note_target_content(target);
    Ok(draw_id)
  }

  /// Reorder a draw target's list: `order` must name every current entry
  /// exactly once (a full permutation, validated here against the mirror).
  /// List order is draw order - later entries land over earlier ones where
  /// depth does not decide - so this is the sorting verb: opaque
  /// front-to-back, transparent back-to-front. Fire-and-forget; the caller
  /// must request a frame.
  pub fn set_draw_order(&self, target: u64, order: &[u64]) -> Result<(), String> {
    {
      let targets = self.targets.borrow();
      let mirror = targets.get(&target).ok_or_else(|| format!("shader texture {target} not found"))?;
      let Some(list) = mirror.entries.as_ref() else {
        return Err(format!("target {target} is not a draw target (create it with createDrawTarget)"));
      };
      validate_order(order, list.entries.keys().copied())?;
    }
    self.send(RasterCmd::SetDrawOrder { target, order: order.to_vec() });
    self.note_target_content(target);
    Ok(())
  }

  /// Remove a draw entry from a draw target; the remaining entries keep
  /// their order and ids. The removed id errors from then on (never reused).
  /// Fire-and-forget; the caller must request a frame.
  pub fn remove_draw(&self, target: u64, draw: u64) -> Result<(), String> {
    let mut targets = self.targets.borrow_mut();
    let mirror = targets.get_mut(&target).ok_or_else(|| format!("shader texture {target} not found"))?;
    let Some(list) = mirror.entries.as_mut() else {
      return Err(format!("target {target} is not a draw target (create it with createDrawTarget)"));
    };
    if list.entries.remove(&draw).is_none() {
      return Err(format!("draw {draw} not found on target {target}"));
    }
    drop(targets);
    if let Some(record) = self.shader_sources.borrow_mut().get_mut(&target) {
      record.retain(|(d, _), _| *d != draw);
    }
    self.send(RasterCmd::RemoveDraw { target, draw });
    self.note_target_content(target);
    Ok(())
  }

  /// Update one draw entry's params (the per-entry analog of
  /// `set_target_params`): validated against the entry's program, merged by
  /// name at the next render. The caller must request a frame.
  pub fn set_draw_params(&self, target: u64, draw: u64, params: &[(String, ParamValue)]) -> Result<(), String> {
    {
      let targets = self.targets.borrow();
      let entry = entry_mirror(&targets, target, draw)?;
      validate_params(&entry.uniforms, params)?;
    }
    self.send(RasterCmd::UpdateDrawParams { target, draw, params: params.to_vec() });
    self.note_target_content(target);
    Ok(())
  }

  /// Update a target's target-level params, routing by target kind. A
  /// single-program target (fragment texture, fixed pipeline target) has one
  /// pass, so target-level params ARE that pass's params: every name must be
  /// an active uniform with a matching component count (validated here,
  /// against the mirror, so the error lands at the call site; note a
  /// declared-but-optimized-out uniform reflects as absent and reports "no
  /// active uniform"), and the target re-renders (sampler inputs
  /// re-resolved) at the raster thread's next dirty flush, as do any targets
  /// sampling it, transitively. The output keeps its id and Impeller texture
  /// (no re-adoption); only the GL contents change.
  ///
  /// A draw target's target-level params are its SHARED params: values every
  /// entry reads - a camera's view-projection above all - folded by name like
  /// every params write and applied at render before each entry's own params,
  /// so an entry naming the same uniform overrides the shared value (specific
  /// beats general). Shared params are target state: they survive entry
  /// add/remove/rebuild. A target legitimately mixes material classes, so a
  /// name is applied where declared and skipped elsewhere (the iResolution
  /// rule), down to ZERO coverage: a name no current entry declares is
  /// stored and skips everywhere until a declaring entry arrives. That keeps
  /// shared state independent of write order - a value seeded before any
  /// entry exists and one written after entries attached are the same state -
  /// and lets a scene publish a standard set (camera position beside the
  /// view-projection) whatever materials are present. Validation is arity
  /// where declared: a name must match the declared component count in every
  /// entry program that declares it. The caller must request a frame.
  pub fn set_target_params(&self, target: u64, params: &[(String, ParamValue)]) -> Result<(), String> {
    {
      let targets = self.targets.borrow();
      let mirror = targets.get(&target).ok_or_else(|| format!("target {target} not found"))?;
      let Some(list) = mirror.entries.as_ref() else {
        validate_params(&mirror.uniforms, params)?;
        drop(targets);
        self.send(RasterCmd::UpdateShaderParams { id: target, params: params.to_vec() });
        self.note_target_content(target);
        return Ok(());
      };
      for (name, value) in params {
        for entry in list.entries.values() {
          validate_param_if_declared(&entry.uniforms, name, value)?;
        }
      }
    }
    self.send(RasterCmd::UpdateTargetParams { target, params: params.to_vec() });
    self.note_target_content(target);
    Ok(())
  }

  /// Rebind a target's target-level sampler2D inputs by uniform name,
  /// routing by target kind like `set_target_params`; bindings not named
  /// keep their current source, and the caller must request a frame. On a
  /// single-program target the bindings are the one pass's inputs, validated
  /// strictly against its uniform table. Every path errors if the target or
  /// any source texture id is unknown, or a binding would create a sampling
  /// cycle whose members are all flush-rendered targets (such a cycle is a
  /// feedback loop the flush cannot order). A cycle through a manual target
  /// is legal: the flush never renders one, so the loop is only ever stepped
  /// by explicit renders - ping-pong feedback is two manual targets bound to
  /// each other. Self-binding stays rejected for every target, manual
  /// included: a pass sampling the very texture it writes is a same-pass GL
  /// feedback loop (undefined pixels), not a scheduling problem.
  ///
  /// A draw target's target-level bindings are its SHARED bindings: sources
  /// every entry reads (an environment map, a shadow map, a LUT), written
  /// once per target. At render each entry gets the shared names its program
  /// declares and its own bindings do not override - an entry's own binding
  /// wins, and coverage may be partial, exactly like `set_target_params`.
  /// Shared bindings are target state: entry add/remove/rebuild cannot lose
  /// them. Validation: each name must be a sampler2D everywhere it is
  /// declared, and coverage may be ZERO exactly like `set_target_params` -
  /// an undeclared name is stored, joins the sampler graph, and binds when
  /// a declaring entry arrives. Every entry's effective input count (its
  /// own bindings plus the applicable merged shared set) must fit the
  /// device's texture units, and a shared edge counts for propagation and
  /// cycles even before any entry declares its name.
  pub fn set_target_textures(&self, target: u64, textures: &[(String, u64)]) -> Result<(), String> {
    {
      let targets = self.targets.borrow();
      let mirror = targets.get(&target).ok_or_else(|| format!("target {target} not found"))?;
      let Some(list) = mirror.entries.as_ref() else {
        validate_texture_bindings(&mirror.uniforms, textures)?;
        drop(targets);
        self.validate_new_bindings(target, 0, textures)?;
        let mut sources = self.shader_sources.borrow_mut();
        let record = sources.entry(target).or_default();
        for (name, src_id) in textures {
          record.insert((0, name.clone()), *src_id);
        }
        drop(sources);
        self.send(RasterCmd::UpdateShaderTextures { id: target, textures: textures.to_vec() });
        self.note_target_content(target);
        return Ok(());
      };
      for (name, _) in textures {
        for entry in list.entries.values() {
          if let Some(slot) = entry.uniforms.get(name) {
            if slot.kind == UniformKind::Inactive {
              continue;
            }
            if slot.kind != UniformKind::Sampler2D || slot.count > 1 {
              return Err(format!("uniform '{name}' is {}, not a sampler2D", slot.glsl_name()));
            }
          }
        }
      }
      // Per-entry unit budget against the MERGED shared set: an entry's
      // effective inputs are its own bindings plus the shared names its
      // program declares and does not bind itself.
      let sources = self.shader_sources.borrow();
      let record = sources.get(&target);
      let mut shared: Vec<&str> =
        record.map(|c| c.keys().filter(|(e, _)| *e == 0).map(|(_, n)| n.as_str()).collect()).unwrap_or_default();
      for (name, _) in textures {
        if !shared.contains(&name.as_str()) {
          shared.push(name);
        }
      }
      let limits = self.gpu_limits();
      for (draw_id, entry) in list.entries.iter() {
        let own_count = record.map_or(0, |c| c.keys().filter(|(e, _)| *e == *draw_id).count());
        let extra = shared
          .iter()
          .filter(|n| {
            entry.uniforms.get(**n).is_some_and(|s| s.kind == UniformKind::Sampler2D)
              && record.is_none_or(|c| !c.contains_key(&(*draw_id, (**n).to_string())))
          })
          .count();
        limits.check_texture_units(own_count + extra).map_err(|e| format!("draw {draw_id}: {e}"))?;
      }
    }
    self.validate_new_bindings(target, 0, textures)?;
    let mut sources = self.shader_sources.borrow_mut();
    let record = sources.entry(target).or_default();
    for (name, src_id) in textures {
      record.insert((0, name.clone()), *src_id);
    }
    drop(sources);
    self.send(RasterCmd::UpdateTargetTextures { target, textures: textures.to_vec() });
    self.note_target_content(target);
    Ok(())
  }

  /// Rebind one draw entry's sampler2D inputs by uniform name (the per-entry
  /// analog of `set_target_textures`); bindings not named keep their current
  /// source. Same checks as every bind path: names against the entry's
  /// program, per-entry unit count, source existence, cycles. The caller
  /// must request a frame.
  pub fn set_draw_textures(&self, target: u64, draw: u64, textures: &[(String, u64)]) -> Result<(), String> {
    let entry_uniforms = {
      let targets = self.targets.borrow();
      let entry = entry_mirror(&targets, target, draw)?;
      validate_texture_bindings(&entry.uniforms, textures)?;
      entry.uniforms.clone()
    };
    self.validate_new_bindings(target, draw, textures)?;
    // Combined with the target's shared bindings (entry key 0), the entry's
    // merged inputs must still fit the unit budget - the add_draw rule,
    // re-checked because a rebind can add names.
    {
      let sources = self.shader_sources.borrow();
      let record = sources.get(&target);
      let own = record.map_or(0, |c| c.keys().filter(|(e, _)| *e == draw).count())
        + textures.iter().filter(|(name, _)| record.is_none_or(|c| !c.contains_key(&(draw, name.clone())))).count();
      let shared_extra = record.map_or(0, |c| {
        c.keys()
          .filter(|(e, name)| {
            *e == 0
              && entry_uniforms.get(name.as_str()).is_some_and(|s| s.kind == UniformKind::Sampler2D)
              && !c.contains_key(&(draw, name.clone()))
              && !textures.iter().any(|(n, _)| n == name)
          })
          .count()
      });
      self.gpu_limits().check_texture_units(own + shared_extra)?;
    }
    let mut sources = self.shader_sources.borrow_mut();
    let record = sources.entry(target).or_default();
    for (name, src_id) in textures {
      record.insert((draw, name.clone()), *src_id);
    }
    drop(sources);
    self.send(RasterCmd::UpdateDrawTextures { target, draw, textures: textures.to_vec() });
    self.note_target_content(target);
    Ok(())
  }

  /// Update one draw entry's range and/or buffers (the per-entry `set_draw`):
  /// see `update_draw`. The caller must request a frame.
  pub fn set_draw_range(&self, target: u64, draw: u64, update: DrawUpdate) -> Result<(), String> {
    self.update_draw(target, Some(draw), update)
  }

  /// Apply a `DrawUpdate` to one entry - `draw` None addresses the
  /// single-draw kinds' one entry (the setDraw side), Some a draw target's
  /// entry (the setDrawRange side). One transaction: the buffer swap
  /// (replace-only, see `BufferIds::merged`) and the range merge are both
  /// validated against the resulting state - the merged range against the
  /// swapped buffers' sizes - before either commits, so an error leaves the
  /// entry exactly as it was. A swap alone keeps the current range, which
  /// must still fit the new buffers (a too-small buffer errors here; a
  /// larger one never does); a swap plus a range extends into the new
  /// buffer in one call. The growth primitive: a population outgrowing its
  /// instance buffer allocates a bigger one, writes it, swaps, and destroys
  /// the old (the entry holds the old buffer alive until the swap lands).
  fn update_draw(&self, target: u64, draw: Option<u64>, update: DrawUpdate) -> Result<(), String> {
    let mut targets = self.targets.borrow_mut();
    let mirror = targets.get_mut(&target).ok_or_else(|| format!("shader texture {target} not found"))?;
    let (ids, bounds, range) = match (draw, mirror.entries.as_mut()) {
      (None, Some(_)) => {
        return Err(format!("target {target} is a draw target; update draws per entry with setDrawRange"));
      }
      (Some(_), None) => {
        return Err(format!("target {target} is not a draw target (create it with createDrawTarget)"));
      }
      (None, None) => {
        let Some(range) = mirror.draw.as_mut() else {
          return Err("not a pipeline texture".to_string());
        };
        (&mut mirror.buffers, &mut mirror.bounds, range)
      }
      (Some(id), Some(list)) => {
        let entry = list.entries.get_mut(&id).ok_or_else(|| format!("draw {id} not found on target {target}"))?;
        (&mut entry.buffers, &mut entry.bounds, &mut entry.draw)
      }
    };
    let next_ids = ids.merged(update.buffers)?;
    let swapped = next_ids != *ids;
    let next_bounds = if swapped { self.rebound(*bounds, next_ids)? } else { *bounds };
    let next_range = range.merged(update, next_bounds.indexed)?;
    validate_draw_range(next_range, next_bounds)?;
    *ids = next_ids;
    *bounds = next_bounds;
    *range = next_range;
    drop(targets);
    if swapped {
      self.send(RasterCmd::SetDrawBuffers { target, draw, ids: next_ids });
    }
    match draw {
      None => self.send(RasterCmd::SetDraw { id: target, range: next_range }),
      Some(draw) => self.send(RasterCmd::SetDrawRange { target, draw, range: next_range }),
    }
    self.note_target_content(target);
    Ok(())
  }

  /// `bounds` re-sized for `ids`: the fetch bounds keep their strides (the
  /// pipeline layout and vocabulary are unchanged by a swap; only an index
  /// format change moves the element size) and take the named buffers'
  /// sizes. Errs on an id the buffer registry does not know.
  fn rebound(&self, bounds: DrawBounds, ids: BufferIds) -> Result<DrawBounds, String> {
    let sizes = self.buffer_sizes.borrow();
    let size_of = |id: u64, role: &str| sizes.get(&id).copied().ok_or_else(|| format!("{role} {id} not found"));
    let fetch = match bounds.fetch {
      None => None,
      Some((stride, _)) => Some(match ids.index {
        Some((id, format)) => (format.size() as usize, size_of(id, "index buffer")?),
        None => (stride, size_of(ids.buffer, "buffer")?),
      }),
    };
    let instance = match bounds.instance {
      None => None,
      Some((stride, _)) => Some((stride, size_of(ids.instance_buffer, "instance buffer")?)),
    };
    Ok(DrawBounds { fetch, indexed: bounds.indexed, instance })
  }

  /// Drop a shared program's registry entry and retire its id. Pipelines
  /// created from it keep rendering - they hold the program until they are
  /// destroyed - and the GL program is deleted once the last user is gone, so
  /// either destruction order is safe.
  pub fn destroy_shader_program(&self, id: u64) {
    self.program_uniforms.borrow_mut().remove(&id);
    self.send(RasterCmd::DestroyProgram { id });
  }

  /// Declare (or clear, with None) the window shader: the frame then resolves
  /// into the runtime-owned layer and `shader.program` draws over it into the
  /// window as the last step before present (see `WindowShader`). Fire-and-
  /// forget on the ordered frame channel, so a change lands cleanly between
  /// two frames; the raster thread holds the program while declared, so
  /// destroying its handle keeps the effect running until it is re-declared
  /// or cleared. The caller must request a frame. Errs on an unknown program
  /// handle, or on params/textures naming anything but the program's active
  /// uniforms (same call-site validation as the target paths; `uSource`,
  /// `uPrevious` and `iResolution` are runtime-filled and need no entry
  /// here - anything else the shader declares, a time uniform included, is
  /// app-driven through `params` like any other uniform).
  pub fn set_window_shader(&self, shader: Option<WindowShader>) -> Result<(), String> {
    if let Some(ws) = &shader {
      // The runtime-filled layers occupy units ahead of the declared inputs:
      // uSource always, uPrevious while declared.
      self.gpu_limits().check_texture_units(1 + usize::from(ws.previous) + ws.textures.len())?;
      let programs = self.program_uniforms.borrow();
      let uniforms = programs.get(&ws.program).ok_or_else(|| format!("program {} not found", ws.program))?;
      validate_params(uniforms, &ws.params)?;
      validate_texture_bindings(uniforms, &ws.textures)?;
    }
    self.send(RasterCmd::SetWindowShader { shader });
    Ok(())
  }

  /// Install (Some) or clear (None) the stats overlay: a small diagnostics
  /// quad the raster thread composites over every frame, after the window
  /// shader pass. Never part of the app's display list, so a window shader
  /// cannot warp it and updating it invalidates no retained frame state
  /// (an Impeller surface draw always clears its target, so the overlay is
  /// rasterized into a small retained layer once per declaration and drawn
  /// over FBO 0 as a blended copy pass each frame). Retained raster-side;
  /// send again to refresh the figures. The caller must request a frame for
  /// a visible change.
  pub fn set_stats_overlay(&self, overlay: Option<StatsOverlay>) {
    self.send(RasterCmd::SetStatsOverlay { overlay });
  }

  /// Render a manual target (`TargetSpec::manual`) once, now. Fire-and-forget
  /// on the ordered raster channel, so renders land in call order relative to
  /// every other GPU command - two renders of one target run twice, in order,
  /// and a readback issued after one observes its pass. Pending pure-target
  /// writes flush first, so the pass samples fresh inputs; targets sampling
  /// this one re-render at the next flush. The caller must request a frame
  /// for displayed output. Errs on an unknown id or a target the flush owns
  /// (a non-manual one, whose pass must stay a pure function of its inputs).
  pub fn render_target(&self, id: u64) -> Result<(), String> {
    if !self.targets.borrow().contains_key(&id) {
      return Err(format!("shader texture {id} not found"));
    }
    if !self.manual_targets.borrow().contains(&id) {
      return Err(format!("target {id} is not manual (the runtime renders it; create with render: \"manual\")"));
    }
    self.send(RasterCmd::RenderTarget { id });
    // A manual target's pixels change exactly here (and at copy_texture), so
    // this notes directly - note_target_content's manual skip is for the
    // writes that only stage state for a later render.
    self.note_content(id);
    Ok(())
  }

  /// Overwrite manual target `dst` with texture `src`'s current pixels: the
  /// GPU-side seed/history write, the copy analog of `update_texture`.
  /// Fire-and-forget on the ordered raster channel, so copies land in call
  /// order with renders and readbacks; the caller must request a frame for
  /// displayed output. Exact: sizes must match (an intentional tight
  /// contract - a scaling copy is an ordinary pass). Errs on unknown ids, a
  /// non-manual destination (the flush owns those contents), a size
  /// mismatch, or src == dst.
  pub fn copy_texture(&self, src: u64, dst: u64) -> Result<(), String> {
    let src_entry = self.textures.get(src).ok_or_else(|| format!("texture {src} not found"))?;
    let dst_entry = self.textures.get(dst).ok_or_else(|| format!("texture {dst} not found"))?;
    if !self.manual_targets.borrow().contains(&dst) {
      return Err(format!("target {dst} is not manual (the runtime renders it; create with render: \"manual\")"));
    }
    if src == dst {
      return Err(format!("cannot copy texture {src} into itself"));
    }
    let (sw, sh) = (src_entry.width(), src_entry.height());
    let (dw, dh) = (dst_entry.width(), dst_entry.height());
    if (sw, sh) != (dw, dh) {
      return Err(format!("size mismatch: source is {sw}x{sh}, destination is {dw}x{dh}"));
    }
    self.send(RasterCmd::CopyTexture { src, dst });
    self.note_content(dst);
    Ok(())
  }

  /// Update a pipeline texture's draw range - which vertices are drawn
  /// (`first_vertex`, `vertex_count`) and how many instances - and re-render
  /// it with its last-applied params. Fields absent from `update` keep their
  /// current value (the params merge rule), so the common case stays one
  /// field. The caller must request a frame. Errs on a negative field, or a
  /// vertex range whose fetch would run past the end of the target's buffer
  /// (undefined behaviour in raw GLES; validated against the bound captured
  /// at create, see `TargetMirror::bounds` - attributeless targets fetch
  /// nothing, so any non-negative range is safe there).
  pub fn set_draw(&self, id: u64, update: DrawUpdate) -> Result<(), String> {
    self.update_draw(id, None, update)
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
  /// snapshot boundaries re-render this way instead of reallocating).
  /// Storage is exact-size, so only reuse at the same `width` x `height`.
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

  /// Rasterize a shaded snapshot boundary's display list and run its node
  /// shader pass in one trip: the subtree renders into the source texture,
  /// then `shader.program` draws one fullscreen pass over it into the
  /// output, which the boundary composites in place of the raw snapshot.
  /// With `shader.previous`, `history` binds as `uPrevious` (created
  /// transparent when None) - the caller owns rotating source and history
  /// roles across calls. Pass Some(handles) to re-render in place; they must
  /// have been created by this method at the same `width` x `height` (only
  /// an exact dimension match reuses). Validates like `set_window_shader`:
  /// known program, params/textures naming active uniforms only (`uSource`,
  /// `uPrevious` and `iResolution` are runtime-filled).
  pub fn rasterize_shaded(
    &self,
    dl: &DisplayList,
    width: u32,
    height: u32,
    aa: bool,
    shader: &NodeShader,
    source: Option<&Texture>,
    output: Option<&Texture>,
    history: Option<&Texture>,
  ) -> Result<(Texture, Texture, Option<Texture>), String> {
    self.validate_node_shader(shader)?;
    self.rpc(|reply| RasterCmd::RasterizeDlShaded {
      dl: dl.clone(),
      width,
      height,
      aa,
      shader: shader.clone(),
      source: source.cloned(),
      output: output.cloned(),
      history: history.cloned(),
      reply,
    })?
  }

  /// Re-run a node shader pass over an existing source/output pair from
  /// `rasterize_shaded` (plus the history binding while `previous` is
  /// declared): the declaration changed (the params path) while the
  /// boundary's content stayed valid. Fire-and-forget on the ordered raster
  /// channel, so the refreshed pixels land ahead of the frame that
  /// composites them; the caller owns requesting that frame.
  pub fn rerun_node_shader(
    &self,
    shader: &NodeShader,
    source: &Texture,
    output: &Texture,
    history: Option<&Texture>,
    width: u32,
    height: u32,
  ) -> Result<(), String> {
    self.validate_node_shader(shader)?;
    self.send(RasterCmd::RerunNodeShader {
      shader: shader.clone(),
      source: source.clone(),
      output: output.clone(),
      history: history.cloned(),
      width,
      height,
    });
    Ok(())
  }

  // Call-site validation for a node shader declaration, against the UI-side
  // mirrors (the same checks as the window shader): unit budget including
  // the runtime-filled uSource (and uPrevious while declared), a known
  // program, and params/textures naming its active uniforms only.
  fn validate_node_shader(&self, shader: &NodeShader) -> Result<(), String> {
    self.gpu_limits().check_texture_units(1 + usize::from(shader.previous) + shader.textures.len())?;
    let programs = self.program_uniforms.borrow();
    let uniforms = programs.get(&shader.program).ok_or_else(|| format!("program {} not found", shader.program))?;
    validate_params(uniforms, &shader.params)?;
    validate_texture_bindings(uniforms, &shader.textures)?;
    Ok(())
  }

  /// Rasterize a display list and read back exactly `width` x `height` RGBA8
  /// pixels (tightly packed top-to-bottom rows, same layout as
  /// `read_texture`). The raster thread rasterizes and reads back in one
  /// trip; the render target never leaves it and no texture is registered. A
  /// caller that wants a texture composes with `create_texture_from_pixels`.
  pub fn capture_node_pixels(&self, dl: &DisplayList, width: u32, height: u32) -> Result<Vec<u8>, String> {
    self.rpc(|reply| RasterCmd::RasterizeReadback { dl: dl.clone(), width, height, reply })?
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
  /// Record that texture `id`'s pixels changed (or will, at the next dirty
  /// flush) behind its unchanged id, plus everything downstream: every
  /// flush-rendered target sampling it re-renders, transitively (see
  /// `content_closure`). Inserting an already-noted id is a no-op, so a
  /// per-frame write burst on one target costs one closure walk. Drained by
  /// `take_content_changes`.
  fn note_content(&self, id: u64) {
    let mut changes = self.content_changes.borrow_mut();
    if !changes.insert(id) {
      return;
    }
    content_closure(&self.shader_sources.borrow(), &self.manual_targets.borrow(), id, &mut changes);
  }

  /// `note_content` for a mutated target: a manual target's pixels hold
  /// until an explicit render or copy steps it (those note then), so a
  /// params/entry/range write to one is not a content change yet.
  fn note_target_content(&self, id: u64) {
    if self.manual_targets.borrow().contains(&id) {
      return;
    }
    self.note_content(id);
  }

  /// `note_content` for a buffer write: every flush-rendered target drawing
  /// from the buffer re-renders with the new geometry, so each such target's
  /// pixels change content.
  fn note_buffer_content(&self, buffer: u64) {
    let affected: Vec<u64> = {
      let targets = self.targets.borrow();
      let manual = self.manual_targets.borrow();
      targets
        .iter()
        .filter(|(id, mirror)| {
          !manual.contains(id)
            && (mirror.buffers.reads(buffer)
              || mirror.entries.as_ref().is_some_and(|l| l.entries.values().any(|e| e.buffers.reads(buffer))))
        })
        .map(|(id, _)| *id)
        .collect()
    };
    for id in affected {
      self.note_content(id);
    }
  }

  /// Drain the texture ids whose pixels changed since the last drain. The
  /// frame build takes these before its clean check and applies them as
  /// damage on the snapshot boundaries that baked those pixels
  /// (`RenderTree::texture_content_changed`); everything else keeps live
  /// texture references and needs no damage for a content change.
  pub fn take_content_changes(&self) -> HashSet<u64> {
    std::mem::take(&mut *self.content_changes.borrow_mut())
  }

  pub fn destroy_texture(&self, id: u64) {
    let mut pending = self.pending_destroys.borrow_mut();
    if !pending.contains(&id) {
      pending.push(id);
    }
    // A YUV output takes its planes with it. They are never referenced by
    // the render tree, so they reclaim at the next sweep; the group is
    // removed now, so a late update_yuv errs instead of dirtying a target
    // whose planes are going away.
    if let Some(group) = self.yuv_groups.borrow_mut().remove(&id) {
      for (_, plane, _) in group.sets.into_iter().flatten() {
        if !pending.contains(&plane) {
          pending.push(plane);
        }
      }
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
      self.targets.borrow_mut().remove(&id);
      self.shader_sources.borrow_mut().remove(&id);
      self.manual_targets.borrow_mut().remove(&id);
      self.send(RasterCmd::DestroyTexture { id });
      false
    });
  }
}

/// The loadOp invariant behind both target create paths: loading the
/// previous contents makes render count observable, which only the app may
/// count - on a flush-rendered target the output would silently depend on
/// how often the flush happened to run.
fn validate_load(spec: &TargetSpec) -> Result<(), String> {
  if spec.load && !spec.manual {
    return Err("loadOp \"load\" requires render: \"manual\" (a runtime-rendered target must stay a pure function of its inputs)".to_string());
  }
  Ok(())
}

// The entry mirror for (target, draw), sharing the error spelling of every
// per-entry path.
fn entry_mirror(targets: &HashMap<u64, TargetMirror>, target: u64, draw: u64) -> Result<&EntryMirror, String> {
  let mirror = targets.get(&target).ok_or_else(|| format!("shader texture {target} not found"))?;
  let Some(list) = mirror.entries.as_ref() else {
    return Err(format!("target {target} is not a draw target (create it with createDrawTarget)"));
  };
  list.entries.get(&draw).ok_or_else(|| format!("draw {draw} not found on target {target}"))
}

/// Whether `to` is reachable from `from` (inclusive: `from == to` is a hit)
/// by following sampler edges in `sources` (target id -> its source id per
/// (draw entry, uniform name) binding) without passing through a node in
/// `barriers`: the sampling-cycle test behind every bind path. Barriers are
/// the manual targets - the flush never renders one, so a path through one
/// can never be part of a flush-ordered feedback loop and does not count.
/// Pure over the id graph, so it unit-tests without a Context.
pub(crate) fn samples_transitively(
  sources: &HashMap<u64, HashMap<(u64, String), u64>>,
  barriers: &HashSet<u64>,
  from: u64,
  to: u64,
) -> bool {
  let mut stack = vec![from];
  let mut visited: HashSet<u64> = HashSet::new();
  while let Some(node) = stack.pop() {
    if node == to {
      return true;
    }
    if visited.insert(node) && !barriers.contains(&node) {
      if let Some(srcs) = sources.get(&node) {
        stack.extend(srcs.values().copied());
      }
    }
  }
  false
}

/// Collect into `changes` the flush-rendered targets whose pixels change
/// when `root`'s content does: everything sampling it, transitively, walking
/// the sampler graph upstream-to-downstream. Manual targets stop the walk -
/// the flush never renders one, so its pixels hold until an explicit render
/// steps it (which notes content itself, resuming propagation from there).
/// `root` itself is the caller's call: a stepped manual target counts, a
/// written-but-manual one does not. Pure over the id graph, so it unit-tests
/// without a Context (like `samples_transitively`).
pub(crate) fn content_closure(
  sources: &HashMap<u64, HashMap<(u64, String), u64>>,
  manual: &HashSet<u64>,
  root: u64,
  changes: &mut HashSet<u64>,
) {
  let mut stack = vec![root];
  while let Some(id) = stack.pop() {
    for (target, bindings) in sources.iter() {
      if manual.contains(target) || changes.contains(target) {
        continue;
      }
      if bindings.values().any(|src| *src == id) {
        changes.insert(*target);
        stack.push(*target);
      }
    }
  }
}

