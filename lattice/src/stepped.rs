// The stepping half of the headless hosts (okf/done/test-harness.md, stage
// 4): alloy's stepped mode emits no frame signal, so the host asks for every
// frame, and app time is frame / fps and nothing else. Two hosts step: test
// mode (test_host.rs, where a test asks through `srt:test`) and the render
// host (render_host.rs, which steps a fixed number of frames and reads each
// back). Both take the wall out of the engine (`performance.now()` reads 0,
// the calendar is a fixed epoch plus frame time, `Math.random` is seeded).

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

use alloy::resample::SharedResampler;
use alloy::AlloyEvent;
use flux::rquickjs::{Ctx, JsLifetime};
use tokio::sync::oneshot;

use crate::runtime::EventSender;

/// The frame rate a stepped host runs at unless told another: what apps run
/// at. The frame is the time resolution of a test, so one that asserts a
/// threshold to the millisecond sets 1000.
pub const DEFAULT_FPS: u32 = 60;
/// Where the calendar starts in a stepped engine: 2000-01-01T00:00:00Z. Any
/// fixed instant does; what matters is that it is the same on every run.
pub(crate) const EPOCH_MS: f64 = 946_684_800_000.0;

const NS_PER_SECOND: u64 = 1_000_000_000;

/// The frame signal alloy does not emit, and the frame rate. Context
/// userdata in a stepped engine, which `srt:test` and the render host step
/// through.
#[derive(Clone, JsLifetime)]
pub(crate) struct Stepper {
  #[qjs(skip_trace)]
  events: EventSender,
  // The index of the last frame computed, shared with the frame verb (which
  // publishes it) and the timeline (frame / fps).
  #[qjs(skip_trace)]
  frame: Arc<AtomicU64>,
  #[qjs(skip_trace)]
  fps: Arc<AtomicU32>,
  // Where a synthetic pointer's moves go, as a real producer's do (see
  // alloy's resample.rs): the frame verb samples them.
  #[qjs(skip_trace)]
  resampler: SharedResampler,
}

impl Stepper {
  pub(crate) fn new(
    events: EventSender,
    frame: Arc<AtomicU64>,
    fps: Arc<AtomicU32>,
    resampler: SharedResampler,
  ) -> Self {
    Self { events, frame, fps, resampler }
  }

  /// A new engine starts at frame 0, time 0, on `fps`.
  pub(crate) fn reset(&self, fps: u32) {
    self.frame.store(0, Ordering::Relaxed);
    self.fps.store(fps, Ordering::Relaxed);
    alloy::clock::set_virtual_ns(0);
  }

  pub(crate) fn fps(&self) -> u32 {
    self.fps.load(Ordering::Relaxed)
  }

  /// The index of the last frame computed.
  pub(crate) fn frame(&self) -> u64 {
    self.frame.load(Ordering::Relaxed)
  }

  /// App time as of the last frame computed, in ms.
  pub(crate) fn time_ms(&self) -> f64 {
    self.frame() as f64 * 1000.0 / self.fps() as f64
  }

  /// Another frame rate for this engine. Only before its first step: time
  /// is frame / fps, so a change later would move the time already passed.
  #[cfg(feature = "test")]
  pub(crate) fn set_fps(&self, fps: u32) -> Result<(), String> {
    if fps == 0 {
      return Err("the frame rate must be a positive integer".to_string());
    }
    if self.frame() != 0 {
      return Err("the frame rate is set before the first frame of a test".to_string());
    }
    self.fps.store(fps, Ordering::Relaxed);
    // The refresh rate is a fact the app reads (a frame callback's `rate`).
    let _ = self.events.send(AlloyEvent::DisplayRefreshRate { hz: fps as f32 });
    Ok(())
  }

  /// The frame interval, in ms.
  #[cfg(feature = "test")]
  pub(crate) fn frame_ms(&self) -> f64 {
    1000.0 / self.fps() as f64
  }

  /// Send a pointer, key, wheel or text event into the real pipeline, the
  /// way the control API does: a down, an up, a key or a wheel enters the
  /// UI loop's channel and is dispatched on arrival, ahead of the next
  /// frame the host asks for; a move feeds the resampler and is dispatched
  /// with that frame.
  pub(crate) fn inject(&self, event: AlloyEvent) {
    let events = self.events.clone();
    let _ = self.resampler.feed(event, std::time::Instant::now(), |event, at| events.send_at(event, at));
  }

  /// Send an event that is no pointer input (a pad snapshot, a link)
  /// straight into the UI loop's channel: dispatched on arrival, ahead of
  /// the next frame.
  #[cfg(feature = "test")]
  pub(crate) fn send(&self, event: AlloyEvent) {
    let _ = self.events.send(event);
  }

  /// Ask for the next frame: the signal the UI loop turns into the frame
  /// verb, at the next frame's virtual time, which is also the process
  /// clock's reading from here (what video is latched against).
  pub(crate) fn step(&self) {
    let frame = self.frame();
    let fps = self.fps();
    let virtual_ns = ((frame + 1) * NS_PER_SECOND / fps as u64) as i64;
    alloy::clock::set_virtual_ns(virtual_ns);
    let at = alloy::clock::at(virtual_ns);
    let signal = AlloyEvent::FrameRendered { frame, fps, refreshes: 1, present_at: at, reference: at, grid: at };
    let _ = self.events.send(signal);
  }
}

/// Whether the window's size has reached this engine (the first resize,
/// which is what an app's first frame is built on), and who waits for it:
/// a test's mount (`srt:test` `windowReady`, a JS promise) or the render
/// host (a native waiter). Installed with the engine, ahead of any import:
/// the resize can land before the file has evaluated.
#[derive(Clone, Default, JsLifetime)]
pub(crate) struct WindowReady(#[qjs(skip_trace)] Rc<RefCell<ReadyState>>);

#[derive(Default)]
struct ReadyState {
  ready: bool,
  promises: Vec<(flux::rquickjs::Persistent<flux::rquickjs::Function<'static>>, flux::Hold)>,
  waiters: Vec<oneshot::Sender<()>>,
}

impl WindowReady {
  /// Whether the size is there.
  #[cfg(feature = "test")]
  pub(crate) fn is_ready(&self) -> bool {
    self.0.borrow().ready
  }

  /// Hold a promise's resolver (and the engine) until the size is there.
  #[cfg(feature = "test")]
  pub(crate) fn wait_promise(
    &self,
    resolve: flux::rquickjs::Persistent<flux::rquickjs::Function<'static>>,
    hold: flux::Hold,
  ) {
    self.0.borrow_mut().promises.push((resolve, hold));
  }

  /// A native waiter: resolved at once when the size is there already.
  pub(crate) fn wait(&self) -> oneshot::Receiver<()> {
    let (tx, rx) = oneshot::channel();
    let mut state = self.0.borrow_mut();
    if state.ready {
      let _ = tx.send(());
    } else {
      state.waiters.push(tx);
    }
    rx
  }
}

/// The window's size reached this engine: whoever waited on it goes on.
pub(crate) fn window_ready(ctx: &Ctx<'_>) {
  let Some(state) = ctx.userdata::<WindowReady>() else {
    return;
  };
  let (promises, waiters) = {
    let mut state = state.0.borrow_mut();
    state.ready = true;
    (std::mem::take(&mut state.promises), std::mem::take(&mut state.waiters))
  };
  for (resolve, _hold) in promises {
    if let Err(e) = resolve.restore(ctx).and_then(|resolve| resolve.call::<_, ()>(())) {
      flux::report_uncaught(ctx, e, "srt:test windowReady()");
    }
  }
  for waiter in waiters {
    let _ = waiter.send(());
  }
}
