use impellers::DisplayList;
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::{mpsc, Arc};

use crate::audio::AudioRegistry;
use crate::camera::CameraRegistry;
use crate::gpu::{AttributeTable, GpuLimits, GpuResources, ShaderStage, TextureRegistry, UniformTable, WriteLeases};
use crate::microphone::MicrophoneRegistry;
use crate::raster::{RasterCmd, RasterCounters, RasterSender, RasterStats};
use crate::spatial::Spatial;

mod buffer;
mod capture;
pub(crate) mod content;
mod mirror;
mod order;
mod program;
mod spatial;
mod target;
mod texture;

pub use capture::{CaptureDone, CaptureInfo};

use mirror::{PipelineMirror, SubTargetMirror, TargetMirror};
use order::InstanceOrders;
use texture::YuvGroup;

// All GL work - texture uploads, shader passes, offscreen rasterization,
// compositing, present - runs on the raster thread, which owns the process's
// single GL context and single Impeller context (see raster.rs for why).
// Context is the UI thread's handle on it: methods marshal into RasterCmds,
// either fire-and-forget sends or blocking RPCs. The UI side keeps just enough
// bookkeeping (texture dims, shader kinds, buffer sizes) to validate ids and
// answer size queries without a round trip.
//
// The impl is split by concern across this module's files, named to pair
// with the raster-side gpu/ files where one exists: mirror.rs (the UI-side
// validation mirrors), texture.rs (pixel textures, YUV, destroy/reclaim),
// buffer.rs (vertex buffers and write leases), program.rs (stages,
// programs, pipelines), target.rs (render targets, draw lists,
// params/bindings/range updates), capture.rs (node captures and readbacks),
// spatial.rs (the spatial core's sink writer), content.rs (content-change
// tracking and the sampler-graph walks). This file holds the struct and the
// raster-channel plumbing.

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
  // Sub-target id -> its parent and rectangle (see `SubTargetMirror`); a
  // tile is in `targets` like any draw target but never in the texture
  // registry.
  sub_targets: RefCell<HashMap<u64, SubTargetMirror>>,
  // Depth texture id -> the draw target owning it (create_draw_target with
  // DepthStorage::Texture). A depth id is a registered, sampler-only id:
  // for every graph question (edges, cycles, content propagation,
  // reclamation) it stands for its owner (see source_of), and the paths
  // that would treat it as a texture of its own (destroy, readback, copy)
  // consult this map to refuse.
  depth_ids: RefCell<HashMap<u64, u64>>,
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
  /// Per linked program: its active vertex attributes, mirrored from the
  /// link reply so `program_attributes` answers without a raster round trip.
  program_attributes: RefCell<HashMap<u64, Rc<AttributeTable>>>,
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
  // Entries that declared an instance order and the buffers they order (see
  // context/order.rs); end_buffer_write gathers ordered publishes through it.
  orders: RefCell<InstanceOrders>,
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
  // Ids vended for runtime-owned textures (snapshot boundary rasterizations,
  // see `borrow_texture_id`). The app reads them like any id but does not own
  // them: `destroy_texture` on one is a caller error, and the owner releases
  // the id when it goes away.
  borrowed: RefCell<HashSet<u64>>,
  // Planar YUV textures (see yuv.rs): app-visible output id -> its plane
  // sets, for update_yuv, and for destroy_texture to take the planes down
  // with the output.
  yuv_groups: RefCell<HashMap<u64, YuvGroup>>,
  /// The spatial core (transform hierarchy + sinks); see `crate::spatial`.
  /// Its draw sinks resolve to this context's draw entries.
  spatial: RefCell<Spatial>,
}

/// An overlay declaration (see `Context::set_overlay`): the
/// overlay's display list, drawn with its content at the origin, plus the
/// window-space rectangle it composites into - physical pixels, top-left
/// origin, `width` x `height` also being the rasterized layer's size.
pub struct Overlay {
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
      sub_targets: RefCell::new(HashMap::new()),
      depth_ids: RefCell::new(HashMap::new()),
      content_changes: RefCell::new(HashSet::new()),
      program_uniforms: RefCell::new(HashMap::new()),
      program_attributes: RefCell::new(HashMap::new()),
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
      orders: RefCell::new(InstanceOrders::new()),
      limits: Cell::new(None),
      cameras: CameraRegistry::default(),
      microphones: MicrophoneRegistry::default(),
      audio: AudioRegistry::default(),
      capture_requests: RefCell::new(HashMap::new()),
      capture_ready: RefCell::new(Vec::new()),
      pending_destroys: RefCell::new(Vec::new()),
      borrowed: RefCell::new(HashSet::new()),
      yuv_groups: RefCell::new(HashMap::new()),
      spatial: RefCell::new(Spatial::new()),
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

  /// Install (Some) or clear (None) the overlay: a small diagnostics
  /// quad the raster thread composites over every frame, after the window
  /// shader pass. Never part of the app's display list, so a window shader
  /// cannot warp it and updating it invalidates no retained frame state
  /// (an Impeller surface draw always clears its target, so the overlay is
  /// rasterized into a small retained layer once per declaration and drawn
  /// over FBO 0 as a blended copy pass each frame). Retained raster-side;
  /// send again to refresh the figures. The caller must request a frame for
  /// a visible change.
  pub fn set_overlay(&self, overlay: Option<Overlay>) {
    self.send(RasterCmd::SetOverlay { overlay });
  }

  /// Inventory the GPU resources the raster thread tracks: registered
  /// textures, vertex buffers, and shader/pipeline targets with their
  /// bookkeeping (draw state, layout, bindings, current params - the most
  /// recent writes, which the next flush renders with). Sorted by id for
  /// stable output.
  pub fn depth_owner(&self, id: u64) -> Option<u64> {
    self.depth_ids.borrow().get(&id).copied()
  }

  /// The depth texture id of draw target `target`, when it has one.
  pub(super) fn depth_of(&self, target: u64) -> Option<u64> {
    self.targets.borrow().get(&target).and_then(|m| m.entries.as_ref()).and_then(|l| l.depth_texture)
  }

  /// What the sampler graph records for a binding to `id`: a depth id
  /// stands for its owner, the target whose render writes it, so every
  /// UI-side walk sees target-to-target edges. The binding itself keeps the
  /// raw id - the raster side resolves that to the depth GL name.
  pub(super) fn source_of(&self, id: u64) -> u64 {
    self.depth_owner(id).unwrap_or(id)
  }

  /// A binding to a depth id may not ask for linear filtering: without a
  /// comparison mode a depth texture is only sampling-complete at NEAREST
  /// (ES 3.0), so the override would read zero everywhere - a silent
  /// all-lit shadow map. Rejected here, at the call site, on every bind
  /// path.
  pub(super) fn check_depth_binding(&self, binding: &crate::gpu::TextureBinding) -> Result<(), String> {
    if binding.sampler.filter == Some(crate::gpu::SamplerFilter::Linear) {
      if let Some(owner) = self.depth_owner(binding.id) {
        return Err(format!(
          "sampler '{}' binds target {owner}'s depth texture with filter \"linear\": depth samples only at nearest (filter in the shader instead)",
          binding.name
        ));
      }
    }
    Ok(())
  }

  /// A `sampler2DShadow` uniform compares against depth, so only a depth id
  /// may back it - a color texture behind a comparison sampler is undefined
  /// GL. Checked where the program's uniform kinds are known, on every bind
  /// path a pipeline program can take; the reverse (a depth id on a plain
  /// sampler2D) stays legal - raw depth reads, the post-effect input.
  pub(super) fn check_compare_bindings(
    &self,
    uniforms: &crate::gpu::UniformTable,
    textures: &[crate::gpu::TextureBinding],
  ) -> Result<(), String> {
    for b in textures {
      if uniforms.get(&b.name).is_some_and(|s| s.kind == crate::gpu::UniformKind::Sampler2DShadow)
        && self.depth_owner(b.id).is_none()
      {
        return Err(format!(
          "uniform '{}' is a sampler2DShadow; bind a draw target's depth texture (depthTexture(target))",
          b.name
        ));
      }
    }
    Ok(())
  }

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
}
