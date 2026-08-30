//! The raster thread: sole owner of the process's GL context and Impeller
//! context. The UI (JS) thread builds display lists and value types only; every
//! GL operation arrives here as a `RasterCmd` over one ordered channel, so
//! parameter updates always apply before the frames that sample them. This is
//! Impeller's GLES contract (one context, created and used on exactly one
//! thread) and it is what keeps ANGLE / D3D11 and mobile GLES drivers stable;
//! see okf/backlog/angle-cross-context-impeller-textures.md.
//!
//! Two calling conventions:
//! - Fire-and-forget (frames, uploads, param writes, destroys): the UI thread
//!   sends and moves on. Invalid-id errors are logged here; the UI side
//!   validates what its own bookkeeping can catch before sending.
//! - Blocking RPC (creates, compiles, readbacks): the command carries a reply
//!   sender and the UI thread blocks on the reply. Rare or load-time ops only.
//!
//! Cross-thread handle lifetime: DisplayList/Texture are refcounted and
//! Send + Sync; a Texture handle dropped on the UI thread defers its GL
//! deletion to this context's reactor, which flushes on the next frame here
//! (verified by examples/xthread_release.rs).

mod capture;
mod cmd;

pub(crate) use cmd::RasterCmd;

use impellers::{Context as ImpellerContext, DisplayList, ISize, Texture};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc};

use crate::backend::{FrameOutput, GlBinding};
use crate::gl;
use crate::gpu::{
  release_buffer, release_pipeline, release_program, validate_params, validate_texture_bindings, AttributeTable,
  BufferIds, DepthStorage, DrawSpec, EntryBuffers, GpuBuffer, GpuBufferInfo, GpuLimits, GpuPipelineInfo,
  GpuProgramInfo, GpuRegionInfo, GpuRenderPipelineInfo, GpuResources, GpuTextureInfo, GpuWindowShaderInfo, ParamValue, PassInput,
  PassTimer, PipelineDesc, PipelineSpec, RenderPipeline, ShaderProgram, ShaderTexture, TargetSpec, TextureBinding,
  Timed, UniformTable, WindowShader,
};
use crate::gpu::{GpuTexture, SamplerCache, SamplerState, TextureFormat};
use capture::flip_for_fbo;

/// Counters shared between the raster thread, the frame loop, and the UI
/// thread's Context, one allocation for all of them. Diagnostics (get_stats)
/// read these live through the Context rather than from any frame-latched
/// snapshot, because a latch goes stale exactly when the raster thread wedges
/// and these numbers matter most (see
/// okf/backlog/idle-tick-gpu-backlog-runaway.md). All cumulative except
/// `queue_depth`; consumers diff between queries.
pub struct RasterStats {
  /// Raster commands sent but not yet executed (queued plus the one in
  /// hand): incremented by `RasterSender::send`, decremented by the command
  /// loop as each command finishes. 0 means the raster thread has nothing to
  /// do; the frame loop reads it to gate the idle Tick, since a backlogged
  /// raster thread produces no presents and is otherwise indistinguishable
  /// from a genuinely idle GPU.
  pub(crate) queue_depth: AtomicUsize,
  /// Idle Ticks emitted by the frame loop (app.rs increments). Ticks racing
  /// while `queue_depth` sits nonzero is the idle-tick-runaway signature.
  pub(crate) idle_ticks: AtomicU64,
  /// Present-fence timeouts: frames whose previous present had not retired
  /// within the fence timeout, i.e. the GPU is over budget and pacing was
  /// lost for that frame.
  pub(crate) fence_timeouts: AtomicU64,
  /// Shader/pipeline target renders executed by `flush_dirty`. Passes
  /// racing ahead of presented frames means redundant target re-renders
  /// (the ~900-passes-per-frame failure this counter exists to catch).
  pub(crate) passes: AtomicU64,
  /// Wall time spent issuing those passes, in microseconds. This is
  /// raster-thread occupancy (command issue plus any driver backpressure),
  /// not GPU-side duration - GL is asynchronous - but occupancy is the
  /// wedge signal: it is what starves presents.
  pub(crate) pass_issue_micros: AtomicU64,
  /// GPU-side execution time of those passes, in microseconds, from timer
  /// queries (gpu::PassTimer); the number `pass_issue_micros` is not. Lags
  /// the pass by a frame or two (results are harvested non-blocking) and
  /// stays at zero with `timer_queries` false.
  pub(crate) pass_exec_micros: AtomicU64,
  /// GPU-side execution time of the window draw of each presented frame
  /// (Impeller's display list plus the window shader composite, not the
  /// pass flush before it and not the present), microseconds, from the same
  /// timer queries. Per frame this is the number to hold against the
  /// refresh period; it is what explains `fence_timeouts`.
  pub(crate) frame_exec_micros: AtomicU64,
  /// Whether the raster context supports timer queries; set once by the
  /// raster thread at startup. False means `pass_exec_micros` is meaningless
  /// and reports as absent.
  pub(crate) timer_queries: AtomicBool,
  /// Wall time spent executing non-Frame raster commands, in microseconds:
  /// texture uploads, readbacks, offscreen rasterizations, compiles, param
  /// writes, and the pass flushes those commands trigger. This is the work
  /// no frame-phase timing sees - the raster thread can be seconds-per-frame
  /// busy here while frameMs reads healthy. Frame commands are excluded
  /// twice over: their phases are already timed (FrameTiming, get_stats
  /// frameMs), and their present blocks on vsync by design, which would read
  /// as busy on a perfectly healthy app.
  pub(crate) cmd_micros: AtomicU64,
}

/// A plain-data reading of `RasterStats`, taken at one instant. What
/// diagnostics record and report; every field but `queue` is cumulative, so
/// two readings give a rate over the span between them.
#[derive(Clone, Copy, Debug, Default)]
pub struct RasterCounters {
  /// Raster commands sent but not yet executed (queued plus the one in
  /// hand) at the instant of the reading; 0 means the raster thread is idle.
  pub queue: usize,
  /// Idle Ticks the frame loop has emitted.
  pub idle_ticks: u64,
  /// Present-fence timeouts: frames where the GPU was over budget.
  pub fence_timeouts: u64,
  /// Shader/pipeline target passes executed on the raster thread.
  pub passes: u64,
  /// Raster-thread wall time spent issuing those passes, microseconds
  /// (occupancy, not GPU-side duration).
  pub pass_issue_micros: u64,
  /// GPU-side execution time of those passes, microseconds, from timer
  /// queries; None when the context has no timer queries.
  pub pass_exec_micros: Option<u64>,
  /// GPU-side execution time of the window draws, microseconds; None when
  /// the context has no timer queries.
  pub frame_exec_micros: Option<u64>,
  /// Raster-thread wall time spent executing non-Frame commands,
  /// microseconds - the work no frame-phase timing sees.
  pub cmd_micros: u64,
}

impl RasterStats {
  pub(crate) fn sample(&self) -> RasterCounters {
    RasterCounters {
      queue: self.queue_depth.load(Ordering::Acquire),
      idle_ticks: self.idle_ticks.load(Ordering::Relaxed),
      fence_timeouts: self.fence_timeouts.load(Ordering::Relaxed),
      passes: self.passes.load(Ordering::Relaxed),
      pass_issue_micros: self.pass_issue_micros.load(Ordering::Relaxed),
      pass_exec_micros: self
        .timer_queries
        .load(Ordering::Relaxed)
        .then(|| self.pass_exec_micros.load(Ordering::Relaxed)),
      frame_exec_micros: self
        .timer_queries
        .load(Ordering::Relaxed)
        .then(|| self.frame_exec_micros.load(Ordering::Relaxed)),
      cmd_micros: self.cmd_micros.load(Ordering::Relaxed),
    }
  }

  pub(crate) fn new() -> Self {
    RasterStats {
      queue_depth: AtomicUsize::new(0),
      idle_ticks: AtomicU64::new(0),
      fence_timeouts: AtomicU64::new(0),
      passes: AtomicU64::new(0),
      pass_issue_micros: AtomicU64::new(0),
      pass_exec_micros: AtomicU64::new(0),
      frame_exec_micros: AtomicU64::new(0),
      timer_queries: AtomicBool::new(false),
      cmd_micros: AtomicU64::new(0),
    }
  }
}

/// A producer's half of the raster command channel, paired with the shared
/// counters for the queue-depth bookkeeping (see `RasterStats::queue_depth`).
/// The UI thread's Context holds one; the platform loop holds a clone for
/// surface-liveness rebinds (see liveness.rs).
#[derive(Clone)]
pub(crate) struct RasterSender {
  tx: mpsc::Sender<RasterCmd>,
  stats: Arc<RasterStats>,
}

impl RasterSender {
  pub(crate) fn new(tx: mpsc::Sender<RasterCmd>, stats: Arc<RasterStats>) -> Self {
    RasterSender { tx, stats }
  }

  /// Increment-before-send so the counter never under-reports: the raster
  /// thread may consume and decrement the instant the command is in the
  /// channel.
  pub(crate) fn send(&self, cmd: RasterCmd) -> Result<(), mpsc::SendError<RasterCmd>> {
    self.stats.queue_depth.fetch_add(1, Ordering::Release);
    let result = self.tx.send(cmd);
    if result.is_err() {
      self.stats.queue_depth.fetch_sub(1, Ordering::Release);
    }
    result
  }
}

// Consecutive failed presents that confirm the GL context is gone for good.
// Two, not more: a demand-driven app may attempt very few presents after the
// loss (observed frozen-window traces stopped at two), so a higher threshold
// can leave a dead window open forever; one tolerated failure covers a
// transient glitch.
const PRESENT_FAILURE_EXIT_THRESHOLD: u32 = 2;

// Upper bound on the present-fence wait (see the `present_fences` field); a
// fence this late means the GPU is stalled and the frame is lost to the
// slow-frame path anyway, so give up and draw.
const PRESENT_FENCE_TIMEOUT_NS: i32 = 100_000_000;

// Undisplayed frames allowed in flight before the draw blocks on the oldest
// fence. Two, not one: the fence covers not just GPU execution but the
// compositor returning the swapped buffer, and on slow-GPU/TV targets that
// present latency runs 4-5 vsyncs - with depth 1 the wait expired at the
// timeout on EVERY frame of a saturated 50 Hz TV (measured ~8-9 timeouts/s
// at 7-8 fps, ~100 of the ~140 ms frame period pure capped wait buying no
// pacing at all). Depth 2 overlaps the next draw with that latency while
// still capping ahead-of-glass depth below the driver queue's 2-3. Cost: on
// fast-GPU desktops (fences signal in ms, fenceTimeouts reads 0) the CPU may
// run one frame further ahead of glass; if that ever shows up in drag
// latency, the fallback is adaptive depth - see
// okf/backlog/adaptive-present-fence-depth.md.
const PRESENT_FENCE_DEPTH: usize = 2;

pub(crate) struct RasterState {
  gl: glow::Context,
  impeller_ctx: ImpellerContext,
  // The context binding this thread draws through: bind/rebind, present,
  // proc-address (see backend::GlBinding).
  binding: Box<dyn GlBinding>,
  // Physical framebuffer size (see backend::pack_size), published by the main
  // thread on resize and read when wrapping FBO 0.
  surface_size: Arc<AtomicU64>,
  // The four shared GL sampler objects alloy's passes bind per sampled input
  // (see SamplerCache for why texture-object state cannot carry this).
  samplers: SamplerCache,
  /// Timer queries around every pass (see gpu::PassTimer); harvested at the
  /// top of each raster command into `stats.pass_exec_micros` and the
  /// per-target counters.
  pass_timer: PassTimer,
  // The device ceilings, queried once at startup and served to the UI thread
  // over the Limits RPC (Context caches the reply for call-site validation).
  limits: GpuLimits,
  // Retained FBOs and scratch storage for every rig rasterization: the
  // window frame itself plus offscreen rasters (snapshot boundaries, node
  // captures), grown to the largest allocation requested. Retained because
  // a per-call allocate/release cycle is exactly what ANGLE/D3D11 handles
  // poorly (see the OffscreenRig doc in gl/rig.rs).
  offscreen_rig: gl::OffscreenRig,
  // Size of the last drawn frame, so geometry transitions are logged exactly
  // once. Diagnostic only (resize-race visibility).
  last_size: ISize,
  // Playback mode: a frame is read back and shipped instead of presented.
  capture_frames: bool,
  // Consecutive failed presents; a short streak confirms context loss.
  present_failures: u32,
  // Instant of the last slow-frame warning, for the 1/s rate limit.
  slow_frame_log: Option<std::time::Instant>,
  // Instant of the last fence-timeout/-failure warning, same rate limit.
  fence_wait_log: Option<std::time::Instant>,
  // Shared live counters (see RasterStats): this thread decrements the queue
  // depth per executed command and accumulates fence timeouts and pass
  // count/time; get_stats reads them through the Context.
  stats: Arc<RasterStats>,
  // Once-per-second frame phase trace (see FrameTiming).
  timing: FrameTiming,
  // Fences signaled as each present's GPU work completes, awaited before a
  // draw once PRESENT_FENCE_DEPTH are outstanding. Vsync alone lets the CPU
  // swap several frames ahead of what is on glass (Android's BufferQueue
  // runs 2-3 deep, desktop driver queues similarly), and that queue depth is
  // direct input-to-photon latency; the fences cap undisplayed frames in
  // flight below the driver's own depth. Empty in capture mode, which never
  // presents.
  present_fences: std::collections::VecDeque<glow::Fence>,
  // GL-side view of every registered texture (id -> name + dims), for sampler
  // resolution, re-uploads, and readbacks. Mirrors the UI side's registry
  // through the command stream.
  textures: HashMap<u64, GpuTexture>,
  // Compiled shader targets keyed by the texture id their output is
  // registered under.
  shaders: HashMap<u64, ShaderTexture>,
  // Draw target id -> its sub-targets in creation order (the group order of
  // its pass). A sub-target is in `shaders` (every per-target command
  // routes to it unchanged) but never in `textures`: it has no texture of
  // its own, the parent's is what everything samples.
  regions: HashMap<u64, Vec<u64>>,
  // Draw targets with a depth texture: target id -> the depth's registry
  // id (its GL name lives in `textures` like any other), and the reverse
  // for the flush graph, where a binding to a depth id is an edge to the
  // target that renders it.
  target_depths: HashMap<u64, u64>,
  depth_owners: HashMap<u64, u64>,
  // Shared shader/pipeline programs in their own id space. Pipelines and
  // targets hold their program by Rc, so removal here only deletes the GL
  // program once no user is left (see gpu::release_program).
  programs: HashMap<u64, Rc<ShaderProgram>>,
  // Shared render pipelines (program + draw state) in their own id space.
  // Targets hold their pipeline by Rc, like programs.
  render_pipelines: HashMap<u64, Rc<RenderPipeline>>,
  // Raw compiled stages in their own id space, inputs to LinkProgram. The GL
  // shader object is deleted on DestroyStage; linked programs are unaffected.
  stages: HashMap<u64, crate::gpu::CompiledStage>,
  // Vertex buffers pipelines draw from, in their own id space. Targets hold
  // their buffer by Rc, like programs and pipelines, so removal here only
  // deletes the GL buffer once no target draws from it (see
  // gpu::release_buffer).
  buffers: HashMap<u64, Rc<GpuBuffer>>,
  // The declared window shader, with its retained layer texture. None = the
  // frame resolves straight to FBO 0 (the free path).
  window_shader: Option<WindowShaderState>,
  // The installed overlay, composited over every frame after the
  // window shader pass (never part of the app's display list, so a window
  // shader cannot warp it and the frame never samples it).
  overlay: Option<OverlayState>,
  // The shared fullscreen copy program behind CopyTexture (fragColor =
  // texture(uSrc, vUV)), compiled on first use and kept for the thread's
  // life. Rc because ShaderProgram release goes through release_program.
  copy_program: Option<Rc<ShaderProgram>>,
  // The tile-clear program (see pass::DrawGroup), compiled on first use by
  // a parent render with sub-targets.
  tile_clear_program: Option<Rc<ShaderProgram>>,
  // Texture ids whose content changed (or, for shader targets, whose own
  // params/bindings/geometry changed) since the last dirty flush. Writes only
  // mark; flush_dirty renders the affected shader targets in dependency order
  // at the points pixels become observable (a drawn frame, an offscreen
  // rasterization, a readback). This is what makes target chains propagate,
  // and it renders each target at most once per flush no matter how many
  // writes landed.
  dirty: HashSet<u64>,
  // Sampled content may have changed since the last layer resolve: set by
  // every command except Frame/SetWindowShader (and by any non-clean Frame),
  // cleared by a resolve. While set, a clean-tree frame may not skip its
  // resolve. Starts true (nothing resolved yet).
  content_dirty: bool,
  // Shaded frames that skipped the tree raster and ran only the pass over
  // the retained layer (the clean-tree fast path). Reported in
  // GpuWindowShaderInfo for verification.
  pass_only_frames: u64,
  tx: mpsc::Sender<FrameOutput>,
  // Wakes the main thread's event wait after a present; None in playback
  // mode, whose capture loop blocks on the channel directly.
  wake: Option<Box<dyn Fn() + Send + Sync>>,
}

/// The active window shader: the declared spec, the program it resolved to
/// (held by Rc so DestroyProgram cannot pull it out from under the pass), and
/// the retained layer target the frame resolves into. The layer is allocated
/// by the first shaded frame, reallocated on window resize, and freed when
/// the shader is cleared; it is never adopted into Impeller and has no
/// registry id - it exists only as the source of the shader pass.
struct WindowShaderState {
  spec: WindowShader,
  program: Rc<ShaderProgram>,
  layer: Option<LayerTarget>,
  /// The `uPrevious` history layer (spec.previous): holds the last resolved
  /// frame, rotated with `layer` before each resolve. Same ownership rules as
  /// `layer`; freed when `previous` is withdrawn or the shader clears.
  prev_layer: Option<LayerTarget>,
}

struct LayerTarget {
  tex: glow::Texture,
  fbo: glow::Framebuffer,
  width: u32,
  height: u32,
}

/// The installed overlay: the declaration from the UI thread plus the
/// small retained layer its display list is rasterized into. The layer is
/// redrawn only when the declaration changes (`stale`), so the per-frame
/// cost is one blended copy draw; same ownership rules as the window
/// shader's layer.
struct OverlayState {
  decl: crate::context::Overlay,
  layer: Option<LayerTarget>,
  stale: bool,
}

/// Reply to an RPC; a dead requester (UI thread shutting down) is not an error.
fn reply<T>(tx: mpsc::Sender<T>, value: T) {
  tx.send(value).ok();
}

/// Get (compiling on first use) the shared fullscreen copy program
/// (`fragColor = texture(uSrc, vUV)`), used by CopyTexture and the stats
/// overlay composite. A free function over the slot so callers holding other
/// field borrows can reach it.
fn ensure_copy_program(gl: &glow::Context, slot: &mut Option<Rc<ShaderProgram>>) -> Result<Rc<ShaderProgram>, String> {
  if slot.is_none() {
    let program =
      ShaderProgram::new_fragment(gl, "uniform sampler2D uSrc;\nvoid main() { fragColor = texture(uSrc, vUV); }")?;
    *slot = Some(Rc::new(program));
  }
  Ok(slot.as_ref().expect("copy program just ensured").clone())
}

/// Get (compiling on first use) the tile-clear program (see
/// `pass::TILE_CLEAR_FRAGMENT`); a compile failure is logged once and the
/// slot stays empty, so parents render their tiles without the wipe.
fn ensure_tile_clear_program(gl: &glow::Context, slot: &mut Option<Rc<ShaderProgram>>) -> Option<Rc<ShaderProgram>> {
  if slot.is_none() {
    match ShaderProgram::new_fragment(gl, crate::gpu::TILE_CLEAR_FRAGMENT) {
      Ok(program) => *slot = Some(Rc::new(program)),
      Err(e) => log::error!("[alloy] tile clear program failed to compile: {e}"),
    }
  }
  slot.clone()
}

/// Rolling once-per-second trace of the interactive frame's native phases
/// (drag latency diagnostics): average fence wait, draw, and present (swap
/// block) times, plus the worst present. Logs only in seconds where frames
/// were drawn, so an idle app stays silent. A present averaging near a full
/// refresh period means swaps block on a saturated present queue.
struct FrameTiming {
  since: std::time::Instant,
  wait: f32,
  draw: f32,
  present: f32,
  present_max: f32,
  frames: u32,
}

impl FrameTiming {
  fn new() -> Self {
    FrameTiming { since: std::time::Instant::now(), wait: 0.0, draw: 0.0, present: 0.0, present_max: 0.0, frames: 0 }
  }

  fn record(&mut self, wait_ms: f32, draw_ms: f32, present_ms: f32) {
    self.wait += wait_ms;
    self.draw += draw_ms;
    self.present += present_ms;
    self.present_max = self.present_max.max(present_ms);
    self.frames += 1;
    if self.since.elapsed().as_secs_f32() >= 1.0 {
      let n = self.frames as f32;
      log::debug!(
        "[alloy] raster: {} frames/s, wait {:.1}ms, draw {:.1}ms, present {:.1}ms (max {:.1}ms)",
        self.frames,
        self.wait / n,
        self.draw / n,
        self.present / n,
        self.present_max
      );
      *self = FrameTiming::new();
    }
  }
}

impl RasterState {
  #[allow(clippy::too_many_arguments)]
  pub(crate) fn new(
    gl: glow::Context,
    impeller_ctx: ImpellerContext,
    binding: Box<dyn GlBinding>,
    surface_size: Arc<AtomicU64>,
    capture_frames: bool,
    stats: Arc<RasterStats>,
    tx: mpsc::Sender<FrameOutput>,
    wake: Option<Box<dyn Fn() + Send + Sync>>,
  ) -> Self {
    let limits = GpuLimits::query(&gl);
    let samplers = SamplerCache::new(&gl, limits.max_anisotropy);
    if limits.max_anisotropy > 1 {
      log::info!("[alloy] anisotropic filtering up to {}x (EXT_texture_filter_anisotropic)", limits.max_anisotropy);
    } else {
      log::info!("[alloy] anisotropic filtering unavailable (no EXT_texture_filter_anisotropic)");
    }
    let pass_timer = PassTimer::new(&gl);
    stats.timer_queries.store(pass_timer.supported(), Ordering::Relaxed);
    RasterState {
      gl,
      impeller_ctx,
      binding,
      surface_size,
      samplers,
      pass_timer,
      limits,
      offscreen_rig: gl::OffscreenRig::new(),
      last_size: ISize::new(0, 0),
      capture_frames,
      present_failures: 0,
      slow_frame_log: None,
      fence_wait_log: None,
      stats,
      timing: FrameTiming::new(),
      present_fences: std::collections::VecDeque::new(),
      textures: HashMap::new(),
      shaders: HashMap::new(),
      target_depths: HashMap::new(),
      depth_owners: HashMap::new(),
      programs: HashMap::new(),
      render_pipelines: HashMap::new(),
      stages: HashMap::new(),
      buffers: HashMap::new(),
      dirty: HashSet::new(),
      regions: HashMap::new(),
      window_shader: None,
      overlay: None,
      copy_program: None,
      tile_clear_program: None,
      content_dirty: true,
      pass_only_frames: 0,
      tx,
      wake,
    }
  }

  /// The thread loop: block for the next command, drain everything queued
  /// behind it, execute in order. In interactive mode only the newest queued
  /// Frame draws (superseded frames drop); the commands between frames still
  /// apply in order, so state is never sampled out of sequence. Exits when the
  /// UI thread drops its sender or the main loop stops receiving frames.
  pub(crate) fn run(mut self, rx: mpsc::Receiver<RasterCmd>) {
    'outer: loop {
      let first = match rx.recv() {
        Ok(cmd) => cmd,
        Err(_) => break,
      };
      let batch: Vec<RasterCmd> = std::iter::once(first).chain(rx.try_iter()).collect();
      let last_frame =
        if self.capture_frames { None } else { batch.iter().rposition(|cmd| matches!(cmd, RasterCmd::Frame { .. })) };
      for (i, cmd) in batch.into_iter().enumerate() {
        // Any command that can change what a frame samples (texture uploads,
        // target renders, program changes, ...) invalidates the clean-tree
        // fast path until the next real resolve. Frame itself, window shader
        // redeclarations (the very writes the fast path exists for), and the
        // overlay (drawn post-pass into FBO 0, never sampled by the
        // frame) are the only exempt commands; a shader *program* change
        // still invalidates through its cleared layer.
        if !matches!(cmd, RasterCmd::Frame { .. } | RasterCmd::SetWindowShader { .. } | RasterCmd::SetOverlay { .. }) {
          self.content_dirty = true;
        }
        // Frames are excluded from cmd_micros (see RasterStats); everything
        // else the loop executes is otherwise invisible to timing.
        let timed = !matches!(cmd, RasterCmd::Frame { .. });
        let cmd_start = std::time::Instant::now();
        self.harvest_pass_timings();
        match cmd {
          RasterCmd::Frame { dl, tree_clean } => {
            // A load-shed frame that was not clean still changed the tree:
            // the frame that draws in its place must not skip the resolve.
            if !tree_clean {
              self.content_dirty = true;
            }
            if (self.capture_frames || Some(i) == last_frame) && self.frame(dl).is_err() {
              break 'outer; // main loop is gone
            }
          }
          RasterCmd::RebindWindowSurface => {
            // Event-driven (return-to-visible): a failure recorded against
            // the surface that died with the background (a frame in flight
            // at pause) is stale evidence; reset so the exit threshold
            // cannot misfire across a background/resume.
            if self.rebind_window_surface() {
              self.present_failures = 0;
            }
          }
          RasterCmd::CreateTexture { id, width, height, pixels, sampler, format, label, reply: tx } => {
            reply(tx, self.create_texture(id, width, height, &pixels, sampler, format, label));
          }
          RasterCmd::UpdateTexture { id, pixels } => {
            if let Err(e) = self.update_texture(id, &pixels) {
              log::warn!("[alloy] texture update failed: {e}");
            }
          }
          RasterCmd::UpdateYuv { planes, frame } => {
            for (id, offset) in planes {
              let len = match self.textures.get(&id) {
                Some(gpu) => (gpu.width as usize) * (gpu.height as usize) * gpu.format.bytes_per_pixel(),
                None => {
                  log::warn!("[alloy] yuv plane {id} not found");
                  continue;
                }
              };
              match frame.get(offset..offset.saturating_add(len)) {
                Some(plane) => {
                  if let Err(e) = self.update_texture(id, plane) {
                    log::warn!("[alloy] yuv plane update failed: {e}");
                  }
                }
                None => {
                  log::warn!("[alloy] yuv plane {id} needs {len} bytes at offset {offset}, frame has {}", frame.len());
                }
              }
            }
          }
          RasterCmd::CreateShaderTexture {
            id,
            width,
            height,
            fragment_src,
            params,
            textures,
            sampler,
            label,
            reply: tx,
          } => {
            reply(tx, self.create_shader_texture(id, width, height, &fragment_src, &params, textures, sampler, label));
          }
          RasterCmd::CreatePipelineTexture { id, spec, reply: tx } => {
            reply(tx, self.create_pipeline_texture(id, spec));
          }
          RasterCmd::CompileStage { id, stage, source, header, reply: tx } => {
            let result = crate::gpu::compile_stage(&self.gl, stage, &source, header).map(|shader| {
              self.stages.insert(id, shader);
            });
            reply(tx, result);
          }
          RasterCmd::LinkProgram { id, vertex, fragment, label, reply: tx } => {
            reply(tx, self.link_program(id, vertex, fragment, label));
          }
          RasterCmd::DestroyStage { id } => {
            if let Some(stage) = self.stages.remove(&id) {
              crate::gpu::delete_stage(&self.gl, stage.shader);
            }
          }
          RasterCmd::CreateRenderPipeline { id, program, desc, label, reply: tx } => {
            reply(tx, self.create_render_pipeline(id, program, desc, label));
          }
          RasterCmd::DestroyRenderPipeline { id } => {
            if let Some(pipeline) = self.render_pipelines.remove(&id) {
              release_pipeline(&self.gl, pipeline);
            }
          }
          RasterCmd::CreateShaderTarget { id, spec, entry, reply: tx } => {
            reply(tx, self.create_shader_target(id, spec, entry));
          }
          RasterCmd::CreateDrawTarget { id, depth_id, spec, depth, reply: tx } => {
            reply(tx, self.create_draw_target(id, depth_id, spec, depth));
          }
          RasterCmd::CreateSubTarget { id, parent, x, y, spec, reply: tx } => {
            reply(tx, self.create_sub_target(id, parent, x, y, spec));
          }
          RasterCmd::SetTargetRect { id, x, y, width, height } => {
            let parent = self.shaders.get(&id).and_then(|s| s.region().map(|r| r.parent));
            match (self.shaders.get_mut(&id), parent) {
              (Some(shader), Some(parent)) => {
                if let Err(e) = shader.set_region_rect(x, y, width, height) {
                  log::warn!("[alloy] set target rect failed: {e}");
                }
                self.dirty.insert(parent);
              }
              _ => log::warn!("[alloy] set target rect failed: target {id} is not a sub-target"),
            }
          }
          RasterCmd::AddDraw { target, draw, entry, before } => {
            if let Err(e) = self.add_draw(target, draw, entry, before) {
              log::warn!("[alloy] add draw failed: {e}");
            }
          }
          RasterCmd::SetDrawOrder { target, order } => {
            self.entry_write(target, "draw reorder", |_, shader| shader.set_entry_order(&order));
          }
          RasterCmd::RemoveDraw { target, draw } => {
            self.entry_write(target, "draw removal", |gl, shader| shader.remove_entry(gl, draw));
          }
          RasterCmd::UpdateDrawParams { target, draw, params } => {
            self.entry_write(target, "draw params update", |_, shader| shader.merge_entry_params(draw, &params));
          }
          RasterCmd::UpdateTargetParams { target, params } => {
            self.entry_write(target, "target params update", |_, shader| shader.merge_shared_params(&params));
          }
          RasterCmd::UpdateTargetTextures { target, textures } => {
            self.entry_write(target, "target texture rebind", |_, shader| shader.merge_shared_bindings(&textures));
          }
          RasterCmd::UpdateDrawTextures { target, draw, textures } => {
            self.entry_write(target, "draw texture rebind", |_, shader| shader.set_entry_bindings(draw, &textures));
          }
          RasterCmd::SetDrawRange { target, draw, range } => {
            self.entry_write(target, "draw range update", |_, shader| shader.set_entry_draw(draw, range));
          }
          RasterCmd::SetDrawBuffers { target, draw, ids } => {
            let buffers = resolve_entry_buffers(&self.buffers, ids);
            self.entry_write(target, "draw buffer swap", |gl, shader| shader.set_entry_buffers(gl, draw, buffers?));
          }
          RasterCmd::DestroyProgram { id } => {
            if let Some(program) = self.programs.remove(&id) {
              release_program(&self.gl, program);
            }
          }
          RasterCmd::SetWindowShader { shader } => {
            self.set_window_shader(shader);
          }
          RasterCmd::SetOverlay { overlay } => {
            self.set_overlay(overlay);
          }
          RasterCmd::UpdateShaderParams { id, params } => {
            // A manual target only folds the values (its pixels change on its
            // next explicit render, not here), so it is not marked dirty.
            match self.shaders.get_mut(&id) {
              Some(shader) => {
                shader.merge_params(&params);
                if !shader.manual() {
                  self.dirty.insert(id);
                }
              }
              None => log::warn!("[alloy] shader params update failed: shader texture {id} not found"),
            }
          }
          RasterCmd::UpdateShaderTextures { id, textures } => {
            let rebound = match self.shaders.get_mut(&id) {
              Some(shader) => shader.set_sampler_bindings(&textures).map(|()| shader.manual()),
              None => Err(format!("shader texture {id} not found")),
            };
            match rebound {
              Ok(manual) => {
                if !manual {
                  self.dirty.insert(id);
                }
              }
              Err(e) => log::warn!("[alloy] shader texture rebind failed: {e}"),
            }
          }
          RasterCmd::ResizeShaderTexture { id, width, height, reply: tx } => {
            reply(tx, self.resize_shader_texture(id, width, height));
          }
          RasterCmd::SetDraw { id, range } => {
            let result = self
              .shaders
              .get_mut(&id)
              .ok_or_else(|| format!("shader texture {id} not found"))
              .and_then(|shader| shader.set_draw(range).map(|()| shader.manual()));
            match result {
              Ok(manual) => {
                if !manual {
                  self.dirty.insert(id);
                }
              }
              Err(e) => log::warn!("[alloy] draw update failed: {e}"),
            }
          }
          RasterCmd::RenderTarget { id } => {
            // Fresh inputs first (the pixel-observer rule: this pass samples
            // its sources), then the one pass, then seed the dirty set so
            // targets sampling this one re-render at the next flush - the
            // same shape as an uploadTexture content change.
            self.flush_dirty();
            match self.shaders.get(&id) {
              Some(shader) => {
                let start = std::time::Instant::now();
                self.pass_timer.begin(&self.gl);
                shader.render(&self.gl, &|bindings| resolve_binding_list(&self.textures, &self.samplers, bindings));
                self.pass_timer.end(&self.gl, Timed::Pass { target: id });
                let micros = start.elapsed().as_micros() as u64;
                shader.record_pass(micros);
                self.stats.passes.fetch_add(1, Ordering::Relaxed);
                self.stats.pass_issue_micros.fetch_add(micros, Ordering::Relaxed);
                self.dirty.insert(id);
              }
              None => log::warn!("[alloy] render target failed: shader texture {id} not found"),
            }
          }
          RasterCmd::CopyTexture { src, dst } => {
            // Observes src's pixels, so flush first (the same rule as
            // RenderTarget); the copy itself then seeds dst into the dirty
            // set inside the helper.
            self.flush_dirty();
            if let Err(e) = self.copy_texture(src, dst) {
              log::warn!("[alloy] texture copy failed: {e}");
            }
          }
          RasterCmd::AdoptTexture { id, texture, width, height } => match capture::gl_name(&texture) {
            Ok(gl_texture) => {
              let label = Some("snapshot".to_string());
              self.textures.insert(
                id,
                GpuTexture {
                  gl_texture,
                  width,
                  height,
                  sampler: SamplerState::default(),
                  format: TextureFormat::Rgba8,
                  label,
                },
              );
              self.dirty.insert(id);
            }
            Err(e) => log::warn!("[alloy] adopt snapshot texture {id} failed: {e}"),
          },
          RasterCmd::DestroyTexture { id } => {
            self.textures.remove(&id);
            self.dirty.remove(&id);
            // The depth texture goes with its target (its name is
            // Impeller-owned like the color, so removal is bookkeeping).
            if let Some(depth_id) = self.target_depths.remove(&id) {
              self.textures.remove(&depth_id);
              self.depth_owners.remove(&depth_id);
            }
            // A parent takes its sub-targets with it; a sub-target leaves
            // its parent's group list and dirties the parent, whose next
            // full render clears the rectangle it drew.
            for tile in self.regions.remove(&id).unwrap_or_default() {
              self.dirty.remove(&tile);
              if let Some(shader) = self.shaders.remove(&tile) {
                shader.destroy(&self.gl);
              }
            }
            if let Some(shader) = self.shaders.remove(&id) {
              if let Some(region) = shader.region() {
                if let Some(tiles) = self.regions.get_mut(&region.parent) {
                  tiles.retain(|t| *t != id);
                }
                self.dirty.insert(region.parent);
              }
              shader.destroy(&self.gl);
            }
          }
          RasterCmd::CreateBuffer { id, data, label, reply: tx } => {
            reply(
              tx,
              GpuBuffer::new(&self.gl, &data, label).map(|buffer| {
                self.buffers.insert(id, Rc::new(buffer));
              }),
            );
          }
          RasterCmd::WriteBuffer { id, data, byte_offset } => {
            if let Err(e) = self.write_buffer(id, &data, byte_offset) {
              log::warn!("[alloy] buffer write failed: {e}");
            }
          }
          RasterCmd::WriteBufferLease { id, block, len, recycle } => {
            // The block is exclusively owned here (it moved across the
            // channel), so slicing it is sound. Recycle even on failure -
            // the pool, not this arm, decides a block's fate; a dead UI
            // side just drops it (send failing is shutdown, not an error).
            if let Err(e) = self.write_buffer(id, &block[..len], 0) {
              log::warn!("[alloy] buffer write failed: {e}");
            }
            let _ = recycle.send((id, block));
          }
          RasterCmd::ReadBuffer { id, byte_offset, len, reply: tx } => {
            let result = self
              .buffers
              .get(&id)
              .ok_or_else(|| format!("buffer {id} not found"))
              .and_then(|buffer| buffer.read(&self.gl, byte_offset, len));
            reply(tx, result);
          }
          RasterCmd::DestroyBuffer { id } => {
            if let Some(buffer) = self.buffers.remove(&id) {
              release_buffer(&self.gl, buffer);
            }
          }
          RasterCmd::RasterizeDl { dl, width, height, aa, reply: tx } => {
            self.flush_dirty();
            reply(tx, self.rasterize(&dl, width, height, aa));
          }
          RasterCmd::RasterizeDlInto { dl, texture, width, height, aa, reply: tx } => {
            self.flush_dirty();
            reply(tx, self.rasterize_into(&dl, &texture, width, height, aa));
          }
          RasterCmd::RasterizeDlShaded { dl, width, height, aa, shader, source, output, history, reply: tx } => {
            self.flush_dirty();
            reply(tx, self.rasterize_shaded(&dl, width, height, aa, &shader, source, output, history));
          }
          RasterCmd::RerunNodeShader { shader, source, output, history, width, height } => {
            // Observes its declared registry bindings (and the boundary's
            // retained source), so fresh inputs first - the same
            // pixel-observer rule as RenderTarget. Fire-and-forget: there is
            // no JS call site left, so a failure only warns and the boundary
            // keeps compositing the output's previous pixels.
            self.flush_dirty();
            if let Err(e) = self.node_shader_pass(&shader, &source, Some(output), history.as_ref(), width, height) {
              log::warn!("[alloy] node shader re-run failed: {e}");
            }
          }
          RasterCmd::RasterizeReadback { dl, width, height, reply: tx } => {
            self.flush_dirty();
            // Node captures carry no boundary prop, so they stay at full AA.
            let result = self.rasterize(&dl, width, height, true).and_then(|texture| {
              let size = ISize::new(width as i64, height as i64);
              gl::read_texture_pixels(&self.gl, &texture, size)
              // The intermediate texture drops here, on the context thread;
              // Impeller frees its GL name.
            });
            reply(tx, result);
          }
          RasterCmd::ReadTexture { texture, width, height, reply: tx } => {
            self.flush_dirty();
            let size = ISize::new(width as i64, height as i64);
            reply(tx, gl::read_texture_pixels(&self.gl, &texture, size));
          }
          RasterCmd::Resources { reply: tx } => {
            reply(tx, self.resources());
          }
          RasterCmd::Limits { reply: tx } => {
            reply(tx, self.limits);
          }
        }
        // The command is done (a load-shed one counts: it was consumed); the
        // frame-error exit above skips this, but the thread is gone then
        // anyway.
        if timed {
          self.stats.cmd_micros.fetch_add(cmd_start.elapsed().as_micros() as u64, Ordering::Relaxed);
        }
        self.stats.queue_depth.fetch_sub(1, Ordering::Release);
      }
    }
  }

  /// Draw the frame's display list to the window backbuffer and hand it on:
  /// present in interactive mode, read the pixels back in playback mode. Then
  /// notify the main loop, which only does frame bookkeeping (fps,
  /// FrameRendered) and playback encoding. Err means the main loop is gone
  /// and this thread should exit.
  fn frame(&mut self, dl: DisplayList) -> Result<(), ()> {
    // The frame samples shader targets (directly via <texture src>, or through
    // the window-shader layer); resolve every pending target write first.
    self.flush_dirty();
    let (width, height) = crate::backend::unpack_size(self.surface_size.load(Ordering::Acquire));
    let size = ISize::new(width as i64, height as i64);
    let wait_start = std::time::Instant::now();
    self.await_present_fence();
    let wait_ms = wait_start.elapsed().as_secs_f32() * 1000.0;
    let draw_start = std::time::Instant::now();
    self.pass_timer.begin(&self.gl);
    let drawn = self.draw_to_window(&dl, size);
    self.pass_timer.end(&self.gl, Timed::Frame);
    let draw_ms = draw_start.elapsed().as_secs_f32() * 1000.0;
    // The overlay composites over the finished frame (shaded or not),
    // before the capture readback so playback frames carry it too. Excluded
    // from draw_ms: it is diagnostics, not the app's frame cost.
    if drawn {
      self.draw_overlay(size);
    }
    if self.capture_frames {
      let pixels = if drawn { gl::read_fbo0_pixels(&self.gl, size) } else { Vec::new() };
      self.tx.send(FrameOutput::Captured(pixels)).map_err(|_| ())?;
    } else {
      let present_start = std::time::Instant::now();
      if drawn && !self.present() && self.rebind_window_surface() {
        // The failed swap's frame is lost with the dead binding (Android
        // replaces the EGL surface across background/resume, and a frame
        // latched by resize or expose can reach this thread before the
        // event-driven rebind). Redraw against the rebound surface and
        // present again; the retry's outcome feeds the failure threshold
        // honestly (fail, rebind, fail again = confirmed loss).
        if self.draw_to_window(&dl, size) {
          self.present();
        }
      }
      let present_ms = present_start.elapsed().as_secs_f32() * 1000.0;
      self.timing.record(wait_ms, draw_ms, present_ms);
      // A frame's native cost beyond ~2 vsync periods means this thread is
      // being stalled in the driver; log which step, rate-limited to one line
      // per second so a sustained stall stays readable. Debug, not warn: a
      // saturated tiled GPU (Android TV) lives here in steady state, and the
      // timing stats carry the numbers - raise SRT_LOG=debug to see these.
      if wait_ms + draw_ms + present_ms > 35.0 && self.slow_frame_log.is_none_or(|t| t.elapsed().as_secs() >= 1) {
        self.slow_frame_log = Some(std::time::Instant::now());
        log::debug!("[alloy] slow frame: fence wait {wait_ms:.1}ms, draw {draw_ms:.1}ms, present {present_ms:.1}ms");
      }
      // Resize-race diagnostics: the published surface size moved while this
      // frame was drawing, so what just reached the screen already has stale
      // geometry. The resize settle window (lattice) repaints behind it.
      let (now_w, now_h) = crate::backend::unpack_size(self.surface_size.load(Ordering::Acquire));
      if (now_w as i64, now_h as i64) != (width as i64, height as i64) {
        log::warn!("[alloy] surface size changed during frame: drew {width}x{height}, now {now_w}x{now_h}");
      }
      self.tx.send(FrameOutput::Presented).map_err(|_| ())?;
    }
    // Wake only after the frame is in the channel, so the woken loop finds it.
    if let Some(wake) = &self.wake {
      wake();
    }
    Ok(())
  }

  /// Block until outstanding presents are back under PRESENT_FENCE_DEPTH (or
  /// the timeout passes per fence), consuming the awaited fences. See the
  /// `present_fences` field. A timeout is the "GPU is over budget for a full
  /// refresh period and then some" signal - pacing is lost for this frame
  /// (we draw anyway; hanging the raster thread would be worse). Counted for
  /// get_stats (fenceTimeouts) and logged at debug 1/s: a healthy discrete
  /// GPU never hits this while a saturated tiled one (Android TV) lives near
  /// it in steady state, so the counter is the observability and the log
  /// line is SRT_LOG=debug diagnosis material (see
  /// okf/backlog/idle-tick-gpu-backlog-runaway.md, present-fence finding).
  fn await_present_fence(&mut self) {
    while self.present_fences.len() >= PRESENT_FENCE_DEPTH {
      let fence = self.present_fences.pop_front().expect("len checked above");
      let status = unsafe {
        let status =
          glow::HasContext::client_wait_sync(&self.gl, fence, glow::SYNC_FLUSH_COMMANDS_BIT, PRESENT_FENCE_TIMEOUT_NS);
        glow::HasContext::delete_sync(&self.gl, fence);
        status
      };
      match status {
        glow::ALREADY_SIGNALED | glow::CONDITION_SATISFIED => {}
        status => {
          if status == glow::TIMEOUT_EXPIRED {
            self.stats.fence_timeouts.fetch_add(1, Ordering::Relaxed);
          }
          if self.fence_wait_log.is_none_or(|t| t.elapsed().as_secs() >= 1) {
            self.fence_wait_log = Some(std::time::Instant::now());
            if status == glow::TIMEOUT_EXPIRED {
              log::debug!(
                "[alloy] present fence timed out after {}ms: GPU over budget, pacing lost this frame",
                PRESENT_FENCE_TIMEOUT_NS / 1_000_000
              );
            } else {
              log::warn!("[alloy] present fence wait failed (status {status:#x})");
            }
          }
        }
      }
    }
  }

  /// Rasterize the display list through the retained rig and resolve it into
  /// FBO 0; true when a frame reached the backbuffer. False skips the frame
  /// (a zero-sized minimized window, or a failed draw) - the caller still
  /// notifies, so lockstep consumers (playback) never stall.
  fn draw_to_window(&mut self, dl: &DisplayList, size: ISize) -> bool {
    // Resize-race diagnostics: geometry transitions as this thread sees them,
    // once per size.
    if self.last_size.width != size.width || self.last_size.height != size.height {
      log::info!(
        "[alloy] frame size {}x{} -> {}x{}",
        self.last_size.width,
        self.last_size.height,
        size.width,
        size.height
      );
      self.last_size = size;
    }
    if size.width <= 0 || size.height <= 0 {
      return false;
    }
    if self.window_shader.is_some() {
      match self.draw_to_window_shaded(dl, size) {
        Ok(()) => return true,
        // Fall back to the plain path so the app stays visible; the layer or
        // pass failure is a diagnostic, not a black window.
        Err(e) => log::warn!("[alloy] window shader pass failed: {e}; drawing without it"),
      }
    }
    match gl::render_display_list_to_window(&self.gl, &mut self.impeller_ctx, &mut self.offscreen_rig, dl, size) {
      Ok(()) => true,
      Err(e) => {
        log::warn!("[alloy] frame draw failed at {}x{}: {e}; skipping frame", size.width, size.height);
        false
      }
    }
  }

  /// Shader-active frame: rasterize the display list - flipped, so the layer
  /// reads top-left origin like every sampled texture - through the rig into
  /// the retained layer, then draw the window shader program over it straight
  /// into FBO 0 (no intermediate target, no closing blit). The program's
  /// vertex stage is what flips back to window orientation. `spec.previous`
  /// retains last frame's resolve as a second layer bound as `uPrevious`.
  fn draw_to_window_shaded(&mut self, dl: &DisplayList, size: ISize) -> Result<(), String> {
    let (width, height) = (size.width as u32, size.height as u32);
    let state = self.window_shader.as_mut().expect("shaded draw requires a declared window shader");

    // The clean-tree fast path: the submit declared the display list
    // unchanged (see Context::submit_clean), nothing content-bearing arrived
    // since the last resolve, and the retained layer matches the window - so
    // the layer already holds this frame's pixels and only the pass needs to
    // run. History frames never skip: uPrevious must track the last frame,
    // and a skipped resolve would freeze it on stale content.
    let skip_resolve = !self.content_dirty
      && !state.spec.previous
      && state.layer.as_ref().is_some_and(|l| l.width == width && l.height == height);
    if skip_resolve {
      self.pass_only_frames += 1;
    } else {
      if state.spec.previous {
        // Rotate the history before resolving: the current layer becomes
        // uPrevious and last frame's history buffer is resolved over. On the
        // first shaded frame the fresh history layer samples opaque black (its
        // creation clear).
        std::mem::swap(&mut state.layer, &mut state.prev_layer);
        if state.prev_layer.is_none() {
          let (tex, fbo) = crate::gpu::create_layer_target(&self.gl, width, height, [0.0, 0.0, 0.0, 1.0])?;
          state.prev_layer = Some(LayerTarget { tex, fbo, width, height });
        }
      } else if let Some(old) = state.prev_layer.take() {
        unsafe {
          glow::HasContext::delete_framebuffer(&self.gl, old.fbo);
          glow::HasContext::delete_texture(&self.gl, old.tex);
        }
      }

      let flipped = flip_for_fbo(dl, height)?;

      // (Re)allocate the layer at the window's pixel size. A resize drops and
      // recreates it: that is resize-frequency churn, not the per-frame kind
      // the rig exists to avoid.
      if state.layer.as_ref().is_none_or(|l| l.width != width || l.height != height) {
        if let Some(old) = state.layer.take() {
          unsafe {
            glow::HasContext::delete_framebuffer(&self.gl, old.fbo);
            glow::HasContext::delete_texture(&self.gl, old.tex);
          }
        }
        let (tex, fbo) = crate::gpu::create_layer_target(&self.gl, width, height, [0.0, 0.0, 0.0, 1.0])?;
        state.layer = Some(LayerTarget { tex, fbo, width, height });
      }
      let layer = state.layer.as_ref().expect("layer allocated above");

      gl::render_display_list_to_layer(
        &self.gl,
        &mut self.impeller_ctx,
        &mut self.offscreen_rig,
        &flipped,
        size,
        layer.fbo,
      )?;
      self.content_dirty = false;
    }
    let layer = state.layer.as_ref().expect("resolved or retained above");

    // The layer binds as uSource, the history layer (when declared and live)
    // as uPrevious - internal textures, no sampler object (their linear/clamp
    // object state stands; Impeller never draws them). Extra declared inputs
    // resolve through the registry by id with their declared sampling, a
    // missing id dropping to unbound (samples black), the same contract as
    // shader targets.
    let mut textures: Vec<PassInput> = vec![("uSource".to_string(), layer.tex, None)];
    if state.spec.previous {
      if let Some(prev) = &state.prev_layer {
        textures.push(("uPrevious".to_string(), prev.tex, None));
      }
    }
    for b in &state.spec.textures {
      match self.textures.get(&b.id) {
        Some(gpu) => {
          textures.push((b.name.clone(), gpu.gl_texture, Some(self.samplers.get(gpu.sampler.overridden(&b.sampler)))))
        }
        None => log::warn!("[alloy] window shader input '{}': texture {} not found", b.name, b.id),
      }
    }
    crate::gpu::render_program_to_window(
      &self.gl,
      &state.program,
      width,
      height,
      &state.spec.params,
      &textures,
      state.spec.vertex_count,
    );
    Ok(())
  }

  /// Apply a SetOverlay command: adopt the new declaration, keeping the
  /// rasterized layer's storage when the size is unchanged (the once-per-
  /// second figure refresh redraws into it), and mark it stale so the next
  /// frame re-rasterizes. None frees everything.
  fn set_overlay(&mut self, overlay: Option<crate::context::Overlay>) {
    let mut layer = self.overlay.take().and_then(|old| old.layer);
    // Free the retained layer on clear, and on a size change (reallocated at
    // the next frame's rasterize); a same-size redeclaration redraws into it.
    let free_layer = match &overlay {
      None => layer.is_some(),
      Some(decl) => layer.as_ref().is_some_and(|l| l.width != decl.width || l.height != decl.height),
    };
    if free_layer {
      let old = layer.take().expect("checked above");
      unsafe {
        glow::HasContext::delete_framebuffer(&self.gl, old.fbo);
        glow::HasContext::delete_texture(&self.gl, old.tex);
      }
    }
    if let Some(decl) = overlay {
      self.overlay = Some(OverlayState { decl, layer, stale: true });
    }
  }

  /// Composite the installed overlay over the finished frame in FBO 0.
  /// A stale declaration is rasterized into the retained layer first - drawn
  /// UNFLIPPED on purpose: the wrapped layer FBO is a bottom-up window
  /// target, so its rows come out in FBO 0's own convention and the plain
  /// copy draw (vUV = p, see the fullscreen vertex stage) lands the overlay
  /// upright with no flip anywhere. The composite is a premultiplied-alpha
  /// blended draw at the declared window rectangle (converted here to GL's
  /// bottom-up viewport origin). Failures warn and skip: a lost overlay
  /// frame must never cost the app frame.
  fn draw_overlay(&mut self, window: ISize) {
    let Some(ov) = &mut self.overlay else { return };
    let (width, height) = (ov.decl.width, ov.decl.height);
    if width == 0 || height == 0 {
      return;
    }
    if ov.stale || ov.layer.is_none() {
      if ov.layer.is_none() {
        match crate::gpu::create_layer_target(&self.gl, width, height, [0.0, 0.0, 0.0, 0.0]) {
          Ok((tex, fbo)) => ov.layer = Some(LayerTarget { tex, fbo, width, height }),
          Err(e) => {
            log::warn!("[alloy] overlay layer: {e}");
            return;
          }
        }
      }
      let layer = ov.layer.as_ref().expect("layer ensured above");
      let size = ISize::new(width as i64, height as i64);
      if let Err(e) = gl::render_display_list_to_layer(
        &self.gl,
        &mut self.impeller_ctx,
        &mut self.offscreen_rig,
        &ov.decl.dl,
        size,
        layer.fbo,
      ) {
        log::warn!("[alloy] overlay rasterize failed: {e}");
        return;
      }
      ov.stale = false;
    }
    let program = match ensure_copy_program(&self.gl, &mut self.copy_program) {
      Ok(program) => program,
      Err(e) => {
        log::warn!("[alloy] overlay copy program: {e}");
        return;
      }
    };
    let layer = ov.layer.as_ref().expect("rasterized above");
    let origin = (ov.decl.x, window.height as i32 - (ov.decl.y + height as i32));
    let input: PassInput = ("uSrc".to_string(), layer.tex, None);
    crate::gpu::composite_program_over_window(&self.gl, &program, origin, width, height, &[input]);
  }

  /// Apply a SetWindowShader command. A redeclaration with the same program
  /// keeps the retained layer and just adopts the new params/textures/vertex
  /// count (the per-frame params path); a different program releases the old
  /// state and starts fresh. None clears everything.
  fn set_window_shader(&mut self, shader: Option<WindowShader>) {
    let Some(spec) = shader else {
      self.clear_window_shader();
      return;
    };
    if let Some(state) = &mut self.window_shader {
      if state.spec.program == spec.program {
        state.spec = spec;
        return;
      }
    }
    let Some(program) = self.programs.get(&spec.program) else {
      // The UI side validated against its mirror; a miss here means the
      // mirrors diverged. Keep whatever was active rather than flashing the
      // unshaded frame.
      log::warn!("[alloy] window shader: program {} not found", spec.program);
      return;
    };
    let program = program.clone();
    self.clear_window_shader();
    self.window_shader = Some(WindowShaderState { spec, program, layer: None, prev_layer: None });
  }

  /// Free the window shader state: the layer's GL objects die here (they were
  /// never adopted or registered), the program only if nothing else holds it.
  fn clear_window_shader(&mut self) {
    if let Some(state) = self.window_shader.take() {
      for layer in [state.layer, state.prev_layer].into_iter().flatten() {
        unsafe {
          glow::HasContext::delete_framebuffer(&self.gl, layer.fbo);
          glow::HasContext::delete_texture(&self.gl, layer.tex);
        }
      }
      release_program(&self.gl, state.program);
    }
  }

  /// Clear the window backbuffer and swap it once, before any frame exists.
  /// Purely so the window becomes visible: on Wayland a surface is not mapped
  /// until its first buffer commit, so an app whose first render blocks (a
  /// synchronous device probe, say) puts nothing on screen at all - no title
  /// bar, nothing for the compositor to show, no way to close it but the pid.
  /// A black window that never fills in is a diagnosable failure; no window is
  /// not.
  ///
  /// Deliberately not a frame: no FrameOutput::Presented, no wake, no present
  /// fence. The main loop's bookkeeping (frame counter, FrameRendered to JS,
  /// vsync arming, pacing samples) must only ever see presents the UI thread
  /// actually built.
  pub(crate) fn prime_window(&self) {
    // Playback keeps the window hidden and never swaps.
    if self.capture_frames {
      return;
    }
    unsafe {
      glow::HasContext::bind_framebuffer(&self.gl, glow::FRAMEBUFFER, None);
      glow::HasContext::disable(&self.gl, glow::SCISSOR_TEST);
      glow::HasContext::clear_color(&self.gl, 0.0, 0.0, 0.0, 1.0);
      glow::HasContext::clear(&self.gl, glow::COLOR_BUFFER_BIT);
    }
    // Debug, not warn: the first real frame's present judges the surface for
    // real (failure counter, rebind-and-redraw recovery). This one is a
    // courtesy, and a platform that refuses it loses only the empty window.
    if !self.binding.swap() {
      log::debug!("[alloy] priming swap failed: {}", self.binding.error());
    }
  }

  /// Swap the window's backbuffer; true on success. Without the failure
  /// check a lost context / removed device leaves the app running normally
  /// while nothing reaches the screen (a frozen window with no message). A
  /// failed swap gets one rebind-and-redraw recovery attempt (see `frame`);
  /// a confirmed loss exits instead: see okf/backlog/gpu-context-loss.md.
  fn present(&mut self) -> bool {
    if self.binding.swap() {
      self.present_failures = 0;
      // At most one fence joins per frame (a retried present only follows a
      // failed one, which queued nothing), and `await_present_fence` trimmed
      // to depth-1 before the draw, so the queue never exceeds
      // PRESENT_FENCE_DEPTH. A failed fence_sync just means no pacing this
      // frame: same behavior as before this mechanism existed.
      if let Ok(fence) = unsafe { glow::HasContext::fence_sync(&self.gl, glow::SYNC_GPU_COMMANDS_COMPLETE, 0) } {
        self.present_fences.push_back(fence);
        // Flush the fence into the command stream now. ANGLE/D3D11 defers a
        // post-swap fence's submission (~2 swaps) and its glClientWaitSync
        // never blocks (the flush bit does not rescue it), so without this
        // the wait reads TIMEOUT_EXPIRED on every frame of a healthy GPU and
        // fenceTimeouts counts frames instead of stalls. Free elsewhere: the
        // swap already flushed everything but the fence itself. Measured in
        // alloy/examples/present_fence_probe.rs (phases A vs D).
        unsafe { glow::HasContext::flush(&self.gl) };
      }
      return true;
    }
    self.present_failures += 1;
    if self.present_failures == 1 {
      log::error!("[alloy] present failed: {}", self.binding.error());
    }
    if self.present_failures >= PRESENT_FAILURE_EXIT_THRESHOLD {
      log::error!("[alloy] GPU context lost ({} consecutive failed presents), exiting", self.present_failures);
      std::process::exit(1);
    }
    false
  }

  /// Rebind the context to the window's current EGL surface (see the
  /// RasterCmd doc); true on success. Must run on this thread: the context is
  /// current here and SDL_GL_MakeCurrent operates on the calling thread's
  /// binding. The swap interval is per-surface EGL state, so re-assert vsync.
  /// The failure counter is deliberately NOT touched here: the recovery path
  /// in `frame` judges the retry present on its own, and only the
  /// event-driven command resets stale evidence.
  fn rebind_window_surface(&mut self) -> bool {
    if !self.binding.bind() {
      log::warn!("[alloy] rebind window surface failed: {}", self.binding.error());
      return false;
    }
    if !self.capture_frames && !self.binding.set_swap_interval() {
      log::warn!("[alloy] set swap interval failed: {}", self.binding.error());
    }
    true
  }

  fn create_texture(
    &mut self,
    id: u64,
    width: u32,
    height: u32,
    pixels: &[u8],
    sampler: SamplerState,
    format: TextureFormat,
    label: Option<String>,
  ) -> Result<Texture, String> {
    let size = ISize::new(width as i64, height as i64);
    let mut gpu = GpuTexture::new(&self.gl, size, sampler, format);
    // A replace-at-id with no new label is an id-stable resize: labels are
    // create-time state and follow the id through it.
    gpu.label = label.or_else(|| self.textures.get(&id).and_then(|old| old.label.clone()));
    gpu.upload(&self.gl, pixels, size);
    match gl::adopt_texture(&gpu, &self.impeller_ctx, size) {
      Some(impeller) => {
        let replaced = self.textures.insert(id, gpu).is_some();
        // Replacing at an existing id (an id-stable resize): same contract as
        // UpdateTexture - shaders sampling this id re-render at the next
        // flush so they pick up the new texture without a params change.
        if replaced {
          self.dirty.insert(id);
        }
        Ok(impeller)
      }
      None => {
        // Adoption never took ownership, so the name is still ours to free.
        unsafe { glow::HasContext::delete_texture(&self.gl, gpu.gl_texture) };
        Err("adopt texture failed".to_string())
      }
    }
  }

  /// Resolve every pending target write: render each shader/pipeline target
  /// whose own state changed, or whose sampled content (transitively) did,
  /// in dependency order - sources before the targets sampling them - then
  /// clear the dirty set. Called at the points target pixels become
  /// observable (a drawn frame, an offscreen rasterization, a readback), so
  /// a chain of targets propagates end to end with each target rendered at
  /// most once per flush. Manual targets are excluded from the graph: the
  /// flush never renders one (only RenderTarget does) and never propagates
  /// through one - a dirty manual id acts as a plain content source, exactly
  /// like an uploaded texture. That exclusion is what keeps the purity
  /// invariant honest: everything ordered here is a pure function of its
  /// inputs, so rendering it zero, one, or many times is indistinguishable.
  fn flush_dirty(&mut self) {
    if self.dirty.is_empty() {
      return;
    }
    // A binding to a depth id is an edge to the target that renders it. A
    // sub-target is an edge from its parent: a dirty tile makes the parent
    // affected and orders it after the tile's own sources, and the parent
    // renders the one pass for all of them below.
    let own_sources: HashMap<u64, Vec<u64>> = self
      .shaders
      .iter()
      .filter(|(_, shader)| !shader.manual())
      .map(|(id, shader)| {
        let sources =
          shader.binding_sources().into_iter().map(|s| self.depth_owners.get(&s).copied().unwrap_or(s)).collect();
        (*id, sources)
      })
      .collect();
    let mut edges = own_sources.clone();
    for (parent, tiles) in &self.regions {
      if let Some(sources) = edges.get_mut(parent) {
        sources.extend(tiles.iter().copied());
      }
    }
    let (order, cyclic) = propagation_order(&self.dirty, &edges);
    if !cyclic.is_empty() {
      // The UI side rejects sampling cycles at bind time, so reaching this
      // means the mirrors diverged. Render each member once anyway: stale
      // inputs, but forward progress and no hang.
      let members: Vec<String> = cyclic.iter().map(|id| self.texture_desc(*id)).collect();
      log::warn!("[alloy] sampling cycle between shader targets [{}]; rendering each once", members.join(", "));
    }
    let affected: HashSet<u64> = order.iter().chain(cyclic.iter()).copied().collect();
    let changed = |id: &u64| self.dirty.contains(id) || affected.contains(id);
    let tile_clear = if self.regions.is_empty() {
      None
    } else {
      ensure_tile_clear_program(&self.gl, &mut self.tile_clear_program)
    };
    for id in order.iter().chain(cyclic.iter()) {
      let Some(shader) = self.shaders.get(id) else { continue };
      // A tile renders as a group of its parent's pass, never alone.
      if shader.region().is_some() {
        continue;
      }
      let start = std::time::Instant::now();
      self.pass_timer.begin(&self.gl);
      let resolve = |bindings: &[TextureBinding]| resolve_binding_list(&self.textures, &self.samplers, bindings);
      match self.regions.get(id) {
        None => shader.render(&self.gl, &resolve),
        Some(tiles) => {
          // Full when the parent's own state or inputs changed (its clear
          // wipes every tile, so every tile redraws), or when every tile
          // changed anyway (a pass-level clear is cheaper on a tiler than
          // the full-surface load a no-clear pass implies); otherwise only
          // the changed tiles, each over its own rectangle.
          let full = self.dirty.contains(id)
            || own_sources.get(id).is_some_and(|s| s.iter().any(changed))
            || tiles.iter().all(changed);
          let tiles: Vec<&ShaderTexture> =
            tiles.iter().filter(|t| full || changed(t)).filter_map(|t| self.shaders.get(t)).collect();
          shader.render_groups(&self.gl, &resolve, full, &tiles, tile_clear.as_deref());
        }
      }
      self.pass_timer.end(&self.gl, Timed::Pass { target: *id });
      let micros = start.elapsed().as_micros() as u64;
      shader.record_pass(micros);
      self.stats.passes.fetch_add(1, Ordering::Relaxed);
      self.stats.pass_issue_micros.fetch_add(micros, Ordering::Relaxed);
    }
    self.dirty.clear();
  }

  /// Resize an existing shader/pipeline target in place: a new target texture
  /// on the same FBO and program, re-rendered at the new size with the
  /// last-applied params (a manual target is cleared instead - the pass only
  /// runs on RenderTarget), then adopted into Impeller. Replies with the new
  /// handle so the UI side re-registers it under the same id; the old handle
  /// keeps the old GL name alive until in-flight display lists drop it.
  fn resize_shader_texture(&mut self, id: u64, width: u32, height: u32) -> Result<cmd::TargetHandles, String> {
    let shader = self.shaders.get_mut(&id).ok_or_else(|| format!("shader texture {id} not found"))?;
    shader.resize(&self.gl, width, height)?;
    let shader = self.shaders.get(&id).expect("shader present after resize");
    let size = ISize::new(width as i64, height as i64);
    let gpu = GpuTexture {
      gl_texture: shader.gl_texture(),
      width,
      height,
      sampler: shader.sampler(),
      format: TextureFormat::Rgba8,
      // The id-stable resize keeps the create's label, like create_texture's
      // replace-at-id path.
      label: self.textures.get(&id).and_then(|old| old.label.clone()),
    };
    // The depth texture got a fresh name too (see ShaderTexture::resize):
    // re-adopt and re-register it at its own id.
    let depth = match (shader.depth_texture(), self.target_depths.get(&id).copied()) {
      (Some(gl_texture), Some(depth_id)) => {
        let label = self.textures.get(&depth_id).and_then(|old| old.label.clone());
        let depth_gpu =
          GpuTexture { gl_texture, width, height, sampler: SamplerState::DEPTH, format: TextureFormat::Depth24, label };
        let impeller =
          gl::adopt_texture(&depth_gpu, &self.impeller_ctx, size).ok_or("adopt resized depth texture failed")?;
        self.textures.insert(depth_id, depth_gpu);
        Some(impeller)
      }
      _ => None,
    };
    match gl::adopt_texture(&gpu, &self.impeller_ctx, size) {
      Some(impeller) => {
        self.textures.insert(id, gpu);
        if let Some(shader) = self.shaders.get(&id) {
          if shader.manual() {
            // A resize cannot preserve accumulated history (new storage);
            // clear it so the app re-seeds from defined pixels. The flush
            // will not render it, so the clear must happen here.
            shader.clear(&self.gl);
          }
        }
        // The new storage renders (and its samplers re-resolve) at the next
        // flush, before anything observes it; for a manual target the dirty
        // seed only re-renders its samplers against the new (cleared) name.
        self.dirty.insert(id);
        Ok(cmd::TargetHandles { color: impeller, depth })
      }
      // Should-not-happen path (adoption of a valid GL name): the shader keeps
      // rendering into the new target, but the registry entry still shows the
      // old one. The new name stays referenced by the shader, so nothing is
      // freed here; the error surfaces to the caller.
      None => Err("adopt resized shader texture failed".to_string()),
    }
  }

  fn update_texture(&mut self, id: u64, pixels: &[u8]) -> Result<(), String> {
    let gpu = self.textures.get(&id).ok_or_else(|| format!("texture {id} not found"))?;
    let expected = (gpu.width as usize) * (gpu.height as usize) * gpu.format.bytes_per_pixel();
    if pixels.len() != expected {
      return Err(format!(
        "texture {} update is {} bytes, expected {expected} ({})",
        describe(id, &gpu.label),
        pixels.len(),
        gpu.format.name()
      ));
    }
    let size = ISize::new(gpu.width as i64, gpu.height as i64);
    gpu.upload(&self.gl, pixels, size);
    // Shader targets sampling this texture re-render at the next flush, so
    // data-texture changes are visible without a params change.
    self.dirty.insert(id);
    Ok(())
  }

  #[allow(clippy::too_many_arguments)]
  fn create_shader_texture(
    &mut self,
    id: u64,
    width: u32,
    height: u32,
    fragment_src: &str,
    params: &[(String, ParamValue)],
    textures: Vec<TextureBinding>,
    sampler: SamplerState,
    label: Option<String>,
  ) -> Result<(Texture, UniformTable), String> {
    let mut shader = ShaderTexture::new(&self.gl, width, height, fragment_src, textures)?.with_sampler(sampler);
    let uniforms = shader.uniform_table();
    // Uniform names only exist after the compile, so create-time params and
    // bindings validate here, inside the blocking RPC - the error still
    // surfaces at the JS call site, and the half-built target rolls back.
    if let Err(e) =
      validate_params(&uniforms, params).and_then(|()| validate_texture_bindings(&uniforms, shader.sampler_bindings()))
    {
      shader.destroy(&self.gl);
      return Err(e);
    }
    shader.merge_params(params);
    let texture = self.register_shader_target(id, shader, width, height, label, "adopt shader texture failed")?;
    Ok((texture, uniforms))
  }

  fn create_pipeline_texture(&mut self, id: u64, spec: PipelineSpec) -> Result<(Texture, UniformTable), String> {
    let label = spec.target.label.clone();
    let buffers = resolve_entry_buffers(&self.buffers, spec.entry.buffer_ids())?;
    let shader = ShaderTexture::new_pipeline(
      &self.gl,
      spec.target.width,
      spec.target.height,
      &spec.vertex_src,
      &spec.fragment_src,
      spec.entry.textures.clone(),
      spec.pipeline,
      buffers,
      spec.entry.draw,
      spec.target.clear_color,
      spec.target.samples,
    )?;
    let mut shader =
      shader.with_sampler(spec.target.sampler).with_manual(spec.target.manual).with_load(spec.target.load);
    let uniforms = shader.uniform_table();
    // Same post-compile validation and rollback as create_shader_texture.
    if let Err(e) = validate_params(&uniforms, &spec.entry.params)
      .and_then(|()| validate_texture_bindings(&uniforms, shader.sampler_bindings()))
    {
      shader.destroy(&self.gl);
      return Err(e);
    }
    shader.merge_params(&spec.entry.params);
    let texture = self.register_shader_target(
      id,
      shader,
      spec.target.width,
      spec.target.height,
      label,
      "adopt pipeline texture failed",
    )?;
    Ok((texture, uniforms))
  }

  /// Link two compiled stages from the stage registry into a registered
  /// program, replying with the reflected uniform and attribute tables for
  /// the UI-side validation mirror. The UI side validated the ids and stage kinds against
  /// its mirror; a miss here means the mirrors diverged.
  fn link_program(
    &mut self,
    id: u64,
    vertex: u64,
    fragment: u64,
    label: Option<String>,
  ) -> Result<(UniformTable, AttributeTable), String> {
    let vs = self.stages.get(&vertex).ok_or_else(|| format!("shader {vertex} not found"))?;
    let fs = self.stages.get(&fragment).ok_or_else(|| format!("shader {fragment} not found"))?;
    let program = ShaderProgram::from_stages(&self.gl, vs, fs)?.with_label(label);
    let tables = (program.uniform_table(), program.attribute_table());
    self.programs.insert(id, Rc::new(program));
    Ok(tables)
  }

  /// Pair a registered program with draw state under pipeline id `id`.
  fn create_render_pipeline(
    &mut self,
    id: u64,
    program_id: u64,
    desc: PipelineDesc,
    label: Option<String>,
  ) -> Result<(), String> {
    let program = self.programs.get(&program_id).ok_or_else(|| format!("program {program_id} not found"))?.clone();
    let pipeline = RenderPipeline::new(program, Some(program_id), desc).map_err(|(_, e)| e)?.with_label(label);
    self.render_pipelines.insert(id, Rc::new(pipeline));
    Ok(())
  }

  /// Create a fixed single-entry target over a registered pipeline
  /// (`entry.pipeline`) and adopt it under texture id `id`; the first render
  /// happens at the next dirty flush.
  fn create_shader_target(&mut self, id: u64, spec: TargetSpec, entry: DrawSpec) -> Result<Texture, String> {
    let pipeline_id = entry.pipeline;
    let pipeline =
      self.render_pipelines.get(&pipeline_id).ok_or_else(|| format!("pipeline {pipeline_id} not found"))?.clone();
    // The program already exists, so params and bindings validate before
    // anything is built - no rollback needed on this path.
    let uniforms = pipeline.uniform_table();
    validate_params(&uniforms, &entry.params)?;
    validate_texture_bindings(&uniforms, &entry.textures)?;
    let buffers = resolve_entry_buffers(&self.buffers, entry.buffer_ids())?;
    let mut shader = ShaderTexture::from_pipeline(
      &self.gl,
      pipeline,
      Some(pipeline_id),
      spec.width,
      spec.height,
      entry.textures.clone(),
      buffers,
      entry.draw,
      spec.clear_color,
      spec.samples,
    )
    .map_err(|(_, e)| e)?
    .with_sampler(spec.sampler)
    .with_manual(spec.manual)
    .with_load(spec.load);
    shader.merge_params(&entry.params);
    self.register_shader_target(id, shader, spec.width, spec.height, spec.label, "adopt shader target failed")
  }

  /// Create a draw target - empty ordered draw list over color plus optional
  /// target-owned depth storage - and adopt it under texture id `id`. A
  /// flush-rendered draw target starts dirty (its first render is the clear);
  /// a manual one is cleared at registration like every manual target.
  ///
  /// With `DepthStorage::Texture` the depth texture is adopted and
  /// registered under `depth_id` beside the color: the same ownership as
  /// the color (Impeller deletes the name when the handle drops), and the
  /// same fixed sampling its id declares (`SamplerState::DEPTH`).
  fn create_draw_target(
    &mut self,
    id: u64,
    depth_id: Option<u64>,
    spec: TargetSpec,
    depth: DepthStorage,
  ) -> Result<cmd::TargetHandles, String> {
    let (width, height) = (spec.width, spec.height);
    let shader = ShaderTexture::new_draw_target(&self.gl, width, height, depth, spec.clear_color, spec.samples)?
      .with_sampler(spec.sampler)
      .with_manual(spec.manual)
      .with_load(spec.load);
    let depth_texture = shader.depth_texture();
    let depth_label = spec.label.as_ref().map(|l| format!("{l}.depth"));
    let color = match self.register_shader_target(id, shader, width, height, spec.label, "adopt draw target failed") {
      Ok(color) => color,
      Err(e) => {
        // register deleted the color name; the depth texture is ours until
        // adopted.
        if let Some(tex) = depth_texture {
          unsafe { glow::HasContext::delete_texture(&self.gl, tex) };
        }
        return Err(e);
      }
    };
    let depth = match (depth_texture, depth_id) {
      (Some(gl_texture), Some(depth_id)) => {
        let gpu = GpuTexture {
          gl_texture,
          width,
          height,
          sampler: SamplerState::DEPTH,
          format: TextureFormat::Depth24,
          label: depth_label,
        };
        match gl::adopt_texture(&gpu, &self.impeller_ctx, ISize::new(width as i64, height as i64)) {
          Some(impeller) => {
            self.textures.insert(depth_id, gpu);
            self.target_depths.insert(id, depth_id);
            self.depth_owners.insert(depth_id, id);
            Some(impeller)
          }
          None => {
            // Roll the color registration back: dropping `color` lets
            // Impeller delete that name; the depth name is still ours.
            self.textures.remove(&id);
            self.dirty.remove(&id);
            if let Some(shader) = self.shaders.remove(&id) {
              shader.destroy(&self.gl);
            }
            unsafe { glow::HasContext::delete_texture(&self.gl, gl_texture) };
            return Err("adopt depth texture failed (depth \"texture\" is unavailable on this device)".to_string());
          }
        }
      }
      _ => None,
    };
    Ok(cmd::TargetHandles { color, depth })
  }

  /// Create a sub-target of draw target `parent` under `id` (see
  /// `RasterCmd::CreateSubTarget`): in the shader map and the parent's
  /// group list, never in the texture map. Starts dirty like every
  /// flush-rendered target; the parent renders it at the next flush.
  fn create_sub_target(&mut self, id: u64, parent: u64, x: i32, y: i32, spec: TargetSpec) -> Result<(), String> {
    let parent_shader = self.shaders.get(&parent).ok_or_else(|| format!("target {parent} not found"))?;
    let shader =
      ShaderTexture::new_sub_target(parent_shader, parent, x, y, spec.width, spec.height, spec.clear_color, spec.label)?;
    self.shaders.insert(id, shader);
    self.regions.entry(parent).or_default().push(id);
    self.dirty.insert(id);
    Ok(())
  }

  /// Add a draw entry to a draw target (see `RasterCmd::AddDraw`). The UI
  /// side validated everything against its mirrors; a failure here means the
  /// mirrors diverged.
  fn add_draw(&mut self, target: u64, draw: u64, entry: DrawSpec, before: Option<u64>) -> Result<(), String> {
    let pipeline = self
      .render_pipelines
      .get(&entry.pipeline)
      .ok_or_else(|| format!("pipeline {} not found", entry.pipeline))?
      .clone();
    let buffers = resolve_entry_buffers(&self.buffers, entry.buffer_ids())?;
    let shader = self.shaders.get_mut(&target).ok_or_else(|| format!("shader texture {target} not found"))?;
    shader.add_entry(
      &self.gl,
      draw,
      pipeline,
      Some(entry.pipeline),
      buffers,
      entry.draw,
      entry.params,
      entry.textures,
      before,
    )?;
    if !shader.manual() {
      self.dirty.insert(target);
    }
    Ok(())
  }

  /// Apply a per-entry write to a draw target and mark it dirty (a manual
  /// target only folds - its pixels change on its next explicit render),
  /// warning on failure like every fire-and-forget write.
  fn entry_write(
    &mut self,
    target: u64,
    what: &str,
    write: impl FnOnce(&glow::Context, &mut ShaderTexture) -> Result<(), String>,
  ) {
    let result = self
      .shaders
      .get_mut(&target)
      .ok_or_else(|| format!("shader texture {target} not found"))
      .and_then(|shader| write(&self.gl, shader).map(|()| shader.manual()));
    match result {
      Ok(manual) => {
        if !manual {
          self.dirty.insert(target);
        }
      }
      Err(e) => log::warn!("[alloy] {what} failed: {e}"),
    }
  }

  /// Adopt a new shader/pipeline target into Impeller and record it under
  /// `id` in both the texture and shader maps. The target starts dirty: its
  /// first render happens at the next flush, before anything observes its
  /// pixels, so the blocking create RPC never pays for a draw. A manual
  /// target is cleared instead: its pass runs only on RenderTarget, and the
  /// clear is what keeps undefined storage from ever being observable.
  fn register_shader_target(
    &mut self,
    id: u64,
    shader: ShaderTexture,
    width: u32,
    height: u32,
    label: Option<String>,
    adopt_err: &str,
  ) -> Result<Texture, String> {
    let size = ISize::new(width as i64, height as i64);
    let gpu = GpuTexture {
      gl_texture: shader.gl_texture(),
      width,
      height,
      sampler: shader.sampler(),
      format: TextureFormat::Rgba8,
      label,
    };
    match gl::adopt_texture(&gpu, &self.impeller_ctx, size) {
      Some(impeller) => {
        self.textures.insert(id, gpu);
        if shader.manual() {
          shader.clear(&self.gl);
        } else {
          self.dirty.insert(id);
        }
        self.shaders.insert(id, shader);
        Ok(impeller)
      }
      None => {
        shader.destroy(&self.gl);
        unsafe { glow::HasContext::delete_texture(&self.gl, gpu.gl_texture) };
        Err(adopt_err.to_string())
      }
    }
  }

  /// Drain retired timer queries into the cumulative GPU-side counters
  /// (see gpu::PassTimer). A target destroyed before its result retired is
  /// simply not credited; the total still is.
  fn harvest_pass_timings(&mut self) {
    for exec in self.pass_timer.poll(&self.gl) {
      match exec.what {
        Timed::Pass { target } => {
          self.stats.pass_exec_micros.fetch_add(exec.micros, Ordering::Relaxed);
          if let Some(shader) = self.shaders.get(&target) {
            shader.record_exec(exec.micros);
          }
        }
        Timed::Frame => {
          self.stats.frame_exec_micros.fetch_add(exec.micros, Ordering::Relaxed);
        }
      }
    }
  }

  /// Overwrite manual target `dst` with texture `src`'s current pixels via
  /// the shared copy program - a fullscreen sampling draw into dst's FBO,
  /// never a blit. The UI side validated ids, sizes, and dst's manual mode;
  /// a miss here means the mirrors diverged. Counts as a pass into dst (it
  /// occupies the thread like one) and seeds dst into the dirty set so
  /// targets sampling it re-render at the next flush.
  fn copy_texture(&mut self, src: u64, dst: u64) -> Result<(), String> {
    let program = ensure_copy_program(&self.gl, &mut self.copy_program)?;
    let gpu = self.textures.get(&src).ok_or_else(|| format!("texture {src} not found"))?;
    let shader = self.shaders.get(&dst).ok_or_else(|| format!("shader texture {dst} not found"))?;
    let input: PassInput = ("uSrc".to_string(), gpu.gl_texture, Some(self.samplers.get(gpu.sampler)));
    let start = std::time::Instant::now();
    self.pass_timer.begin(&self.gl);
    shader.overwrite_with(&self.gl, &program, &[input]);
    self.pass_timer.end(&self.gl, Timed::Pass { target: dst });
    let micros = start.elapsed().as_micros() as u64;
    shader.record_pass(micros);
    self.stats.passes.fetch_add(1, Ordering::Relaxed);
    self.stats.pass_issue_micros.fetch_add(micros, Ordering::Relaxed);
    self.dirty.insert(dst);
    Ok(())
  }

  fn write_buffer(&mut self, id: u64, data: &[u8], byte_offset: usize) -> Result<(), String> {
    let buffer = self.buffers.get(&id).ok_or_else(|| format!("buffer {id} not found"))?;
    buffer.write(&self.gl, data, byte_offset).map_err(|e| format!("buffer {}: {e}", describe(id, &buffer.label)))?;
    // Every pipeline drawing from this buffer re-renders at the next flush,
    // so geometry-only changes reach the screen even when no new params
    // arrive. (Marked by target id: buffer ids are their own space.) Manual
    // targets pick the new geometry up at their next explicit render.
    let drawing: Vec<u64> =
      self.shaders.iter().filter(|(_, s)| !s.manual() && s.reads_buffer(id)).map(|(tid, _)| *tid).collect();
    self.dirty.extend(drawing);
    Ok(())
  }

  /// `7 (bloom-h)` when texture id 7 carries a label, else `7`: how raster
  /// messages name a texture - the id stays the cross-reference key, the
  /// label the human name.
  fn texture_desc(&self, id: u64) -> String {
    describe(id, &self.textures.get(&id).and_then(|t| t.label.clone()))
  }

  /// Inventory the GPU resources this thread tracks: registered textures,
  /// vertex buffers, and shader/pipeline targets with their bookkeeping.
  /// Sorted by id for stable output.
  fn resources(&self) -> GpuResources {
    let mut textures: Vec<GpuTextureInfo> = self
      .textures
      .iter()
      .map(|(id, gpu)| GpuTextureInfo {
        id: *id,
        width: gpu.width,
        height: gpu.height,
        target: self.shaders.contains_key(id),
        format: gpu.format.name(),
        sampler: gpu.sampler,
        label: gpu.label.clone(),
      })
      .collect();
    textures.sort_by_key(|t| t.id);

    let mut buffers: Vec<GpuBufferInfo> = self
      .buffers
      .iter()
      .map(|(id, b)| GpuBufferInfo { id: *id, byte_length: b.size, label: b.label.clone() })
      .collect();
    buffers.sort_by_key(|b| b.id);

    let mut pipelines: Vec<GpuPipelineInfo> = self
      .shaders
      .iter()
      .map(|(texture_id, shader)| {
        let (passes, pass_issue_micros, pass_exec_micros) = shader.pass_stats();
        // A draw target reports its entries in the `draws` list; the flat
        // single-pass fields stay for the fixed kinds, where they describe
        // the one pass.
        let flat = !shader.is_draw_list();
        let draw = if flat { shader.draw_range() } else { None };
        let (width, height) = shader.size();
        let region = shader.region().map(|r| GpuRegionInfo { parent: r.parent, x: r.x, y: r.y, width, height });
        GpuPipelineInfo {
          texture_id: *texture_id,
          label: self
            .textures
            .get(texture_id)
            .and_then(|t| t.label.clone())
            .or_else(|| shader.region().and_then(|r| r.label.clone())),
          region,
          kind: if shader.is_draw_list() {
            "draws"
          } else if shader.is_pipeline() {
            "pipeline"
          } else {
            "fragment"
          },
          program_id: if flat { shader.program_id() } else { None },
          pipeline_id: if flat { shader.pipeline_id() } else { None },
          buffer_id: if flat { shader.buffer_id() } else { None },
          index_buffer_id: if flat { shader.index_buffer_id() } else { None },
          index_format: if flat { shader.index_format_name() } else { None },
          instance_buffer_ids: if flat { shader.instance_buffer_ids() } else { Vec::new() },
          topology: if flat { shader.topology_name() } else { None },
          draw_count: draw.map(|d| d.vertex_count),
          first_vertex: draw.map(|d| d.first_vertex),
          instance_count: draw.map(|d| d.instance_count),
          depth: shader.has_depth(),
          samples: shader.samples(),
          depth_write: if flat { shader.depth_write() } else { None },
          blend: if flat { shader.blend_name() } else { None },
          cull: if flat { shader.cull_name() } else { None },
          attributes: if flat {
            shader.attributes().iter().map(|(name, fmt)| (name.clone(), fmt.name().to_string())).collect()
          } else {
            Vec::new()
          },
          instance_attributes: if flat {
            shader
              .instance_attributes()
              .iter()
              .map(|(name, fmt, slot)| (name.clone(), fmt.name().to_string(), *slot))
              .collect()
          } else {
            Vec::new()
          },
          // For a draw target the flat textures/params fields carry its
          // shared (target-level) bindings and params; the per-entry ones
          // live in `draws`.
          textures: if flat { shader.sampler_bindings().to_vec() } else { shader.shared_bindings().to_vec() },
          params: if flat { shader.last_params() } else { shader.shared_params().to_vec() },
          draws: if flat { Vec::new() } else { shader.draw_infos() },
          manual: shader.manual(),
          load: shader.load(),
          passes,
          pass_issue_micros,
          pass_exec_micros,
        }
      })
      .collect();
    pipelines.sort_by_key(|p| p.texture_id);

    let mut render_pipelines: Vec<GpuRenderPipelineInfo> = self
      .render_pipelines
      .iter()
      .map(|(id, pipeline)| {
        let desc = pipeline.desc();
        GpuRenderPipelineInfo {
          id: *id,
          program_id: pipeline.program_id().unwrap_or(0),
          label: pipeline.label().map(str::to_string),
          topology: desc.topology.name(),
          blend: crate::gpu::blend_name(desc.blend),
          cull: crate::gpu::cull_name(desc.cull),
          depth: desc.depth.is_some(),
          depth_write: desc.depth.map_or(true, |d| d.write),
          attributes: desc.attributes.iter().map(|(name, fmt)| (name.clone(), fmt.name().to_string())).collect(),
          instance_attributes: desc
            .instance_attributes
            .iter()
            .map(|(name, fmt, slot)| (name.clone(), fmt.name().to_string(), *slot))
            .collect(),
        }
      })
      .collect();
    render_pipelines.sort_by_key(|p| p.id);

    let mut programs: Vec<GpuProgramInfo> =
      self.programs.iter().map(|(id, p)| GpuProgramInfo { id: *id, label: p.label().map(str::to_string) }).collect();
    programs.sort_by_key(|p| p.id);

    let window_shader = self.window_shader.as_ref().map(|state| GpuWindowShaderInfo {
      program_id: state.spec.program,
      width: state.layer.as_ref().map_or(0, |l| l.width),
      height: state.layer.as_ref().map_or(0, |l| l.height),
      previous: state.spec.previous && state.prev_layer.is_some(),
      pass_only_frames: self.pass_only_frames,
    });

    GpuResources { textures, buffers, pipelines, render_pipelines, programs, window_shader }
  }
}

/// `7 (bloom-h)` with a label, `7` without: the one spelling for a labeled id
/// in raster-side messages.
fn describe(id: u64, label: &Option<String>) -> String {
  match label {
    Some(label) => format!("{id} ({label})"),
    None => id.to_string(),
  }
}

/// Resolve an entry's buffer ids (vertex, index, instance) against the
/// buffer registry: the Rc clones the entry keeps for its VAO's lifetime.
/// The draw range itself arrives already resolved and bounds-checked from
/// the UI thread (`resolve_draw_range`), which owns the stride/size mirrors;
/// a miss here means those mirrors diverged.
fn resolve_entry_buffers(buffers: &HashMap<u64, Rc<GpuBuffer>>, ids: BufferIds) -> Result<EntryBuffers, String> {
  let lookup = |id: u64, role: &str| -> Result<Rc<GpuBuffer>, String> {
    buffers.get(&id).cloned().ok_or_else(|| format!("{role} {id} not found"))
  };
  let vertex = match ids.buffer {
    0 => None,
    id => Some((lookup(id, "buffer")?, id)),
  };
  let index = match ids.index {
    Some((id, format)) => Some((lookup(id, "index buffer")?, id, format)),
    None => None,
  };
  let mut instances = Vec::new();
  for &id in ids.instance_buffers.iter().take_while(|&&id| id != 0) {
    instances.push((lookup(id, "instance buffer")?, id));
  }
  Ok(EntryBuffers { vertex, index, instances })
}

/// Which shader targets need re-rendering after the contents of the `dirty`
/// ids changed, given the sampler graph `edges` (target id -> the ids it
/// samples, with multiplicity): every target that is itself dirty or samples
/// a dirty/affected id, in dependency order - sources before the targets
/// sampling them - so one pass over the result renders a chain end to end.
/// Targets on a sampling cycle cannot be ordered and come back in the second
/// list. Both lists are deterministic (ascending id per Kahn layer) for a
/// given input. Pure over the id graph, so it unit-tests without GL.
pub(crate) fn propagation_order(dirty: &HashSet<u64>, edges: &HashMap<u64, Vec<u64>>) -> (Vec<u64>, Vec<u64>) {
  use std::collections::BTreeSet;
  // Affected = fixpoint of "dirty target, or samples a dirty/affected id".
  let mut affected: BTreeSet<u64> = BTreeSet::new();
  loop {
    let before = affected.len();
    for (id, sources) in edges {
      if !affected.contains(id)
        && (dirty.contains(id) || sources.iter().any(|s| dirty.contains(s) || affected.contains(s)))
      {
        affected.insert(*id);
      }
    }
    if affected.len() == before {
      break;
    }
  }
  // Kahn's algorithm over the affected subgraph: a target is ready once none
  // of its sources are still waiting (sources outside `remaining` are either
  // unaffected or already ordered).
  let mut order = Vec::with_capacity(affected.len());
  let mut remaining = affected;
  loop {
    let ready: Vec<u64> =
      remaining.iter().copied().filter(|id| edges[id].iter().all(|s| !remaining.contains(s))).collect();
    if ready.is_empty() {
      break;
    }
    for id in ready {
      remaining.remove(&id);
      order.push(id);
    }
  }
  (order, remaining.into_iter().collect())
}

/// Map a binding list to live GL textures, each with the sampler object for
/// the source's declared state under the binding's override, dropping any
/// id no longer registered (it samples as unbound/black). The resolver a
/// target's render calls per pass - once for a fragment target, once per
/// entry for a mesh target.
fn resolve_binding_list(
  textures: &HashMap<u64, GpuTexture>,
  samplers: &SamplerCache,
  bindings: &[TextureBinding],
) -> Vec<PassInput> {
  bindings
    .iter()
    .filter_map(|b| {
      textures
        .get(&b.id)
        .map(|gpu| (b.name.clone(), gpu.gl_texture, Some(samplers.get(gpu.sampler.overridden(&b.sampler)))))
    })
    .collect()
}
