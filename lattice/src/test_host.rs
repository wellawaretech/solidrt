// Test mode (okf/plans/test-harness.md, stage 4): the dev client as a test
// host. Headless on alloy's stepped mode, which emits no frame signal: a
// test asks for every frame (`srt:test` `frame`), so app time is frame / fps
// and nothing else, and a frame nobody demanded draws nothing. The wall is
// out of the engine (`performance.now()` reads 0, the calendar is a fixed
// epoch plus frame time, `Math.random` is seeded), and every test runs in an
// engine of its own, built by the engine loop like any reload: the file is
// evaluated once for the listing and once more per test (flux::test).

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

use alloy::resample::SharedResampler;
use alloy::AlloyEvent;
use flux::rquickjs::JsLifetime;

use crate::input_plan::Injected;
use crate::runtime::EventSender;

/// The frame rate a test steps at unless it sets another: what apps run at
/// and what `srt render` defaults to. The frame is the time resolution of a
/// test, so one that asserts a threshold to the millisecond sets 1000.
pub const DEFAULT_FPS: u32 = 60;
/// Where the calendar starts in every test engine: 2000-01-01T00:00:00Z.
/// Any fixed instant does; what matters is that it is the same on every run.
pub(crate) const EPOCH_MS: f64 = 946_684_800_000.0;

const NS_PER_SECOND: u64 = 1_000_000_000;

/// What a test run is started with (see `start_tests`).
pub struct TestRun {
  pub options: flux::test::RunOptions,
}

/// The stepping half of test mode: the frame signal alloy does not emit.
/// Context userdata in a test engine, which is what `srt:test` steps through.
#[derive(Clone, JsLifetime)]
pub(crate) struct Stepper {
  #[qjs(skip_trace)]
  events: EventSender,
  // The index of the last frame computed, shared with the frame verb (which
  // publishes it) and the timeline (frame / fps).
  #[qjs(skip_trace)]
  frame: Arc<AtomicU64>,
  #[qjs(skip_trace)]
  rate: Arc<AtomicU32>,
  // Where a synthetic pointer's moves go, as a real producer's do (see
  // alloy's resample.rs): the frame verb samples them.
  #[qjs(skip_trace)]
  resampler: SharedResampler,
}

impl Stepper {
  pub(crate) fn new(
    events: EventSender,
    frame: Arc<AtomicU64>,
    rate: Arc<AtomicU32>,
    resampler: SharedResampler,
  ) -> Self {
    Self { events, frame, rate, resampler }
  }

  /// A new engine starts at frame 0, time 0, on the default rate.
  pub(crate) fn reset(&self) {
    self.frame.store(0, Ordering::Relaxed);
    self.rate.store(DEFAULT_FPS, Ordering::Relaxed);
    alloy::clock::set_virtual_ns(0);
  }

  pub(crate) fn rate(&self) -> u32 {
    self.rate.load(Ordering::Relaxed)
  }

  /// App time as of the last frame computed, in ms.
  pub(crate) fn time_ms(&self) -> f64 {
    self.frame.load(Ordering::Relaxed) as f64 * 1000.0 / self.rate() as f64
  }

  /// Another frame rate for this engine. Only before its first step: time
  /// is frame / fps, so a change later would move the time already passed.
  pub(crate) fn set_rate(&self, fps: u32) -> Result<(), String> {
    if fps == 0 {
      return Err("the frame rate must be a positive integer".to_string());
    }
    if self.frame.load(Ordering::Relaxed) != 0 {
      return Err("the frame rate is set before the first frame of a test".to_string());
    }
    self.rate.store(fps, Ordering::Relaxed);
    // The refresh rate is a fact the app reads (a frame callback's `rate`).
    let _ = self.events.send(AlloyEvent::DisplayRefreshRate { hz: fps as f32 });
    Ok(())
  }

  /// The frame interval, in ms.
  pub(crate) fn frame_ms(&self) -> f64 {
    1000.0 / self.rate() as f64
  }

  /// Send one step of an input plan into the real pipeline, the way the
  /// control API does: a down, an up, a key or a wheel enters the UI
  /// loop's channel and is dispatched on arrival, ahead of the next frame
  /// the test asks for; a move feeds the resampler and is dispatched with
  /// that frame.
  pub(crate) fn inject(&self, step: Injected) -> Result<(), String> {
    match step {
      Injected::Event(event) => {
        let events = self.events.clone();
        let _ = self.resampler.feed(event, std::time::Instant::now(), |event, at| events.send_at(event, at));
        Ok(())
      }
      // The pads live in alloy's interactive loop, which a test does not
      // run (okf/plans/test-harness.md, step 4.3).
      Injected::Gamepad(_) => Err("a synthetic gamepad is not available in a test yet".to_string()),
    }
  }

  /// Ask for the next frame: the signal the UI loop turns into the frame
  /// verb, at the next frame's virtual time, which is also the process
  /// clock's reading from here (what video is latched against).
  pub(crate) fn step(&self) {
    let frame = self.frame.load(Ordering::Relaxed);
    let fps = self.rate();
    let virtual_ns = ((frame + 1) * NS_PER_SECOND / fps as u64) as i64;
    alloy::clock::set_virtual_ns(virtual_ns);
    let at = alloy::clock::at(virtual_ns);
    let signal = AlloyEvent::FrameRendered { frame, fps, refreshes: 1, present_at: at, reference: at, grid: at };
    let _ = self.events.send(signal);
  }
}

/// Empty the app's sandbox, ahead of every test engine: its data folder
/// (the working directory, which stays) and its fetch cache. A test then
/// starts from no stored state, whatever the tests before it wrote; the
/// file is evaluated after this, so what it opens at module level is opened
/// in the emptied sandbox.
pub(crate) fn empty_sandbox(store: &crate::storage::Storage, app_id: &str) {
  for dir in [store.app_dir(app_id).join("data"), store.cache_dir(app_id)] {
    let Ok(entries) = std::fs::read_dir(&dir) else { continue };
    for entry in entries.flatten() {
      let path = entry.path();
      let removed = if path.is_dir() { std::fs::remove_dir_all(&path) } else { std::fs::remove_file(&path) };
      if let Err(e) = removed {
        log::warn!("[srt] test mode: could not remove {} from the sandbox: {e}", path.display());
      }
    }
  }
}
