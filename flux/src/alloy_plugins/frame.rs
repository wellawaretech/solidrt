// The per-frame protocol over the gui plugins: the order their per-frame
// hooks run in is fixed here, where the plugins live, so a runner drives a
// frame with three calls (`advance`, `deliver`, `draw`) and never learns
// which plugins exist or what each hook returns. The runner keeps what is
// its own: input dispatch ahead of the frame, the clock policy (which app
// time and timer time this frame gets, and whether it is delivered at all),
// and its policy around the draw phases.

use std::cell::RefCell;

use rquickjs::{Ctx, Object};

use alloy::rendertree::composite::PaintStats;
use alloy::rendertree::{self, FrameBuilder, PendingFrame, PlatformContext, RenderTree};

use super::{camera, gpu, raf, spatial, tree};

/// The pre-delivery half of a frame, run once per frame signal before the
/// frame's JS: stamp both animation clocks with the frame's app time
/// (`now_ms`, the timeline rAF and the render event report), advance the
/// clip players (so onFrame handlers read and can overwrite the fresh
/// poses), then tick the capture and playback devices (camera, video, gpu
/// capture settles). Content a device or player changed, or a player still
/// running, latches a frame request. `period_ms` is the display refresh
/// period video frame selection looks ahead by; None when the runner has
/// no presentation model (playback). Runs whether or not the frame is
/// delivered: a paused clock stops app time, not the devices. No-op before
/// the GUI is installed.
pub fn advance(ctx: &Ctx<'_>, now_ms: f64, period_ms: Option<f64>) {
  let Some(s) = tree::try_state(ctx) else {
    return;
  };
  tree::stamp_clock(ctx, now_ms);
  spatial::stamp_clock(ctx, now_ms);
  let players = spatial::advance_players(ctx);
  let mut demand = players.active || players.wrote;
  // A camera frame landed in its texture: the screen content changed even
  // though the tree did not.
  demand |= camera::tick(ctx);
  #[cfg(feature = "video")]
  {
    // Same for a video frame uploaded into its player's texture; a
    // mid-playback player is standing demand for the next tick, so video
    // rides the frame grid instead of free-running on its own uploads.
    let period_us = period_ms.map(|p| (p * 1000.0) as i64).unwrap_or(0);
    let video = super::video::tick(ctx, period_us);
    demand |= video.uploaded || video.playing;
  }
  #[cfg(not(feature = "video"))]
  let _ = period_ms;
  // Settle any captureSnapshot promises whose captures alloy rendered on the
  // previous paint pass.
  gpu::tick(ctx);
  if demand {
    s.gui.platform.request_frame();
  }
}

/// The delivery half: hand the frame to JS. Timers fire first, one
/// task-queue turn on the timer reading (`timer_now_ms`; see
/// `advance_virtual_time`), then the rAF callbacks and the "render" event
/// (payload `{ frame, time }`, time in seconds) on the app time `now_ms`,
/// so the frame's render handler consumes the state the callbacks dirtied.
/// The runner skips this call to pause app time.
pub fn deliver(ctx: &Ctx<'_>, frame: u64, now_ms: f64, timer_now_ms: f64) {
  crate::standards_plugins::time::advance_virtual_time(ctx, timer_now_ms);
  raf::flush(ctx, now_ms);
  let payload = Object::new(ctx.clone()).expect("create render event object");
  payload.set("frame", frame).expect("set frame");
  payload.set("time", now_ms / 1000.0).expect("set time");
  crate::emit_event(ctx, "render", payload);
}

/// One frame of the draw protocol over the shared render tree, the same on
/// every path (the runner's per-frame draw, the direct `render` export): the
/// transition ticks (the render tree's tracks, then the spatial arena's,
/// each reporting its settles to JS before the frame paints), the demand
/// gate, then the build `f` sequences through the handle - commit, and on a
/// rebuild layout, paint and finish with the caller's own work between the
/// phases (a post-layout hook, hover refresh). `f` gets None when nothing
/// wanted a frame: the gate consumed no request and `extra_demand`, the
/// caller's own reason to draw, was false. Running transitions are demand
/// and re-request the next frame here, so the loop ticks until they settle.
/// One driver per tree, so consecutive frames on either path reuse the
/// retained display list. Tree borrows are scoped to each phase call, so
/// JS run between the phases may write properties.
pub fn draw<R>(ctx: &Ctx<'_>, extra_demand: bool, f: impl FnOnce(Option<Frame<'_>>) -> R) -> R {
  let Some(s) = tree::try_state(ctx) else {
    return f(None);
  };
  // Before the gate: the ticks' damage is this frame's reason to rebuild.
  let anim_active = tree::tick(ctx);
  let spatial = spatial::tick(ctx);
  // A JS hook run between the phases (a transitionEnd handler above, a
  // post-layout handler below) can call the direct `render` export; that
  // nested draw finds the driver taken and skips rather than panics.
  let Ok(mut driver) = s.render_driver.try_borrow_mut() else {
    log::warn!("[render] nested draw ignored: a frame is already being built");
    return f(None);
  };
  let demand = extra_demand || anim_active || spatial.active || spatial.wrote;
  let Some(pending) = driver.begin(&s.gui.platform, demand) else {
    return f(None);
  };
  if anim_active || spatial.active {
    s.gui.platform.request_frame();
  }
  f(Some(Frame { pending, tree: &s.tree, platform: &s.gui.platform, atx: &s.gui.alloy }))
}

/// A frame past the demand gate (see `draw`), bound to the tree it draws.
pub struct Frame<'a> {
  pending: PendingFrame<'a>,
  tree: &'a RefCell<RenderTree>,
  platform: &'a PlatformContext,
  atx: &'a alloy::Context,
}

impl<'a> Frame<'a> {
  /// Resolve the frame: GPU content damage applied, then either the retained
  /// display list resubmitted (`Reused`) or the build handle. `Err` means the
  /// render thread is gone.
  pub fn commit(self) -> Result<Commit<'a>, ()> {
    let Frame { pending, tree, platform, atx } = self;
    let commit = pending.commit(&mut tree.borrow_mut(), platform, atx)?;
    Ok(match commit {
      rendertree::Commit::Reused { content_changed } => Commit::Reused { content_changed },
      rendertree::Commit::Build(builder) => Commit::Build(Build { builder, tree, platform, atx }),
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
    let Build { builder, tree, platform, atx } = self;
    let result = builder.finish(&tree.borrow(), platform, atx);
    result
  }
}
