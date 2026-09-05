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

mod buffer_age;
mod cmd;
mod compose;
mod frame;
mod offscreen;
mod repaint;
mod resources;
mod targets;

pub(crate) use cmd::RasterCmd;
#[cfg(test)]
pub(crate) use targets::propagation_order;
pub(crate) use repaint::WindowRoute;
pub use cmd::{DamageRect, PresentDamage};
use repaint::DamageTracker;

use impellers::{Context as ImpellerContext, ISize};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc};

use crate::backend::{FrameOutput, GlBinding};
use crate::gl;
use crate::gl::{
  release_buffer, release_pipeline, release_program, EntryBuffers, GpuBuffer, GpuTexture, PassTimer, RenderPipeline,
  SamplerCache, ShaderProgram, ShaderTexture,
};
use crate::gpu::{BufferIds, GpuLimits, WindowShader};
use crate::gpu::{SamplerState, TextureFormat, TextureShape};

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
  /// Presents the screen missed while a next frame was demanded: over each
  /// contiguous run of demanded presents, the whole periods elapsed minus the
  /// presents delivered (see `record_present_interval`). The direct jank
  /// count - a repeated frame lands here even when every per-second average
  /// reads clean.
  pub(crate) missed_presents: AtomicU64,
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
  /// Presents that drew only a damage patch over an aged back buffer
  /// (partial repaint, okf/done/partial-repaint.md); the verification
  /// signal that stage 2 is engaging.
  pub(crate) partial_presents: AtomicU64,
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
  /// Presents missed while a next frame was demanded - the direct jank
  /// count (see `RasterStats::missed_presents`).
  pub missed_presents: u64,
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
  /// Presents that drew only a damage patch over an aged back buffer
  /// (partial repaint); stays 0 where buffer age is unavailable.
  pub partial_presents: u64,
}

impl RasterStats {
  pub(crate) fn sample(&self) -> RasterCounters {
    RasterCounters {
      queue: self.queue_depth.load(Ordering::Acquire),
      idle_ticks: self.idle_ticks.load(Ordering::Relaxed),
      fence_timeouts: self.fence_timeouts.load(Ordering::Relaxed),
      missed_presents: self.missed_presents.load(Ordering::Relaxed),
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
      partial_presents: self.partial_presents.load(Ordering::Relaxed),
    }
  }

  pub(crate) fn new() -> Self {
    RasterStats {
      queue_depth: AtomicUsize::new(0),
      idle_ticks: AtomicU64::new(0),
      fence_timeouts: AtomicU64::new(0),
      missed_presents: AtomicU64::new(0),
      passes: AtomicU64::new(0),
      pass_issue_micros: AtomicU64::new(0),
      pass_exec_micros: AtomicU64::new(0),
      frame_exec_micros: AtomicU64::new(0),
      timer_queries: AtomicBool::new(false),
      cmd_micros: AtomicU64::new(0),
      partial_presents: AtomicU64::new(0),
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

  /// Block until every command queued before this call has executed. The
  /// thread answers in order, so the reply means its queue was empty and
  /// nothing is drawing: what a process exit or a window teardown needs,
  /// since either pulls the driver out from under a draw still encoding.
  /// A raster thread that is already gone counts as drained.
  pub(crate) fn drain(&self) {
    let (reply_tx, reply_rx) = mpsc::channel();
    if self.send(RasterCmd::Drain { reply: reply_tx }).is_ok() {
      reply_rx.recv().ok();
    }
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

// Slack, in refresh periods, subtracted from a demanded run's elapsed span
// before rounding it to the presents the display expected. Swap-return times
// jitter by more than half a period under mailbox/triple-buffered
// compositors (the reason lattice's animation clock paces by present count,
// not timestamps), so judging intervals with a plain round() would latch
// phantom misses on healthy runs; a real missed present overshoots by a full
// period and still counts through this slack.
const JANK_JITTER_SLACK: f64 = 0.25;

// Refresh rate assumed for miss accounting until the event loop has queried
// the display mode (same fallback the frame loop uses).
const FALLBACK_REFRESH_HZ: f32 = 60.0;

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
  // The UI-side frame-request latch, sampled (never consumed) at present
  // time to tell a demanded gap from an idle one; None until the embedder
  // registers it (SetDemandLatch), and miss accounting stays off without it.
  demand_latch: Option<Arc<AtomicBool>>,
  // The contiguous run of demanded presents miss accounting is currently
  // spanning; None while the app is idle (see record_present_interval).
  present_run: Option<PresentRun>,
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
  stages: HashMap<u64, crate::gl::CompiledStage>,
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
  // Partial repaint (okf/done/partial-repaint.md stage 2): pending damage,
  // the presented-frame damage ring, and the buffer-age query, owned as one
  // protocol (see repaint.rs).
  damage: DamageTracker,
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

/// One contiguous run of demanded presents, the unit miss accounting works
/// over (see `record_present_interval`): misses are counted as the whole
/// refresh periods the run has spanned minus the presents delivered, judged
/// over the accumulated span rather than per interval so per-swap timestamp
/// jitter cancels instead of latching phantom misses.
struct PresentRun {
  /// Instant of the present that opened the run (demand was latched when it
  /// left the swap).
  start: std::time::Instant,
  /// Refresh rate the run is judged against; a mid-run mode change mixes
  /// periods, so the run restarts instead.
  hz: f32,
  /// Presents delivered since `start` (each closes one interval).
  intervals: u64,
  /// Misses already added to `RasterStats::missed_presents` for this run;
  /// only growth beyond this high-water mark is added, so a jittery reading
  /// can never count the same miss twice.
  reported: u64,
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
    let limits = crate::gl::query_limits(&gl);
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
      demand_latch: None,
      present_run: None,
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
      damage: DamageTracker::new(),
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
        if cmd.invalidates_resolved_content() {
          self.content_dirty = true;
        }
        // Frames are excluded from cmd_micros (see RasterStats); everything
        // else the loop executes is otherwise invisible to timing.
        let timed = !matches!(cmd, RasterCmd::Frame { .. });
        let cmd_start = std::time::Instant::now();
        self.harvest_pass_timings();
        match cmd {
          RasterCmd::Frame { dl, tree_clean, damage } => {
            // A load-shed frame that was not clean still changed the tree:
            // the frame that draws in its place must not skip the resolve.
            if !tree_clean {
              self.content_dirty = true;
            }
            // A shed frame's damage folds into the frame that draws in its
            // place, so its changes still reach the screen.
            self.damage.fold(damage);
            if (self.capture_frames || Some(i) == last_frame) && self.frame(dl).is_err() {
              break 'outer; // main loop is gone
            }
          }
          RasterCmd::SetDemandLatch { latch } => {
            self.demand_latch = Some(latch);
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
          RasterCmd::CreateCubeTexture { id, size, faces, sampler, format, label, reply: tx } => {
            reply(tx, self.create_cube_texture(id, size, &faces, sampler, format, label));
          }
          RasterCmd::UpdateTexture { id, pixels } => {
            if let Err(e) = self.update_texture(id, &pixels) {
              log::warn!("[alloy] texture update failed: {e}");
            }
          }
          RasterCmd::UpdateYuv { planes, frame } => {
            for (id, offset) in planes {
              let len = match self.textures.get(&id) {
                Some(gpu) => gpu.format.byte_len(gpu.width, gpu.height),
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
            let result = crate::gl::compile_stage(&self.gl, stage, &source, header).map(|shader| {
              self.stages.insert(id, shader);
            });
            reply(tx, result);
          }
          RasterCmd::LinkProgram { id, vertex, fragment, label, reply: tx } => {
            reply(tx, self.link_program(id, vertex, fragment, label));
          }
          RasterCmd::DestroyStage { id } => {
            if let Some(stage) = self.stages.remove(&id) {
              crate::gl::delete_stage(&self.gl, stage.shader);
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
          RasterCmd::CreateCubeDrawTarget { id, size, spec, depth, reply: tx } => {
            reply(tx, self.create_cube_draw_target(id, size, spec, depth));
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
            self.entry_write(id, "shader params update", |_, shader| {
              shader.merge_params(&params);
              Ok(())
            });
          }
          RasterCmd::UpdateShaderTextures { id, textures } => {
            self.entry_write(id, "shader texture rebind", |_, shader| shader.set_sampler_bindings(&textures));
          }
          RasterCmd::ResizeShaderTexture { id, width, height, reply: tx } => {
            reply(tx, self.resize_shader_texture(id, width, height));
          }
          RasterCmd::SetDraw { id, range } => {
            self.entry_write(id, "draw update", |_, shader| shader.set_draw(range));
          }
          RasterCmd::RenderTarget { id, face } => {
            self.render_target_now(id, face);
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
          RasterCmd::AdoptTexture { id, texture, width, height } => match offscreen::gl_name(&texture) {
            Ok(gl_texture) => {
              let label = Some("snapshot".to_string());
              self.textures.insert(
                id,
                GpuTexture {
                  gl_texture,
                  width,
                  height,
                  shape: TextureShape::D2,
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
            self.release_cube(id);
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
          RasterCmd::Drain { reply: tx } => {
            reply(tx, ());
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
