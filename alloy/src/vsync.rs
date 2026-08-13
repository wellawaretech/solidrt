// Vsync source for frame pacing: a platform backend that arms one display
// vsync callback per request() and answers each with one signal, so the main
// loop can defer frame signals to the display's clock. One-shot re-arm keeps
// the demand-driven contract: no request, no armed callback, no wakeups while
// idle. start() returns None on platforms without a backend; the main loop
// then keeps present-return pacing.
//
// Backends: Android (AChoreographer, below). iOS would slot in here via
// SDL_SetiOSAnimationCallback (CADisplayLink) when iOS support lands. This
// module is the only ndk / ndk-sys consumer in the tree; if SDL ships its own
// choreographer API (libsdl-org/SDL#15013, milestone 3.8.0), reimplement the
// Android backend on that and drop both deps - callers only speak
// request()/try_take().

use std::cell::Cell;
use std::sync::mpsc;
use std::time::Duration;

/// Frame-release policy for the main loop (AlloyCommand::SetFramePacing).
/// VsyncLocked defers each present's frame signal to the display vsync:
/// production phase-locks to the clock the platform batches input on, and a
/// built frame waits in the buffer queue as briefly as possible - best
/// input-to-glass latency, but the release chain's jitter periodically beats
/// the queue's slack and drops a latch. SwapPaced emits the frame signal at
/// present-return: the queue fills and the blocking swap paces production -
/// metronomic presentation at ~1-2 frames more input latency. Policy is
/// chosen above alloy (from the input-modality facts); on platforms without
/// a vsync backend both behave the same.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FramePacing {
  VsyncLocked,
  SwapPaced,
}

pub struct VsyncSource {
  req_tx: mpsc::Sender<(u64, Duration)>,
  signal_rx: mpsc::Receiver<u64>,
  // Generation of the latest request. Each signal carries the generation of
  // the request it answers; try_take discards signals from superseded
  // requests, so a late signal (its present already released by the caller's
  // fallback) can never release a future present early.
  generation: Cell<u64>,
}

impl VsyncSource {
  /// Start the platform vsync backend, or None if this platform has none.
  /// `wake` runs on the backend thread after each signal is queued, so a main
  /// loop blocked on the SDL event queue notices (same pattern as the raster
  /// thread's post-present wake).
  #[allow(unused_variables)]
  pub fn start(wake: impl Fn() + Send + 'static) -> Option<VsyncSource> {
    #[cfg(target_os = "android")]
    {
      let (req_tx, req_rx) = mpsc::channel::<(u64, Duration)>();
      let (signal_tx, signal_rx) = mpsc::channel::<u64>();
      std::thread::Builder::new()
        .name("srt-vsync".into())
        .spawn(move || android::run(req_rx, signal_tx, wake))
        .expect("Failed to spawn vsync thread");
      Some(VsyncSource { req_tx, signal_rx, generation: Cell::new(0) })
    }
    #[cfg(not(target_os = "android"))]
    None
  }

  /// Arm one frame callback; the matching signal arrives via `try_take` after
  /// `wake`, `delay` after the vsync fires. The delay exists because the
  /// platform delivers the vsync's input batch on its own thread in parallel
  /// with the callback: a signal emitted at the vsync itself races that input
  /// and loses often (measured: 37-44 frames drawn per 60 moves), wasting the
  /// frame on not-yet-dirty state. Requests sent while one is being served
  /// coalesce into it. Each request supersedes the previous one: an
  /// unanswered earlier request's signal will be discarded by `try_take`.
  pub fn request(&self, delay: Duration) {
    self.generation.set(self.generation.get() + 1);
    self.req_tx.send((self.generation.get(), delay)).ok();
  }

  /// Drain queued vsync signals; true if the latest request's own signal was
  /// among them. Signals from superseded requests are discarded silently -
  /// their newer request is still outstanding, so a false return leaves the
  /// armed state untouched at the caller.
  pub fn try_take(&self) -> bool {
    let mut took = false;
    while let Ok(g) = self.signal_rx.try_recv() {
      if g == self.generation.get() {
        took = true;
      }
    }
    took
  }
}

/// Rolling estimate of the frame pipeline cost - frame-signal emission to
/// present-return, the full JS build + raster + present chain - driving how
/// late after vsync the frame signal can fire. Later is better (the frame
/// consumes fresher input and the buffer waits less for the compositor
/// latch), bounded by the pipeline still having to finish before the next
/// latch.
pub struct PacingBudget {
  samples: [f32; Self::WINDOW],
  idx: usize,
  filled: usize,
  // Last armed delay; delay() slews toward its target from here.
  delay_ms: Option<f32>,
}

impl PacingBudget {
  const WINDOW: usize = 32;
  // Headroom added on top of the observed worst pipeline cost.
  const MARGIN_MS: f32 = 2.0;
  // Max movement of the armed delay per request (one per frame). The target
  // steps whenever the worst-of-WINDOW picks up or retires an outlier; each
  // step shifts the release phase of the whole production chain, and the
  // buffer queue downstream has to absorb the swing. Slewing turns the step
  // into a drift the queue absorbs frame by frame.
  const SLEW_MS: f32 = 0.5;

  pub fn new() -> PacingBudget {
    PacingBudget { samples: [0.0; Self::WINDOW], idx: 0, filled: 0, delay_ms: None }
  }

  /// Record one emission-to-present duration. Samples beyond 1.5 periods are
  /// slipped frames (the chain waited out an extra vsync), not steady-state
  /// cost; folding them in would drag the start earlier for a whole window.
  pub fn record(&mut self, ms: f32, period: std::time::Duration) {
    if ms > period.as_secs_f32() * 1500.0 {
      return;
    }
    self.samples[self.idx] = ms;
    self.idx = (self.idx + 1) % Self::WINDOW;
    self.filled = (self.filled + 1).min(Self::WINDOW);
  }

  /// Signal delay after vsync: as late as the estimated budget allows, but
  /// never before the input-arrival floor and never past a margin before the
  /// next vsync. The floor exists because the platform needs several ms to
  /// route the vsync's input batch into the SDL queue, and a signal ahead of
  /// its input wastes the frame. 8ms, scaled down to 60% of the period for
  /// high-refresh displays. Measured on-device: 10ms bought nothing over 8 -
  /// the ~5 skipped frames/s that remain are the platform pairing deliveries
  /// across vsync boundaries (input-resampling territory, not a delay
  /// problem) - while eating build margin. An empty window (chain start)
  /// uses the floor. Call once per armed request: the returned delay moves at
  /// most SLEW_MS from the previous call's (see SLEW_MS).
  pub fn delay(&mut self, period: std::time::Duration) -> std::time::Duration {
    let period_ms = period.as_secs_f32() * 1000.0;
    let floor = (period_ms * 0.6).min(8.0);
    let budget = match self.filled {
      0 => period_ms / 2.0,
      n => {
        let worst = self.samples[..n].iter().fold(0.0_f32, |a, &b| a.max(b));
        worst + Self::MARGIN_MS
      }
    };
    let target = (period_ms - budget).clamp(floor, period_ms - Self::MARGIN_MS);
    let delay_ms = match self.delay_ms {
      None => target,
      Some(cur) => cur + (target - cur).clamp(-Self::SLEW_MS, Self::SLEW_MS),
    };
    self.delay_ms = Some(delay_ms);
    std::time::Duration::from_secs_f32(delay_ms / 1000.0)
  }

  /// Last armed delay in ms, for diagnostics; does not advance the slew.
  pub fn current_ms(&self) -> f32 {
    self.delay_ms.unwrap_or(0.0)
  }
}

#[cfg(target_os = "android")]
mod android {
  use std::cell::Cell;
  use std::sync::mpsc;

  pub fn run(req_rx: mpsc::Receiver<(u64, std::time::Duration)>, signal_tx: mpsc::Sender<u64>, wake: impl Fn()) {
    // The choreographer instance is per-thread and requires that thread to
    // own a looper; callbacks are dispatched from this thread's poll calls.
    let looper = ndk::looper::ThreadLooper::prepare();
    let choreographer = unsafe { ndk_sys::AChoreographer_getInstance() };
    if choreographer.is_null() {
      log::warn!("[alloy] no choreographer on this device; vsync pacing disabled");
    }
    let fired = Cell::new(false);
    while let Ok((mut generation, mut delay)) = req_rx.recv() {
      // A burst of requests collapses into one callback answering the latest
      // generation: the consumer flushes everything pending on each signal.
      while let Ok((g, d)) = req_rx.try_recv() {
        generation = g;
        delay = d;
      }
      if !choreographer.is_null() {
        fired.set(false);
        unsafe {
          ndk_sys::AChoreographer_postFrameCallback(
            choreographer,
            Some(frame_callback),
            &fired as *const Cell<bool> as *mut core::ffi::c_void,
          );
        }
        while !fired.get() {
          if looper.poll_once().is_err() {
            // A broken looper cannot dispatch the callback; answer now
            // instead of stalling frame production (= present-return pacing).
            log::warn!("[alloy] vsync looper poll failed; answering without vsync");
            break;
          }
        }
        if fired.get() && !delay.is_zero() {
          std::thread::sleep(delay);
        }
      }
      if signal_tx.send(generation).is_err() {
        return;
      }
      wake();
    }
  }

  // Runs on the vsync thread, inside looper.poll_once(). The frame timestamp
  // is deliberately unused for now: the frame signal is emitted (and timed)
  // sub-millisecond later on the main loop, and the paced clock models time
  // as frame counts, not wall-clock samples.
  unsafe extern "C" fn frame_callback(_frame_time_nanos: core::ffi::c_long, data: *mut core::ffi::c_void) {
    (*(data as *const Cell<bool>)).set(true);
  }
}
