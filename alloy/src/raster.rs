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

use impellers::{Context as ImpellerContext, DisplayList, ISize, Texture};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc};

use crate::backend::{Backend, FrameOutput};
use crate::context::{
  GpuBufferInfo, GpuPipelineInfo, GpuProgramInfo, GpuResources, GpuTextureInfo, GpuWindowShaderInfo, WindowShader,
};
use crate::gl;
use crate::shader::{release_program, AttrFormat, GpuBuffer, ShaderProgram, ShaderStage, ShaderTexture};
use crate::texture::GpuTexture;

/// Owned form of `context::PipelineSpec` (whose fields borrow from JS values),
/// so the spec can cross the channel.
pub(crate) struct PipelineSpecOwned {
  pub width: u32,
  pub height: u32,
  pub vertex_src: String,
  pub fragment_src: String,
  pub params: Vec<(String, f32)>,
  pub textures: Vec<(String, u64)>,
  pub attributes: Vec<(String, String)>,
  pub buffer_id: u64,
  pub topology: String,
  pub draw_count: i32,
  pub depth: bool,
  pub clear_color: [f32; 4],
}

/// Owned form of `context::TargetSpec`: a shader/pipeline target over an
/// already-compiled program, so everything of PipelineSpecOwned except the
/// sources. The mesh fields are meaningful only for pipeline programs; the UI
/// side rejects them for fragment programs before sending.
pub(crate) struct TargetSpecOwned {
  pub width: u32,
  pub height: u32,
  pub params: Vec<(String, f32)>,
  pub textures: Vec<(String, u64)>,
  pub attributes: Vec<(String, String)>,
  pub buffer_id: u64,
  pub topology: String,
  pub draw_count: i32,
  pub depth: bool,
  pub clear_color: [f32; 4],
}

/// The UI thread's half of the raster command channel, paired with the shared
/// queue-depth counter: every send increments it, and the raster loop
/// decrements it as each command finishes executing, so depth 0 means the
/// raster thread has nothing queued and nothing in hand. The frame loop reads
/// it to gate the idle Tick: a backlogged raster thread produces no presents,
/// which is otherwise indistinguishable from a genuinely idle GPU there (see
/// okf/backlog/idle-tick-gpu-backlog-runaway.md).
pub(crate) struct RasterSender {
  tx: mpsc::Sender<RasterCmd>,
  depth: Arc<AtomicUsize>,
}

impl RasterSender {
  pub(crate) fn new(tx: mpsc::Sender<RasterCmd>, depth: Arc<AtomicUsize>) -> Self {
    RasterSender { tx, depth }
  }

  /// Increment-before-send so the counter never under-reports: the raster
  /// thread may consume and decrement the instant the command is in the
  /// channel.
  pub(crate) fn send(&self, cmd: RasterCmd) -> Result<(), mpsc::SendError<RasterCmd>> {
    self.depth.fetch_add(1, Ordering::Release);
    let result = self.tx.send(cmd);
    if result.is_err() {
      self.depth.fetch_sub(1, Ordering::Release);
    }
    result
  }

  pub(crate) fn depth(&self) -> usize {
    self.depth.load(Ordering::Acquire)
  }
}

pub(crate) enum RasterCmd {
  /// Draw and present (interactive) or read back (playback) a frame. In
  /// interactive mode, when several frames are queued only the newest is
  /// drawn (load shedding); in capture mode every frame draws, because
  /// playback's contract is exactly one Captured per submit. `tree_clean`
  /// marks a present-only resubmit of the previous frame's unchanged display
  /// list (see `Context::submit_clean`).
  Frame { dl: DisplayList, tree_clean: bool },
  /// Re-run make-current so the context binds the window's current EGL
  /// surface. Android destroys the surface on background and SDL creates a
  /// fresh one on resume, but this thread's binding still points at the dead
  /// one, so every swap would fail with EGL_BAD_SURFACE. Sent on
  /// return-to-visible, ahead of the resume repaint's Frame on this ordered
  /// channel.
  RebindWindowSurface,
  /// Create (or replace, same id) a sampleable RGBA8 texture and adopt it
  /// into Impeller. Replies with the adopted handle for UI-side registration.
  CreateTexture {
    id: u64,
    width: u32,
    height: u32,
    pixels: Vec<u8>,
    reply: mpsc::Sender<Result<Texture, String>>,
  },
  /// Re-upload pixels into an existing texture; `pixels` is exactly one frame
  /// (the UI side slices multi-frame buffers before sending).
  UpdateTexture { id: u64, pixels: Vec<u8> },
  /// Compile a fragment shader, render once into a new target texture, adopt
  /// it. Compile errors must reach JS, hence the reply.
  CreateShaderTexture {
    id: u64,
    width: u32,
    height: u32,
    fragment_src: String,
    params: Vec<(String, f32)>,
    textures: Vec<(String, u64)>,
    reply: mpsc::Sender<Result<Texture, String>>,
  },
  /// Compile a vertex+fragment pipeline, render once, adopt the target.
  CreatePipelineTexture { id: u64, spec: PipelineSpecOwned, reply: mpsc::Sender<Result<Texture, String>> },
  /// Compile a single raw stage into the stage registry: a complete GLSL ES
  /// source, or one that explicitly asked for the standard header. Compile
  /// errors must reach JS, hence the reply.
  CompileStage { id: u64, stage: ShaderStage, source: String, header: bool, reply: mpsc::Sender<Result<(), String>> },
  /// Link two compiled stages into a program in the program registry. The
  /// stages remain usable for further links. Link errors reach JS via the
  /// reply.
  LinkProgram { id: u64, vertex: u64, fragment: u64, reply: mpsc::Sender<Result<(), String>> },
  /// Delete a compiled stage. Programs linked from it are unaffected (a
  /// linked program keeps its own compiled copies).
  DestroyStage { id: u64 },
  /// Create a target over an already-compiled program, render it once, adopt
  /// it. The target shares the program; many targets may share one.
  CreateShaderTarget { id: u64, program: u64, spec: TargetSpecOwned, reply: mpsc::Sender<Result<Texture, String>> },
  /// Drop a program from the registry. Targets created from it keep it alive
  /// (and keep rendering); the GL program is deleted when the last user goes.
  DestroyProgram { id: u64 },
  /// Declare (Some) or clear (None) the window shader. Fire-and-forget on
  /// this ordered channel, so it applies exactly between two frames. The
  /// declared program is held by Rc while active; the layer texture is
  /// allocated lazily by the first shaded frame and freed on clear.
  SetWindowShader { shader: Option<WindowShader> },
  /// Re-render an existing shader/pipeline target with new params.
  UpdateShaderParams { id: u64, params: Vec<(String, f32)> },
  /// Rebind an existing shader/pipeline target's sampler2D inputs by uniform
  /// name and re-render it with its last-applied params. Unnamed bindings
  /// keep their current source.
  UpdateShaderTextures { id: u64, textures: Vec<(String, u64)> },
  /// Recreate a shader/pipeline target at a new size (same compiled program,
  /// params, and bindings), re-render, and adopt the new target. Replies with
  /// the adopted handle so the UI side re-registers it under the same id.
  ResizeShaderTexture { id: u64, width: u32, height: u32, reply: mpsc::Sender<Result<Texture, String>> },
  /// Set a pipeline's vertex draw count and re-render it.
  SetDrawCount { id: u64, count: i32 },
  /// Drop raster-side bookkeeping for a texture id (and destroy its shader
  /// program/FBO when the id is a shader target). The GL name itself is owned
  /// by the adopted Impeller Texture and dies with the UI side's last handle.
  DestroyTexture { id: u64 },
  /// Create an interleaved vertex buffer from raw bytes.
  CreateBuffer { id: u64, data: Vec<u8>, reply: mpsc::Sender<Result<(), String>> },
  /// Overwrite part of a vertex buffer, then re-render pipelines drawing from it.
  WriteBuffer { id: u64, data: Vec<u8>, byte_offset: usize },
  /// Read back part of a vertex buffer.
  ReadBuffer { id: u64, byte_offset: usize, len: usize, reply: mpsc::Sender<Result<Vec<u8>, String>> },
  /// Free a vertex buffer.
  DestroyBuffer { id: u64 },
  /// Rasterize a display list into a new adopted texture (snapshot repaint
  /// boundaries). The handle goes back to the UI thread, which draws it.
  /// `aa: false` skips the multisampled rig (a "snapshot-no-aa" boundary).
  RasterizeDl { dl: DisplayList, width: u32, height: u32, aa: bool, reply: mpsc::Sender<Result<Texture, String>> },
  /// Re-rasterize into an existing adopted texture, reusing its storage
  /// (snapshot boundary whose retained allocation still fits). The texture's
  /// aligned backing must fit `width` x `height`; the UI thread checks this.
  RasterizeDlInto {
    dl: DisplayList,
    texture: Texture,
    width: u32,
    height: u32,
    aa: bool,
    reply: mpsc::Sender<Result<(), String>>,
  },
  /// Rasterize + read back `width` x `height` pixels in one trip (node
  /// captures). The intermediate padded texture never crosses threads.
  RasterizeReadback { dl: DisplayList, width: u32, height: u32, reply: mpsc::Sender<Result<Vec<u8>, String>> },
  /// Read back a texture's RGBA8 pixels by handle.
  ReadTexture { texture: Texture, width: u32, height: u32, reply: mpsc::Sender<Result<Vec<u8>, String>> },
  /// Inventory textures, buffers, and shader/pipeline targets.
  Resources { reply: mpsc::Sender<GpuResources> },
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
  backend: Backend,
  gl: glow::Context,
  impeller_ctx: ImpellerContext,
  // The window's raw SDL handle, for presenting; the Window itself lives on
  // the main thread.
  window: *mut sdl3::sys::video::SDL_Window,
  // Physical framebuffer size (see backend::pack_size), published by the main
  // thread on resize and read when wrapping FBO 0.
  surface_size: Arc<AtomicU64>,
  // Retained FBOs and scratch storage for every rig rasterization: the
  // window frame itself plus offscreen rasters (snapshot boundaries, node
  // captures), grown to the largest allocation requested. Retained because
  // a per-call allocate/release cycle is exactly what ANGLE/D3D11 handles
  // poorly (see the OffscreenRig doc in gl.rs).
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
  // Cumulative present-fence timeouts (GPU over budget), read live by
  // get_stats through the Context.
  fence_timeouts: Arc<AtomicU64>,
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
  // Shared shader/pipeline programs in their own id space. Targets hold their
  // program by Rc, so removal here only deletes the GL program once no target
  // uses it (see shader::release_program).
  programs: HashMap<u64, Rc<ShaderProgram>>,
  // Raw compiled stages in their own id space, inputs to LinkProgram. The GL
  // shader object is deleted on DestroyStage; linked programs are unaffected.
  stages: HashMap<u64, glow::Shader>,
  // Vertex buffers pipelines draw from, in their own id space.
  buffers: HashMap<u64, GpuBuffer>,
  // The declared window shader, with its retained layer texture. None = the
  // frame resolves straight to FBO 0 (the free path).
  window_shader: Option<WindowShaderState>,
  // Sampled content may have changed since the last layer resolve: set by
  // every command except Frame/SetWindowShader (and by any non-clean Frame),
  // cleared by a resolve. While set, a clean-tree frame may not skip its
  // resolve. Starts true (nothing resolved yet).
  content_dirty: bool,
  // Shaded frames that skipped the tree raster and ran only the pass over
  // the retained layer (the clean-tree fast path). Reported in
  // GpuWindowShaderInfo for verification.
  pass_only_frames: u64,
  // Commands sent but not yet executed (see RasterSender); decremented here
  // as each command completes.
  queue_depth: Arc<AtomicUsize>,
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

/// Reply to an RPC; a dead requester (UI thread shutting down) is not an error.
fn reply<T>(tx: mpsc::Sender<T>, value: T) {
  tx.send(value).ok();
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
    backend: Backend,
    gl: glow::Context,
    impeller_ctx: ImpellerContext,
    window: *mut sdl3::sys::video::SDL_Window,
    surface_size: Arc<AtomicU64>,
    capture_frames: bool,
    queue_depth: Arc<AtomicUsize>,
    fence_timeouts: Arc<AtomicU64>,
    tx: mpsc::Sender<FrameOutput>,
    wake: Option<Box<dyn Fn() + Send + Sync>>,
  ) -> Self {
    RasterState {
      backend,
      gl,
      impeller_ctx,
      window,
      surface_size,
      offscreen_rig: gl::OffscreenRig::new(),
      last_size: ISize::new(0, 0),
      capture_frames,
      present_failures: 0,
      slow_frame_log: None,
      fence_wait_log: None,
      fence_timeouts,
      timing: FrameTiming::new(),
      present_fences: std::collections::VecDeque::new(),
      textures: HashMap::new(),
      shaders: HashMap::new(),
      programs: HashMap::new(),
      stages: HashMap::new(),
      buffers: HashMap::new(),
      window_shader: None,
      content_dirty: true,
      pass_only_frames: 0,
      queue_depth,
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
      let last_frame = if self.capture_frames {
        None
      } else {
        batch.iter().rposition(|cmd| matches!(cmd, RasterCmd::Frame { .. }))
      };
      // Mirror the Frame load-shed for params updates: N params writes to one
      // shader queued in a batch render N times with only the last result ever
      // sampled, which is what lets a backlogged raster thread fall further
      // behind instead of catching up. A shed write folds its params into the
      // surviving render (uniforms are program state and params lists may be
      // partial, so dropping one outright could lose a uniform); by-name
      // application in order makes the one concatenated render equivalent to
      // N. Never shed across a frame that draws: that frame samples the
      // target, so every params write before it must have rendered.
      let mut shed = vec![false; batch.len()];
      {
        let mut later: HashSet<u64> = HashSet::new();
        for (i, cmd) in batch.iter().enumerate().rev() {
          match cmd {
            RasterCmd::Frame { .. } if self.capture_frames || Some(i) == last_frame => later.clear(),
            RasterCmd::UpdateShaderParams { id, .. } => shed[i] = !later.insert(*id),
            _ => {}
          }
        }
      }
      let mut shed_params: HashMap<u64, Vec<(String, f32)>> = HashMap::new();
      for (i, cmd) in batch.into_iter().enumerate() {
        // Any command that can change what a frame samples (texture uploads,
        // target renders, program changes, ...) invalidates the clean-tree
        // fast path until the next real resolve. Frame itself and window
        // shader redeclarations (the very writes the fast path exists for)
        // are the only exempt commands; a shader *program* change still
        // invalidates through its cleared layer.
        if !matches!(cmd, RasterCmd::Frame { .. } | RasterCmd::SetWindowShader { .. }) {
          self.content_dirty = true;
        }
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
          RasterCmd::CreateTexture { id, width, height, pixels, reply: tx } => {
            reply(tx, self.create_texture(id, width, height, &pixels));
          }
          RasterCmd::UpdateTexture { id, pixels } => {
            if let Err(e) = self.update_texture(id, &pixels) {
              log::warn!("[alloy] texture update failed: {e}");
            }
          }
          RasterCmd::CreateShaderTexture { id, width, height, fragment_src, params, textures, reply: tx } => {
            reply(tx, self.create_shader_texture(id, width, height, &fragment_src, &params, textures));
          }
          RasterCmd::CreatePipelineTexture { id, spec, reply: tx } => {
            reply(tx, self.create_pipeline_texture(id, &spec));
          }
          RasterCmd::CompileStage { id, stage, source, header, reply: tx } => {
            let result = crate::shader::compile_stage(&self.gl, stage, &source, header).map(|shader| {
              self.stages.insert(id, shader);
            });
            reply(tx, result);
          }
          RasterCmd::LinkProgram { id, vertex, fragment, reply: tx } => {
            reply(tx, self.link_program(id, vertex, fragment));
          }
          RasterCmd::DestroyStage { id } => {
            if let Some(shader) = self.stages.remove(&id) {
              crate::shader::delete_stage(&self.gl, shader);
            }
          }
          RasterCmd::CreateShaderTarget { id, program, spec, reply: tx } => {
            reply(tx, self.create_shader_target(id, program, &spec));
          }
          RasterCmd::DestroyProgram { id } => {
            if let Some(program) = self.programs.remove(&id) {
              release_program(&self.gl, program);
            }
          }
          RasterCmd::SetWindowShader { shader } => {
            self.set_window_shader(shader);
          }
          RasterCmd::UpdateShaderParams { id, params } => {
            if shed[i] {
              shed_params.entry(id).or_default().extend(params);
            } else {
              match self.shaders.get(&id) {
                Some(shader) => {
                  let mut all = shed_params.remove(&id).unwrap_or_default();
                  all.extend(params);
                  let resolved = resolve_sampler_bindings(&self.textures, shader);
                  shader.render(&self.gl, &all, &resolved);
                }
                None => log::warn!("[alloy] shader params update failed: shader texture {id} not found"),
              }
            }
          }
          RasterCmd::UpdateShaderTextures { id, textures } => {
            // Mutate first (needs &mut), then re-borrow shared for the render:
            // resolve_sampler_bindings reads the whole texture map alongside
            // the shader.
            let rebound = match self.shaders.get_mut(&id) {
              Some(shader) => shader.set_sampler_bindings(&textures),
              None => Err(format!("shader texture {id} not found")),
            };
            match rebound {
              Ok(()) => {
                let shader = self.shaders.get(&id).expect("shader present after rebind");
                let resolved = resolve_sampler_bindings(&self.textures, shader);
                shader.render(&self.gl, &shader.last_params(), &resolved);
              }
              Err(e) => log::warn!("[alloy] shader texture rebind failed: {e}"),
            }
          }
          RasterCmd::ResizeShaderTexture { id, width, height, reply: tx } => {
            reply(tx, self.resize_shader_texture(id, width, height));
          }
          RasterCmd::SetDrawCount { id, count } => {
            let result = self.shaders.get(&id).ok_or_else(|| format!("shader texture {id} not found")).and_then(
              |shader| {
                shader.set_draw_count(count)?;
                let resolved = resolve_sampler_bindings(&self.textures, shader);
                shader.render(&self.gl, &shader.last_params(), &resolved);
                Ok(())
              },
            );
            if let Err(e) = result {
              log::warn!("[alloy] draw count update failed: {e}");
            }
          }
          RasterCmd::DestroyTexture { id } => {
            self.textures.remove(&id);
            if let Some(shader) = self.shaders.remove(&id) {
              shader.destroy(&self.gl);
            }
          }
          RasterCmd::CreateBuffer { id, data, reply: tx } => {
            reply(
              tx,
              GpuBuffer::new(&self.gl, &data).map(|buffer| {
                self.buffers.insert(id, buffer);
              }),
            );
          }
          RasterCmd::WriteBuffer { id, data, byte_offset } => {
            if let Err(e) = self.write_buffer(id, &data, byte_offset) {
              log::warn!("[alloy] buffer write failed: {e}");
            }
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
              buffer.destroy(&self.gl);
            }
          }
          RasterCmd::RasterizeDl { dl, width, height, aa, reply: tx } => {
            reply(tx, self.rasterize(&dl, width, height, aa));
          }
          RasterCmd::RasterizeDlInto { dl, texture, width, height, aa, reply: tx } => {
            reply(tx, self.rasterize_into(&dl, &texture, width, height, aa));
          }
          RasterCmd::RasterizeReadback { dl, width, height, reply: tx } => {
            // Node captures carry no boundary prop, so they stay at full AA.
            let result = self.rasterize(&dl, width, height, true).and_then(|texture| {
              let size = ISize::new(width as i64, height as i64);
              gl::read_texture_pixels(&self.gl, &texture, size)
              // The intermediate padded texture drops here, on the context
              // thread; Impeller frees its GL name.
            });
            reply(tx, result);
          }
          RasterCmd::ReadTexture { texture, width, height, reply: tx } => {
            let size = ISize::new(width as i64, height as i64);
            reply(tx, gl::read_texture_pixels(&self.gl, &texture, size));
          }
          RasterCmd::Resources { reply: tx } => {
            reply(tx, self.resources());
          }
        }
        // The command is done (a load-shed one counts: it was consumed); the
        // frame-error exit above skips this, but the thread is gone then
        // anyway.
        self.queue_depth.fetch_sub(1, Ordering::Release);
      }
    }
  }

  /// Draw the frame's display list to the window backbuffer and hand it on:
  /// present in interactive mode, read the pixels back in playback mode. Then
  /// notify the main loop, which only does frame bookkeeping (fps,
  /// FrameRendered) and playback encoding. Err means the main loop is gone
  /// and this thread should exit.
  fn frame(&mut self, dl: DisplayList) -> Result<(), ()> {
    let (width, height) = crate::backend::unpack_size(self.surface_size.load(Ordering::Acquire));
    let size = ISize::new(width as i64, height as i64);
    let wait_start = std::time::Instant::now();
    self.await_present_fence();
    let wait_ms = wait_start.elapsed().as_secs_f32() * 1000.0;
    let draw_start = std::time::Instant::now();
    let drawn = self.draw_to_window(&dl, size);
    let draw_ms = draw_start.elapsed().as_secs_f32() * 1000.0;
    // TEMPORARY (swap-latency diagnosis): SRT_GL_FINISH=1 blocks on glFinish
    // between draw and present, splitting GPU execution time from present
    // blocking so the two theories in
    // okf/backlog/android-surface-swap-latency.md can be told apart.
    let finish_ms = if crate::sdl_utils::gl_finish_probe() {
      let finish_start = std::time::Instant::now();
      unsafe { glow::HasContext::finish(&self.gl) };
      finish_start.elapsed().as_secs_f32() * 1000.0
    } else {
      0.0
    };
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
      // per second so a sustained stall stays readable.
      if wait_ms + draw_ms + finish_ms + present_ms > 35.0 && self.slow_frame_log.is_none_or(|t| t.elapsed().as_secs() >= 1) {
        self.slow_frame_log = Some(std::time::Instant::now());
        log::warn!(
          "[alloy] slow frame: fence wait {wait_ms:.1}ms, draw {draw_ms:.1}ms, finish {finish_ms:.1}ms, present {present_ms:.1}ms"
        );
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
  /// get_stats (fenceTimeouts) and warned at 1/s, because a healthy discrete
  /// GPU never hits this while a saturated tiled one lives near it (see
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
            self.fence_timeouts.fetch_add(1, Ordering::Relaxed);
          }
          if self.fence_wait_log.is_none_or(|t| t.elapsed().as_secs() >= 1) {
            self.fence_wait_log = Some(std::time::Instant::now());
            if status == glow::TIMEOUT_EXPIRED {
              log::warn!(
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
          let (tex, fbo) = crate::shader::create_layer_target(&self.gl, width, height)?;
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
        let (tex, fbo) = crate::shader::create_layer_target(&self.gl, width, height)?;
        state.layer = Some(LayerTarget { tex, fbo, width, height });
      }
      let layer = state.layer.as_ref().expect("layer allocated above");

      gl::render_display_list_to_layer(&self.gl, &mut self.impeller_ctx, &mut self.offscreen_rig, &flipped, size, layer.fbo)?;
      self.content_dirty = false;
    }
    let layer = state.layer.as_ref().expect("resolved or retained above");

    // The layer binds as uSource, the history layer (when declared and live)
    // as uPrevious; extra declared inputs resolve through the registry by id,
    // a missing id dropping to unbound (samples black), the same contract as
    // shader targets.
    let mut textures: Vec<(String, glow::Texture)> = vec![("uSource".to_string(), layer.tex)];
    if state.spec.previous {
      if let Some(prev) = &state.prev_layer {
        textures.push(("uPrevious".to_string(), prev.tex));
      }
    }
    for (name, id) in &state.spec.textures {
      match self.textures.get(id) {
        Some(gpu) => textures.push((name.clone(), gpu.gl_texture)),
        None => log::warn!("[alloy] window shader input '{name}': texture {id} not found"),
      }
    }
    crate::shader::render_program_to_window(
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

  /// Swap the window's backbuffer; true on success. Without the failure
  /// check a lost context / removed device leaves the app running normally
  /// while nothing reaches the screen (a frozen window with no message). A
  /// failed swap gets one rebind-and-redraw recovery attempt (see `frame`);
  /// a confirmed loss exits instead: see okf/backlog/gpu-context-loss.md.
  fn present(&mut self) -> bool {
    if crate::sdl_utils::gl_swap_window_checked(self.window) {
      self.present_failures = 0;
      // At most one fence joins per frame (a retried present only follows a
      // failed one, which queued nothing), and `await_present_fence` trimmed
      // to depth-1 before the draw, so the queue never exceeds
      // PRESENT_FENCE_DEPTH. A failed fence_sync just means no pacing this
      // frame: same behavior as before this mechanism existed.
      if let Ok(fence) = unsafe { glow::HasContext::fence_sync(&self.gl, glow::SYNC_GPU_COMMANDS_COMPLETE, 0) } {
        self.present_fences.push_back(fence);
      }
      return true;
    }
    self.present_failures += 1;
    if self.present_failures == 1 {
      log::error!("[alloy] present failed: {}", crate::sdl_utils::sdl_error());
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
    if !crate::sdl_utils::gl_remake_current(self.window) {
      log::warn!("[alloy] rebind window surface failed: {}", crate::sdl_utils::sdl_error());
      return false;
    }
    if !self.capture_frames
      && !unsafe { sdl3::sys::video::SDL_GL_SetSwapInterval(crate::sdl_utils::window_swap_interval()) }
    {
      log::warn!("[alloy] SDL_GL_SetSwapInterval failed: {}", crate::sdl_utils::sdl_error());
    }
    true
  }

  fn create_texture(&mut self, id: u64, width: u32, height: u32, pixels: &[u8]) -> Result<Texture, String> {
    let size = ISize::new(width as i64, height as i64);
    let gpu = GpuTexture::new(&self.gl, self.backend, size);
    gpu.upload(&self.gl, pixels, size);
    match gl::adopt_texture(&gpu, &self.impeller_ctx, size) {
      Some(impeller) => {
        let replaced = self.textures.insert(id, gpu).is_some();
        // Replacing at an existing id (an id-stable resize): same contract as
        // UpdateTexture - shaders sampling this id re-render so they pick up
        // the new texture without waiting for a params change.
        if replaced {
          self.rerender_samplers_of(id);
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

  /// Re-render every shader/pipeline that samples texture id `id` with its
  /// last-applied params, so a content or registry change to the source is
  /// visible without a params update.
  fn rerender_samplers_of(&self, id: u64) {
    for shader in self.shaders.values() {
      if shader.sampler_bindings().iter().any(|(_, tex)| *tex == id) {
        let resolved = resolve_sampler_bindings(&self.textures, shader);
        shader.render(&self.gl, &shader.last_params(), &resolved);
      }
    }
  }

  /// Resize an existing shader/pipeline target in place: a new target texture
  /// on the same FBO and program, re-rendered at the new size with the
  /// last-applied params, then adopted into Impeller. Replies with the new
  /// handle so the UI side re-registers it under the same id; the old handle
  /// keeps the old GL name alive until in-flight display lists drop it.
  fn resize_shader_texture(&mut self, id: u64, width: u32, height: u32) -> Result<Texture, String> {
    let shader = self.shaders.get_mut(&id).ok_or_else(|| format!("shader texture {id} not found"))?;
    shader.resize(&self.gl, width, height)?;
    let shader = self.shaders.get(&id).expect("shader present after resize");
    let resolved = resolve_sampler_bindings(&self.textures, shader);
    shader.render(&self.gl, &shader.last_params(), &resolved);
    let size = ISize::new(width as i64, height as i64);
    let gpu = GpuTexture { gl_texture: shader.gl_texture(), backend: self.backend, width, height };
    match gl::adopt_texture(&gpu, &self.impeller_ctx, size) {
      Some(impeller) => {
        self.textures.insert(id, gpu);
        Ok(impeller)
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
    let expected = (gpu.width as usize) * (gpu.height as usize) * 4;
    if pixels.len() != expected {
      return Err(format!("texture {id} update is {} bytes, expected {expected}", pixels.len()));
    }
    let size = ISize::new(gpu.width as i64, gpu.height as i64);
    gpu.upload(&self.gl, pixels, size);
    // Shader targets sampling this texture show stale output until their next
    // params update; re-render them now (same contract as WriteBuffer, so
    // data-texture changes are visible without a params change).
    self.rerender_samplers_of(id);
    Ok(())
  }

  fn create_shader_texture(
    &mut self,
    id: u64,
    width: u32,
    height: u32,
    fragment_src: &str,
    params: &[(String, f32)],
    textures: Vec<(String, u64)>,
  ) -> Result<Texture, String> {
    let shader = ShaderTexture::new(&self.gl, width, height, fragment_src, textures)?;
    let resolved = resolve_sampler_bindings(&self.textures, &shader);
    shader.render(&self.gl, params, &resolved);
    self.register_shader_target(id, shader, width, height, "adopt shader texture failed")
  }

  fn create_pipeline_texture(&mut self, id: u64, spec: &PipelineSpecOwned) -> Result<Texture, String> {
    let (attrs, topology, vbo, draw_count) =
      resolve_mesh_spec(&self.buffers, &spec.attributes, &spec.topology, spec.buffer_id, spec.draw_count)?;
    let shader = ShaderTexture::new_pipeline(
      &self.gl,
      spec.width,
      spec.height,
      &spec.vertex_src,
      &spec.fragment_src,
      spec.textures.clone(),
      &attrs,
      vbo,
      spec.buffer_id,
      topology,
      draw_count,
      spec.depth,
      spec.clear_color,
    )?;
    let resolved = resolve_sampler_bindings(&self.textures, &shader);
    shader.render(&self.gl, &spec.params, &resolved);
    self.register_shader_target(id, shader, spec.width, spec.height, "adopt pipeline texture failed")
  }

  /// Link two compiled stages from the stage registry into a registered
  /// program. The UI side validated the ids and stage kinds against its
  /// mirror; a miss here means the mirrors diverged.
  fn link_program(&mut self, id: u64, vertex: u64, fragment: u64) -> Result<(), String> {
    let vs = *self.stages.get(&vertex).ok_or_else(|| format!("shader {vertex} not found"))?;
    let fs = *self.stages.get(&fragment).ok_or_else(|| format!("shader {fragment} not found"))?;
    let program = ShaderProgram::from_stages(&self.gl, vs, fs)?;
    self.programs.insert(id, Rc::new(program));
    Ok(())
  }

  /// Create a target over a registered program (the target half of the fused
  /// create paths), render it once, and adopt it under texture id `id`.
  fn create_shader_target(&mut self, id: u64, program_id: u64, spec: &TargetSpecOwned) -> Result<Texture, String> {
    let program = self.programs.get(&program_id).ok_or_else(|| format!("program {program_id} not found"))?.clone();
    let shader = if program.is_pipeline() {
      let (attrs, topology, vbo, draw_count) =
        resolve_mesh_spec(&self.buffers, &spec.attributes, &spec.topology, spec.buffer_id, spec.draw_count)?;
      ShaderTexture::from_pipeline_program(
        &self.gl,
        program,
        Some(program_id),
        spec.width,
        spec.height,
        spec.textures.clone(),
        &attrs,
        vbo,
        spec.buffer_id,
        topology,
        draw_count,
        spec.depth,
        spec.clear_color,
      )
      .map_err(|(_, e)| e)?
    } else {
      ShaderTexture::from_fragment_program(
        &self.gl,
        program,
        Some(program_id),
        spec.width,
        spec.height,
        spec.textures.clone(),
      )
      .map_err(|(_, e)| e)?
    };
    let resolved = resolve_sampler_bindings(&self.textures, &shader);
    shader.render(&self.gl, &spec.params, &resolved);
    self.register_shader_target(id, shader, spec.width, spec.height, "adopt shader target failed")
  }

  /// Adopt a freshly rendered shader/pipeline target into Impeller and record
  /// it under `id` in both the texture and shader maps.
  fn register_shader_target(
    &mut self,
    id: u64,
    shader: ShaderTexture,
    width: u32,
    height: u32,
    adopt_err: &str,
  ) -> Result<Texture, String> {
    let size = ISize::new(width as i64, height as i64);
    let gpu = GpuTexture { gl_texture: shader.gl_texture(), backend: self.backend, width, height };
    match gl::adopt_texture(&gpu, &self.impeller_ctx, size) {
      Some(impeller) => {
        self.textures.insert(id, gpu);
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

  fn write_buffer(&mut self, id: u64, data: &[u8], byte_offset: usize) -> Result<(), String> {
    let buffer = self.buffers.get(&id).ok_or_else(|| format!("buffer {id} not found"))?;
    buffer.write(&self.gl, data, byte_offset)?;
    // Re-render every pipeline drawing from this buffer with its last-applied
    // params, so geometry-only changes reach the screen even when no new
    // params arrive.
    for shader in self.shaders.values() {
      if shader.buffer_id() == Some(id) {
        let resolved = resolve_sampler_bindings(&self.textures, shader);
        shader.render(&self.gl, &shader.last_params(), &resolved);
      }
    }
    Ok(())
  }

  /// Rasterize a display list into a new adopted texture of the given pixel
  /// size, ready for sampling.
  fn rasterize(&mut self, dl: &DisplayList, width: u32, height: u32, aa: bool) -> Result<Texture, String> {
    let size = ISize::new(width as i64, height as i64);
    match self.backend {
      Backend::Gl => {
        let flipped = flip_for_fbo(dl, height)?;
        gl::render_display_list_to_texture(
          &self.gl,
          &mut self.impeller_ctx,
          &mut self.offscreen_rig,
          &flipped,
          size,
          aa,
        )
      }
      Backend::Vulkan => panic!("Vulkan backend not yet implemented"),
      Backend::Metal => panic!("Metal backend not yet implemented"),
    }
  }

  /// Re-rasterize a display list into an existing adopted texture whose
  /// aligned backing fits `width` x `height` (the UI thread checks the fit).
  fn rasterize_into(
    &mut self,
    dl: &DisplayList,
    texture: &Texture,
    width: u32,
    height: u32,
    aa: bool,
  ) -> Result<(), String> {
    let size = ISize::new(width as i64, height as i64);
    match self.backend {
      Backend::Gl => {
        let flipped = flip_for_fbo(dl, height)?;
        gl::render_display_list_into_texture(
          &self.gl,
          &mut self.impeller_ctx,
          &mut self.offscreen_rig,
          &flipped,
          texture,
          size,
          aa,
        )
      }
      Backend::Vulkan => panic!("Vulkan backend not yet implemented"),
      Backend::Metal => panic!("Metal backend not yet implemented"),
    }
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
      })
      .collect();
    textures.sort_by_key(|t| t.id);

    let mut buffers: Vec<GpuBufferInfo> =
      self.buffers.iter().map(|(id, b)| GpuBufferInfo { id: *id, byte_length: b.size }).collect();
    buffers.sort_by_key(|b| b.id);

    let mut pipelines: Vec<GpuPipelineInfo> = self
      .shaders
      .iter()
      .map(|(texture_id, shader)| GpuPipelineInfo {
        texture_id: *texture_id,
        kind: if shader.is_pipeline() { "pipeline" } else { "fragment" },
        program_id: shader.program_id(),
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

    let mut programs: Vec<GpuProgramInfo> = self
      .programs
      .iter()
      .map(|(id, program)| GpuProgramInfo { id: *id, kind: if program.is_pipeline() { "pipeline" } else { "fragment" } })
      .collect();
    programs.sort_by_key(|p| p.id);

    let window_shader = self.window_shader.as_ref().map(|state| GpuWindowShaderInfo {
      program_id: state.spec.program,
      width: state.layer.as_ref().map_or(0, |l| l.width),
      height: state.layer.as_ref().map_or(0, |l| l.height),
      previous: state.spec.previous && state.prev_layer.is_some(),
      pass_only_frames: self.pass_only_frames,
    });

    GpuResources { textures, buffers, pipelines, programs, window_shader }
  }
}

/// Resolve the mesh half of a pipeline (target) spec against the buffer
/// registry: parsed attribute formats, GL topology, the source buffer's GL
/// name, and the effective draw count (a negative request means "the whole
/// buffer", derived from buffer size / vertex stride).
fn resolve_mesh_spec(
  buffers: &HashMap<u64, GpuBuffer>,
  attributes: &[(String, String)],
  topology: &str,
  buffer_id: u64,
  draw_count: i32,
) -> Result<(Vec<(String, AttrFormat)>, u32, Option<glow::Buffer>, i32), String> {
  let mut attrs = Vec::with_capacity(attributes.len());
  for (name, fmt) in attributes {
    attrs.push((name.clone(), AttrFormat::parse(fmt)?));
  }
  let topology = crate::shader::parse_topology(topology)?;
  let buffer = if buffer_id != 0 {
    Some(buffers.get(&buffer_id).ok_or_else(|| format!("buffer {buffer_id} not found"))?)
  } else {
    None
  };
  let count = if draw_count >= 0 {
    draw_count
  } else {
    let stride = crate::shader::vertex_stride(&attrs);
    match buffer {
      Some(b) if stride > 0 => (b.size / stride as usize) as i32,
      _ => 0,
    }
  };
  Ok((attrs, topology, buffer.map(|b| b.vbo), count))
}

/// Map a shader's (name -> source texture id) bindings to live GL textures,
/// dropping any id no longer registered (it samples as unbound/black).
/// A wrapped FBO is treated like a window backbuffer, which GL stores
/// bottom-up; pre-flip the content so the texture ends up upright.
fn flip_for_fbo(dl: &DisplayList, height: u32) -> Result<DisplayList, String> {
  let mut flipped = impellers::DisplayListBuilder::new(None);
  flipped.translate(0.0, height as f32);
  flipped.scale(1.0, -1.0);
  flipped.draw_display_list(dl, 1.0);
  flipped.build().ok_or_else(|| "failed to build flipped display list".to_string())
}

fn resolve_sampler_bindings(
  textures: &HashMap<u64, GpuTexture>,
  shader: &ShaderTexture,
) -> Vec<(String, glow::Texture)> {
  shader
    .sampler_bindings()
    .iter()
    .filter_map(|(name, src_id)| textures.get(src_id).map(|gpu| (name.clone(), gpu.gl_texture)))
    .collect()
}
