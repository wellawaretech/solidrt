// The per-frame protocol over the gui plugins: the order their per-frame
// hooks run in is fixed here, where the plugins live, so a runner drives a
// frame with four calls (`advance`, `deliver`, `draw`, then `idle` once
// the frame's work is behind it) and never learns which plugins exist or
// what each hook returns. The runner keeps what is its own: input dispatch
// ahead of the frame, the clock policy (which app time and timer time this
// frame gets, and whether it is delivered at all), its policy around the
// draw phases, and how long the idle period after the frame is.

use std::cell::RefCell;
use std::time::Instant;

use rquickjs::{Ctx, Object};

use alloy::rendertree::composite::PaintStats;
use alloy::rendertree::{self, FrameBuilder, PendingFrame, PlatformContext, RenderTree};

use super::{camera, font, gpu, raf, spatial, tree};

/// The pre-delivery half of a frame, run once per frame signal before the
/// frame's JS: stamp both animation clocks with the frame's app time
/// (`now_ms`, the timeline rAF and the render event report), advance the
/// native motion of the spatial arena - the clip players, then the node
/// transitions - so the frame's JS reads the posed nodes and can
/// overwrite them (the frame's write wins the frame, the producer rule),
/// the late pass of `onBeforeRender` sees the final pose and the publish
/// pass hands what derives from it to the engine; then tick the capture
/// devices (camera, gpu capture settles). What the motion has to tell JS
/// waits for `deliver`. Content a device or mover changed latches a
/// frame request; motion still running is standing demand `draw`
/// re-requests past its gate, and this frame's reasons are seeded here
/// (`draw` appends its own). Video is not here: its frames reach their
/// textures through the raster thread's latch, off the frame loop. Runs
/// whether or not the frame is delivered: a paused clock stops app time
/// (an unchanged stamp steps nothing), not the devices. No-op before the
/// GUI is installed.
pub fn advance(ctx: &Ctx<'_>, now_ms: f64) {
  let Some(s) = tree::try_state(ctx) else {
    return;
  };
  tree::stamp_clock(ctx, now_ms);
  spatial::stamp_clock(ctx, now_ms);
  // This frame's reasons to ask for the next one are collected from here.
  let mut reasons = Vec::new();
  let players = spatial::advance_players(ctx);
  let transitions = spatial::advance_transitions(ctx);
  let mut demand = players.active || players.wrote || transitions;
  if players.active {
    reasons.push("an animation player".to_string());
  }
  if transitions {
    reasons.push("a spatial transition".to_string());
  }
  s.gui.motion.set(super::NativeMotion { players: players.active, transitions });
  // A camera frame landed in its texture: the screen content changed even
  // though the tree did not.
  if camera::tick(ctx) {
    demand = true;
    reasons.push("a camera".to_string());
  }
  // Glyph cells landed in a font atlas: a label drawing from it changes
  // without any tree mutation.
  if font::tick(ctx) {
    demand = true;
    reasons.push("a glyph atlas".to_string());
  }
  // Settle any captureSnapshot promises whose captures alloy rendered on the
  // previous paint pass.
  gpu::tick(ctx);
  // The modules' ticks, after the devices. Taken out while they run so a
  // tick may register another; those go in behind. A tick that asks for
  // the next frame is standing demand: noted for `draw`, which
  // re-requests past its gate (the request made here is what this
  // frame's gate consumes).
  let mut ticks = std::mem::take(&mut *s.gui.ticks.borrow_mut());
  let mut ticking = Vec::new();
  for tick in &mut ticks {
    if (tick.run)(ctx, now_ms) {
      demand = true;
      ticking.push(tick.reason);
    }
  }
  ticks.append(&mut s.gui.ticks.borrow_mut());
  *s.gui.ticks.borrow_mut() = ticks;
  *s.gui.ticking.borrow_mut() = ticking;
  *s.demand.borrow_mut() = reasons;
  if demand {
    s.gui.platform.request_frame();
  }
}

/// A per-frame hook in the frame protocol, for a module that paces work by
/// the frame (a physics world stepping on a fixed timestep, a device read
/// per frame): run in `advance` after the devices, with the frame's app
/// time, so a paused clock holds it and a stepped run drives it
/// deterministically. Returning true asks for the next frame under
/// `reason`, standing demand the way a running transition is (re-requested
/// past the draw gate, which consumes this frame's request), so the loop
/// ticks while the module has motion and a settle waits for it. Per
/// engine: registered from a module's init or `evaluate`, released with
/// the engine.
pub fn on_advance(ctx: &Ctx<'_>, reason: &'static str, run: impl for<'js> FnMut(&Ctx<'js>, f64) -> bool + 'static) {
  let Some(gui) = super::try_gui(ctx) else {
    log::warn!("[render] on_advance before the gui is installed: {reason} will never tick");
    return;
  };
  gui.ticks.borrow_mut().push(super::Tick { reason, run: Box::new(run) });
}

/// The delivery half: hand the frame to JS. Timers fire first, one
/// task-queue turn on the timer reading (`timer_now_ms`; see
/// `advance_virtual_time`), then the frame on the app time `now_ms`: the
/// "frameStart" event (payload `{ frame, time }`, time in seconds) that
/// stamps the frame's clock for JS, what the advance's native motion has
/// to report (a clip's end, a settle: handlers see the frame's poses and
/// its clock), the rAF callbacks, and the "render" event (the same
/// payload), so the frame's render handler consumes the state the
/// callbacks dirtied. The runner skips this call to pause app time; the
/// motion's reports then wait for the next delivery.
pub fn deliver(ctx: &Ctx<'_>, frame: u64, now_ms: f64, timer_now_ms: f64) {
  crate::standards_plugins::time::advance_virtual_time(ctx, timer_now_ms);
  crate::emit_event(ctx, "frameStart", frame_payload(ctx, frame, now_ms));
  spatial::deliver_motion(ctx);
  raf::flush(ctx, now_ms);
  crate::emit_event(ctx, "render", frame_payload(ctx, frame, now_ms));
}

fn frame_payload<'js>(ctx: &Ctx<'js>, frame: u64, now_ms: f64) -> Object<'js> {
  let payload = Object::new(ctx.clone()).expect("create frame event object");
  payload.set("frame", frame).expect("set frame");
  payload.set("time", now_ms / 1000.0).expect("set time");
  payload
}

/// The idle half: the idle period after a delivered frame, for the
/// `requestIdleCallback` queue (see `time::run_idle_period`). The runner
/// calls it in an exec of its own after the frame's exec, so the frame's
/// microtask checkpoint is behind it, and only while `idle_due` says a
/// callback waits. `until` is when the next frame is due (the frame's
/// present deadline, which is when the next signal comes), None when
/// nothing is due: a stepped host between its frames, where the period gets
/// the full budget.
pub fn idle(ctx: &Ctx<'_>, until: Option<Instant>) {
  crate::standards_plugins::time::run_idle_period(ctx, until.map(tokio::time::Instant::from_std));
}

/// One frame of the draw protocol over the shared render tree, the same on
/// every path (the runner's per-frame draw, the direct `render` export): the
/// transition ticks (the render tree's tracks, reporting their settles to
/// JS before the frame paints; then the spatial arena's starts and flush,
/// its step having run in `advance` ahead of the frame's JS), the demand
/// gate, then the build `f` sequences through the handle - commit, and on a
/// rebuild layout, paint and finish with the caller's own work between the
/// phases (a post-layout hook, hover refresh). `f` gets None when nothing
/// wanted a frame: the gate consumed no request and `extra_demand`, the
/// caller's own reason to draw, was false. Running transitions and native
/// motion are demand and re-request the next frame here, so the loop ticks
/// until they settle.
/// One driver per tree, so consecutive frames on either path reuse the
/// retained display list. Tree borrows are scoped to each phase call, so
/// JS run between the phases may write properties. `present_at` is when
/// the frame is expected to reach the screen: the deadline the raster
/// thread latches video frames against, and the one the gate peeks with.
pub fn draw<R>(ctx: &Ctx<'_>, extra_demand: bool, present_at: Instant, f: impl FnOnce(Option<Frame<'_>>) -> R) -> R {
  let Some(s) = tree::try_state(ctx) else {
    return f(None);
  };
  // Before the gate: the ticks' damage is this frame's reason to rebuild.
  let anim_active = tree::tick(ctx);
  let spatial = spatial::tick(ctx);
  // A video frame the raster thread will latch for this present changes a
  // texture's pixels: noted now, by the same rule the raster applies, so a
  // node showing it is damaged and a cached boundary over it re-rasters.
  let video_due = s.gui.alloy.note_due_video(present_at);
  // A JS hook run between the phases (a transitionEnd handler above, a
  // post-layout handler below) can call the direct `render` export; that
  // nested draw finds the driver taken and skips rather than panics.
  let Ok(mut driver) = s.render_driver.try_borrow_mut() else {
    log::warn!("[render] nested draw ignored: a frame is already being built");
    return f(None);
  };
  let demand = extra_demand || anim_active || spatial.active || spatial.wrote || video_due;
  let Some(pending) = driver.begin(&s.gui.platform, demand) else {
    return f(None);
  };
  // Standing demand re-requests past the gate, which just consumed this
  // frame's request: running transitions, an animation-frame callback
  // registered for the next frame (registered during the flush, before the
  // gate), and a frame request declared standing by its caller (core's
  // `onFrame`). The latch then reads true from here to the next gate - in
  // particular when the raster thread samples it at present time to tell a
  // missed present from an idle gap (alloy's `demand_at_present`), and so
  // an animating app's intervals are judged.
  // A streaming texture (a playing video) is standing demand alloy holds:
  // the loop ticks on the refresh grid while it plays. A module's tick
  // that asked for the next frame (`on_advance`) is standing demand too,
  // and so is the native motion `advance` found running (its reasons are
  // already in) or the spatial starts just set running.
  let on_frame = s.gui.platform.take_standing_demand();
  let streaming = s.gui.alloy.streaming_textures();
  let raf = super::raf::has_pending(ctx);
  let ticking = std::mem::take(&mut *s.gui.ticking.borrow_mut());
  let motion = s.gui.motion.replace(super::NativeMotion::default());
  if anim_active || motion.any() || spatial.active || on_frame || streaming || raf || !ticking.is_empty() {
    s.gui.platform.request_frame();
  }
  {
    let mut reasons = s.demand.borrow_mut();
    for reason in ticking {
      reasons.push(reason.to_string());
    }
    if anim_active {
      let node = s.tree.borrow().running_transition();
      reasons.push(match node {
        Some(node) => format!("a transition on {node}"),
        None => "a transition".to_string(),
      });
    }
    // The advance reported the transitions it found running; the starts
    // may have set the first one running since.
    if spatial.active && !motion.transitions {
      reasons.push("a spatial transition".to_string());
    }
    if on_frame {
      reasons.push("onFrame".to_string());
    }
    if raf {
      reasons.push("requestAnimationFrame".to_string());
    }
    if streaming {
      reasons.push("a playing video".to_string());
    }
  }
  f(Some(Frame { pending, tree: &s.tree, platform: &s.gui.platform, atx: &s.gui.alloy, present_at }))
}

/// Why the app wants another frame, as of now: empty when it wants none.
/// A frame is wanted while the request latch is set or a capture waits for
/// its paint. The entries name the standing reasons the last frame found (a
/// running transition and its node, `onFrame`, a playing video, ...); a
/// request with none of them is a one-shot write since that frame, reported
/// as such. What a waiter reads to decide whether the app is at rest, and
/// what it says when the app never comes to rest. Empty before the GUI is
/// installed.
pub fn demand(ctx: &Ctx<'_>) -> Vec<String> {
  let Some(s) = tree::try_state(ctx) else {
    return Vec::new();
  };
  let capture = gpu::capture_pending(ctx);
  let requested = s.gui.platform.frame_request_handle().load(std::sync::atomic::Ordering::Relaxed);
  if !requested && !capture {
    return Vec::new();
  }
  let mut reasons = if requested { s.demand.borrow().clone() } else { Vec::new() };
  if capture {
    reasons.push("captureSnapshot".to_string());
  }
  if reasons.is_empty() {
    reasons.push("a write to the tree or to a texture".to_string());
  }
  reasons
}

/// A frame past the demand gate (see `draw`), bound to the tree it draws.
pub struct Frame<'a> {
  pending: PendingFrame<'a>,
  tree: &'a RefCell<RenderTree>,
  platform: &'a PlatformContext,
  atx: &'a alloy::Context,
  present_at: Instant,
}

impl<'a> Frame<'a> {
  /// Resolve the frame: GPU content damage applied, then either the retained
  /// display list resubmitted (`Reused`) or the build handle. `Err` means the
  /// render thread is gone.
  pub fn commit(self) -> Result<Commit<'a>, ()> {
    let Frame { pending, tree, platform, atx, present_at } = self;
    let commit = pending.commit(&mut tree.borrow_mut(), platform, atx, present_at)?;
    Ok(match commit {
      rendertree::Commit::Reused { content_changed } => Commit::Reused { content_changed },
      rendertree::Commit::Build(builder) => Commit::Build(Build { builder, tree, platform, atx, present_at }),
    })
  }
}

/// How `Frame::commit` resolved the frame (alloy's `Commit`, tree-bound).
pub enum Commit<'a> {
  /// The retained display list was resubmitted; the frame is done.
  /// `content_changed` says whether GPU writes since the last frame changed
  /// the picture behind it (a layer or shader app's every frame).
  Reused { content_changed: bool },
  /// Something changed: sequence the build to produce the frame.
  Build(Build<'a>),
}

/// The rebuild half of a frame: `layout`, then `paint` (which re-runs layout
/// itself, so writes made between the two are absorbed), then `finish`
/// builds, retains and submits the display list.
pub struct Build<'a> {
  builder: FrameBuilder<'a>,
  tree: &'a RefCell<RenderTree>,
  platform: &'a PlatformContext,
  atx: &'a alloy::Context,
  present_at: Instant,
}

impl Build<'_> {
  pub fn layout(&mut self) {
    self.builder.layout(&mut self.tree.borrow_mut(), self.platform, self.atx);
  }

  pub fn paint(&mut self) -> PaintStats {
    self.builder.paint(&mut self.tree.borrow_mut(), self.platform, self.atx)
  }

  /// `Err` means the render thread is gone.
  pub fn finish(self) -> Result<(), ()> {
    let Build { builder, tree, platform, atx, present_at } = self;
    let result = builder.finish(&tree.borrow(), platform, atx, present_at);
    result
  }
}
