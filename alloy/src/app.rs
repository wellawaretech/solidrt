use crate::renderer::ISize;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use crate::backend::{DisplayContext, FrameOutput, GlBinding};
use crate::context::Context;
use crate::event::{
  current_input_devices_event, current_orientation_event, current_resize_event, current_system_theme_event,
  AlloyCommand, AlloyEvent,
};
use crate::gl;
use crate::liveness::SurfaceLiveness;
use crate::mode::Mode;
use crate::raster::RasterCmd;
use crate::stepped::run_stepped_loop;
use ore::event::PlatformEvent;
use ore::lifecycle::Lifecycle;
use ore::platform::Platform;
use ore::window::{Backing, Display, Window, WindowConfig};

pub struct App {
  platform: Platform,
  window: Window,
  display: DisplayContext,
  mode: Mode,
  // Pointer-move resampler the run loop feeds (see resample.rs): moves are
  // consumed into it at the pump and never travel as events. The embedder
  // grabs a handle before run() to sample per frame slot.
  resampler: crate::resample::SharedResampler,
  // Dev-tool mute for the user's own input (an agent measuring or testing;
  // see lattice's dev connection): the run loop drops muted input before the resampler and
  // the event channel. Arc'd for the same reason as the resampler: the
  // embedder grabs a handle before run() and flips it from another thread.
  user_input_muted: Arc<AtomicBool>,
}

/// Open the run's window on `platform` (initialized by the host on this, the
/// main thread) and the display context the raster thread draws through.
pub fn setup(platform: Platform, title: &str, size: ISize, mode: Mode) -> App {
  let (width, height) = (size.width as u32, size.height as u32);

  if mode.is_headless() {
    // The process clock is the host's virtual frame time from before any
    // app code runs (see clock.rs), so a producer opened at mount already
    // schedules on it.
    crate::clock::set_virtual_ns(0);
  }

  // A panic in the `run` closure (the sol-ui thread) would otherwise strand
  // the window: main keeps pumping events over a black surface until it is
  // killed. Let the default hook print its report, then exit, so a failed
  // assertion in a probe takes the process down at once. An embedder's own
  // hook installed before this one is chained (it runs first). The headless
  // mode keeps the default: its host reports an incomplete run itself.
  if !mode.is_headless() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
      default_hook(info);
      std::process::exit(1);
    }));
  }

  let resampler = crate::resample::SharedResampler::new();
  let user_input_muted = Arc::new(AtomicBool::new(false));
  // The window backbuffer's multisampling, where the platform honors it
  // (ore decides where: see WindowConfig::backbuffer_samples).
  let config = |display| WindowConfig { title, width, height, display, backbuffer_samples: gl::MSAA_SAMPLES as u32 };
  let opened = if mode.is_headless() {
    open_headless(&platform, config, (width, height))
  } else {
    open_gl(&platform, config(Display::Interactive))
  };
  let (window, display) = opened.expect("Failed to set up video");
  crate::set_video_driver(window.video_driver());
  App { platform, window, display, mode, resampler, user_input_muted }
}

// A GL window and the display context over its GL surface.
fn open_gl(platform: &Platform, config: WindowConfig<'_>) -> Result<(Window, DisplayContext), String> {
  let mut window = platform.open_window(config)?;
  let surface = window.create_gl_surface()?;
  let display = DisplayContext::new_gl(surface, window.size_in_pixels());
  Ok((window, display))
}

// The headless mode wants no display at all: ore's offscreen window (see
// ore::window::Display::Offscreen for its chain) backs the context with an
// EGL pbuffer, so `sol render` and `sol test` run in CI, over SSH, on any
// headless box. Where that window comes without GL (the dummy driver, on a
// GL stack without EGL device enumeration: ANGLE), the same pbuffer is
// built outside SDL (egl_headless.rs); and where that fails too, the run
// falls back to a hidden window on the real display. Whichever surface
// results only keeps the context current: a headless frame is drawn into a
// framebuffer the raster thread owns at the headless window's size
// (raster::RasterState::window_framebuffer), so the surface's own size and
// content never matter after setup.
fn open_headless<'a>(
  platform: &Platform,
  config: impl Fn(Display) -> WindowConfig<'a>,
  (width, height): (u32, u32),
) -> Result<(Window, DisplayContext), String> {
  let mut window = platform.open_window(config(Display::Offscreen))?;
  if window.backing() == Backing::Gl {
    let surface = window.create_gl_surface()?;
    let display = DisplayContext::new_gl(surface, window.size_in_pixels());
    return Ok((window, display));
  }
  match DisplayContext::new_egl_pbuffer(width, height) {
    Ok(display) => Ok((window, display)),
    Err(e) => {
      log::warn!("[alloy] headless EGL context unavailable ({e}); falling back to a hidden window");
      // The dummy driver's window goes first: dropping its video subsystem
      // handle lets the next open initialize video on the real display.
      drop(window);
      open_gl(platform, config(Display::Hidden))
    }
  }
}

// Interactive loop only: the stepped loop publishes its window's size itself
// (stepped.rs).
fn apply_main_thread_effects(event: &AlloyEvent, surface_size: &Arc<AtomicU64>) {
  if let AlloyEvent::Resize { size, display_scale, .. } = event {
    let (w, h) = ((size.width as f32 * display_scale) as u32, (size.height as f32 * display_scale) as u32);
    surface_size.store(crate::backend::pack_size(w, h), Ordering::Release);
  }
}

// After a cadence command moved the policy or the controller: a hold
// change reaches the release chain (from the next frame signal), the miss
// accounting and the stats, and any change of the policy in force or the
// hold is reported (AlloyEvent::CadenceHold).
fn cadence_moved(
  before: (crate::cadence::CadenceHold, u32),
  cadence: &crate::cadence::CadenceController,
  release: &mut crate::vsync::FrameRelease,
  refreshes: &mut crate::present::RefreshCounting,
  stats: &crate::raster::RasterStats,
  event_tx: &crate::EventSender,
  period: Duration,
) {
  let (policy, hold) = (cadence.policy(), cadence.hold());
  if policy != before.0 {
    log::info!("[alloy] cadence hold policy: {policy:?}");
  }
  if hold != before.1 {
    release.set_hold(hold, period);
    refreshes.set_hold(hold);
    stats.cadence_hold.store(hold, Ordering::Relaxed);
  }
  if (policy, hold) != before {
    event_tx.send(AlloyEvent::CadenceHold { policy, hold }).ok();
  }
}

// Where ANGLE translates GL (Windows over D3D11, macOS over Metal), a device
// loss marks the whole EGL display lost and every later EGL call fails with
// EGL_CONTEXT_LOST until eglTerminate, which SDL issues when the last GL
// window goes (measured, okf/done/gpu-context-loss.md): recovery recreates
// the window. Adreno's driver is the same on Android: after a KGSL fault a
// new context on the same window is born dead (its creation already fails
// a GPU submission, its extension list reads empty, every pass errors;
// measured on the SM-T500, okf/backlog/splat-adreno-gpu-fault.md), so the
// display is terminated there too; SDL's Android backend recreates its one
// window on the same native surface. On Mesa a loss is per context, and a
// new context on the same window recovers it.
const RECOVERY_RECREATES_WINDOW: bool = cfg!(any(target_os = "windows", target_os = "macos", target_os = "android"));

/// A fresh GL context for the raster thread after a loss, on this window or
/// a recreated one (see RECOVERY_RECREATES_WINDOW): ore destroys the old
/// context (the raster thread released it), creates the new one released
/// for the raster thread to bind and reapplies what was set on the window;
/// here the surface size is republished. Interactive runs only: a headless
/// surface has nothing to recreate.
fn recover_gl(window: &mut Window, surface_size: &Arc<AtomicU64>) -> Result<Box<dyn GlBinding>, String> {
  if window.backing() != Backing::Gl {
    return Err("the headless surface has no GL context to recreate".to_string());
  }
  let surface = window.recreate_gl(RECOVERY_RECREATES_WINDOW)?;
  let (w, h) = window.size_in_pixels();
  surface_size.store(crate::backend::pack_size(w, h), Ordering::Release);
  log::info!(
    "[alloy] GL context recreated{} at {w}x{h}",
    if RECOVERY_RECREATES_WINDOW { " with the window" } else { "" }
  );
  Ok(Box::new(gl::WindowBinding::new(surface)))
}

impl App {
  /// Handle onto the resampler the run loop feeds. Grab a clone before
  /// run(): the UI consumer samples it once per frame signal, and
  /// synthetic-input producers feed it at their send sites (see
  /// resample::SharedResampler for the producer-side rule).
  pub fn resampler(&self) -> crate::resample::SharedResampler {
    self.resampler.clone()
  }

  /// Handle onto the user-input mute the run loop honors (see
  /// `user_input_muted`). Grab a clone before run() consumes the App.
  pub fn user_input_mute(&self) -> Arc<AtomicBool> {
    self.user_input_muted.clone()
  }

  /// Run the platform loop until the app winds down: the interactive loop
  /// until the window closes, the stepped loop until the embedder has
  /// dropped its command sender. Alloy itself never exits the process here.
  pub fn run(
    self,
    dl_producer: impl FnOnce(Arc<Context>, mpsc::Sender<AlloyCommand>, mpsc::Receiver<crate::Arrival>) + Send + 'static,
  ) {
    let App { mut platform, mut window, display, mode, resampler, user_input_muted } = self;
    let surface_size = display.surface_size_handle();

    let (tx, rx) = mpsc::channel::<FrameOutput>();
    let (event_tx, event_rx) = mpsc::channel::<crate::Arrival>();
    let event_tx = crate::EventSender::new(event_tx);
    let (cmd_tx, cmd_rx) = mpsc::channel::<AlloyCommand>();
    // Live counters shared with the raster thread and the Context (see
    // raster::RasterStats). The loop below reads the queue depth to gate the
    // idle Tick and increments idle_ticks per emitted Tick.
    let stats = Arc::new(crate::raster::RasterStats::new());
    let waker = platform.waker();
    // Frame wakeup for the interactive loop below: it sleeps on the SDL event
    // queue, so a presented frame must push an event to be noticed before the
    // wait's timeout. The stepped loop presents nothing.
    let wake: Option<Arc<dyn Fn() + Send + Sync>> = if mode.is_headless() {
      None
    } else {
      let waker = waker.clone();
      Some(Arc::new(move || waker.wake()))
    };
    let raster =
      display.run_context(move |ctx| dl_producer(ctx, cmd_tx, event_rx), tx, wake, mode.frame_sink(), stats.clone());

    // Surface-liveness policy (rebind + repaint across the surface
    // lifecycle; see liveness.rs). The latch half arrives later via
    // AlloyCommand::SetFrameRequestLatch.
    let mut liveness = SurfaceLiveness::new();

    if let Mode::Stepped(stepped) = mode {
      run_stepped_loop(window, cmd_rx, event_tx, stepped, surface_size);
      // Nothing may be drawing when the window drops: dropping it unloads
      // the EGL library, and the embedder exits the process right after,
      // which would pull the driver out from under a draw still encoding.
      raster.drain();
      return;
    }

    let initial = current_resize_event(&window);
    apply_main_thread_effects(&initial, &surface_size);
    if liveness.on_event(&initial, Instant::now()) {
      raster.send(RasterCmd::RebindWindowSurface).ok();
    }
    event_tx.send(initial).ok();

    // The lifecycle transitions, forwarded the moment the platform pushes
    // them, ahead of the pump (see ore::lifecycle::Lifecycle for why and on
    // which thread).
    let lifecycle_event_tx = event_tx.clone();
    platform.on_lifecycle(move |lifecycle| match lifecycle {
      Lifecycle::WillBackground(hold) => {
        // Before the suspend goes out: frames already queued on the raster
        // thread must stop touching the window from here on (see
        // WINDOW_BACKGROUNDED in lib.rs).
        crate::set_window_backgrounded(true);
        lifecycle_event_tx.send(AlloyEvent::Suspend { hold }).ok();
      }
      Lifecycle::DidBackground => {
        if cfg!(target_os = "android") {
          crate::audio::device_backgrounded();
        }
        lifecycle_event_tx.send(AlloyEvent::Visibility { visible: false }).ok();
      }
      // SDL has just restarted its audio devices (Android_OnResume runs
      // the restart right before it sends this event); reopen the
      // playback device if the restart left it dead (see
      // audio::device_foregrounded).
      Lifecycle::DidForeground => {
        if cfg!(target_os = "android") {
          crate::audio::device_foregrounded();
        }
      }
    });

    // Where a platform vsync backend exists (Android today), FrameRendered is
    // deferred to the display's vsync (see vsync.rs) so frame production
    // phase-locks to the clock the platform batches input on, and a built
    // frame's buffer sits in the queue as briefly as possible. The vsync
    // thread wakes the blocked event wait exactly like the raster thread does
    // after a present. None = present-return pacing, unchanged.
    let vsync = {
      let waker = waker.clone();
      crate::vsync::VsyncSource::start(move || waker.wake())
    };
    // The vsync frame-release policy (deferred presents, the one outstanding
    // request, the fallback deadline, the signal-delay budget), extracted as
    // a pure state machine so its invariants unit-test without a display;
    // this loop performs the effects its decisions name. See
    // vsync::FrameRelease.
    let mut release = crate::vsync::FrameRelease::new(vsync.is_some(), Instant::now());

    // Open the gamepads before the first wait: None when SDL has no gamepad
    // support on this platform; pads already plugged in surface through the
    // Added events SDL emits on subsystem init, which that wait drains.
    platform.gamepads();
    // The display's refresh rate as last reported: ore's watch reports
    // changes as DisplayRefreshRate events, and every report is published
    // for out-of-loop readers (crate::refresh_rate).
    let mut refresh_rate = window.refresh_rate().unwrap_or(ore::display::DEFAULT_REFRESH_HZ);
    crate::set_refresh_rate(refresh_rate);
    let mut frame: u64 = 0;

    // The refresh count on every frame signal (see present.rs): reference
    // instants become ms on this origin, the counter turns them into whole
    // display refreshes, and the ledger keeps the last few seconds of counts.
    // The refresh rate is its own event (delivered on init and on change);
    // what a count means for the app timeline is the embedder's policy.
    let start_time = Instant::now();
    let ms_since_start = |at: Instant| at.saturating_duration_since(start_time).as_secs_f64() * 1000.0;
    // Where a count places its signal (Counted::grid_ms), as an instant.
    let grid_instant =
      |counted: crate::present::Counted| start_time + Duration::from_secs_f64(counted.grid_ms.max(0.0) / 1000.0);
    let mut refreshes = crate::present::RefreshCounting::new();
    // The cadence hold (see cadence.rs): the controller reads each present's
    // work time and sets the hold FrameRelease enforces; the policy in force
    // comes from the embedder (SetCadenceHold), Off until it arrives.
    let mut cadence = crate::cadence::CadenceController::new();
    // The UI thread's busy flag (SetUiBusyFlag), read by the idle-tick gate;
    // None until the embedder registers it, and the gate ignores it then.
    let mut ui_busy: Option<Arc<AtomicBool>> = None;
    // Instant of the last frame signal emission (FrameRendered or Tick),
    // the start of the frame it triggers; with the swap-call instant the
    // raster thread reports, the CPU side of that frame's work time.
    let mut last_emission = Instant::now();
    // The presents' CPU and GPU terms summed over the second, and their
    // count, for the 1/s cadence line (the work mean split in two).
    let mut work_split: (f32, f32, u32) = (0.0, 0.0, 0);
    // Demand flag and work time of the presents whose frame signal is
    // deferred (FrameRelease keeps only their number), in present order, so
    // a release hands each signal its own present's facts.
    let mut deferred: std::collections::VecDeque<(bool, (f32, f32))> = std::collections::VecDeque::new();
    // Count one frame signal, record it, and add the misses it closed to the
    // shared counter the stats read.
    let count_signal = |refreshes: &mut crate::present::RefreshCounting,
                        reference: Instant,
                        frame: u64,
                        presented: bool,
                        demanded: bool,
                        work_ms: Option<f32>|
     -> crate::present::Counted {
      let counted = refreshes.count(ms_since_start(reference), frame, presented, demanded, work_ms);
      if counted.missed > 0 {
        stats.missed_presents.fetch_add(counted.missed as u64, Ordering::Relaxed);
      }
      counted
    };
    // After a counted present: feed the cadence controller the interval the
    // display showed the previous frame for and this frame's work time, and
    // apply a hold change to the release chain (which enforces it from the
    // next frame signal), the miss accounting (whose open interval follows
    // it) and the stats.
    let learn = |cadence: &mut crate::cadence::CadenceController,
                 release: &mut crate::vsync::FrameRelease,
                 refreshes: &mut crate::present::RefreshCounting,
                 counted: crate::present::Counted,
                 (cpu_ms, gpu_ms): (f32, f32),
                 at: Instant,
                 period: Duration| {
      if counted.interval == 0 {
        return;
      }
      let period_ms = period.as_secs_f64() * 1000.0;
      let change =
        cadence.on_present(counted.interval, cpu_ms, gpu_ms, release.pipelined(), ms_since_start(at), period_ms);
      if let Some(change) = change {
        release.set_hold(change.to, period);
        refreshes.set_hold(change.to);
        stats.cadence_hold.store(change.to, Ordering::Relaxed);
        event_tx.send(AlloyEvent::CadenceHold { policy: cadence.policy(), hold: change.to }).ok();
        log::info!(
          "[alloy] cadence hold: {} -> {} refreshes (interval {}, work {:.1}ms) [{}]",
          change.from,
          change.to,
          change.need,
          change.work_ms,
          cadence.diagnostics(ms_since_start(at))
        );
      }
    };
    let mut fps_last_second = Instant::now();
    let mut fps_frame_count: u32 = 0;
    let mut fps: u32 = 0;
    // Pointer moves received from SDL in the current second, logged 1/s while
    // input flows: the arrival rate shows whether the platform delivers moves
    // at input-device rate or batched to vsync (drag latency diagnostics).
    let mut pointer_moves: u32 = 0;

    // Instant of the last frame signal (FrameRendered or Tick). When the UI
    // thread submits nothing for a full refresh period, an idle Tick keeps its
    // per-frame logic running while the GPU stays idle; presents reset the
    // deadline so Ticks only fire when no frames are being produced.
    let mut last_frame_signal = Instant::now();

    // The loop exists to feed the embedder's producer through event_tx; a
    // dropped receiver means the producer has finished and nothing will ever
    // consume events or build frames again, so a failed send below winds the
    // loop down (returning ends the process: main_tx drops, the raster thread
    // follows). Without this the main/raster pair holds itself up forever -
    // each waits on the other's channel - and window close or SIGTERM (which
    // SDL translates into a Quit event) lands in the dead channel and wedges
    // the process until SIGKILL.
    'run: loop {
      let tick_period = Duration::from_secs_f64(1.0 / refresh_rate.max(1.0) as f64);
      refreshes.set_hz(refresh_rate);
      // Sleep on the SDL event queue (ore's pump) until the next idle-tick
      // deadline: input wakes it directly and the raster and vsync threads
      // push ore's Waker after each present and vsync (see Context::submit),
      // so nothing needs polling. The woken-for events come back with the
      // rest of the drained queue and are handled below; a wake translates
      // to nothing, its work is the rx drain.
      let remaining = match release.wait_deadline() {
        // A present awaits its vsync signal: Ticks are suppressed and the
        // wake comes from the vsync thread, so wait until the fallback
        // deadline instead (neither spinning nor sleeping through it).
        Some(deadline) => deadline.saturating_duration_since(Instant::now()),
        None => tick_period.saturating_sub(last_frame_signal.elapsed()),
      };
      let events = platform.wait(remaining, &window);

      // Drain the raster thread's frame notifications. Drawing and presenting
      // have already happened over there (it owns the process's single GL
      // context); each notification is one on-screen frame, so roll the
      // counters and emit its FrameRendered.
      let mut disconnected = false;
      loop {
        match rx.try_recv() {
          Ok(FrameOutput::Presented { at, ready_at, gpu_micros, demanded, frame: built, swapped }) => {
            if let Some(generation) = liveness.on_present(built, swapped) {
              ore::redraw::answer(generation);
            }
            fps_frame_count += 1;
            // The frame's work time (see cadence.rs): its signal's emission
            // to the swap call on the raster thread, plus the GPU time of
            // the last retired window draw (a frame or two behind; the
            // controller windows it). Independent of any hold, which only
            // delays the next emission.
            let cpu_ms = ready_at.saturating_duration_since(last_emission).as_secs_f32() * 1000.0;
            let gpu_ms = gpu_micros.map_or(0.0, |us| us as f32 / 1000.0);
            work_split = (work_split.0 + cpu_ms, work_split.1 + gpu_ms, work_split.2 + 1);
            let work = (cpu_ms, gpu_ms);
            let work_ms = cpu_ms + gpu_ms;
            match release.on_present(at, tick_period) {
              // No vsync backend, or SwapPaced policy: the frame signal
              // follows the present directly and the blocking swap paces,
              // unless a cadence hold defers it to the slot end. Under
              // VsyncLocked, a present whose vsync signal was banked
              // (arrived while the frame was still in the swap) releases
              // here too, referenced at its banked vsync; otherwise the
              // swap's return is the reference the refreshes are counted
              // from.
              crate::vsync::Release::Emit { arm, vsync: banked } => {
                let reference = banked.unwrap_or(at);
                let counted = count_signal(&mut refreshes, reference, frame, true, demanded, Some(work_ms));
                learn(&mut cadence, &mut release, &mut refreshes, counted, work, at, tick_period);
                // The next frame presents at the end of the slot the hold
                // opens from this reference (one period without a hold).
                let present_at = reference + tick_period * cadence.hold();
                let grid = grid_instant(counted);
                event_tx
                  .send(AlloyEvent::FrameRendered {
                    frame,
                    fps,
                    refreshes: counted.refreshes,
                    present_at,
                    reference,
                    grid,
                  })
                  .ok();
                frame += 1;
                last_frame_signal = Instant::now();
                last_emission = last_frame_signal;
                liveness.on_frame_signal(last_frame_signal, frame);
                if let (Some(v), Some(delay)) = (&vsync, arm) {
                  v.request(delay);
                }
              }
              crate::vsync::Release::Deferred { arm } => {
                deferred.push_back((demanded, work));
                if let (Some(v), Some(delay)) = (&vsync, arm) {
                  v.request(delay);
                }
              }
            }
          }
          // The raster thread lost its context and waits for a fresh one
          // (okf/done/gpu-context-loss.md). Answered right here: nothing
          // else runs until it draws again. A None answer ends the process
          // on the raster side, as a confirmed loss always did.
          Ok(FrameOutput::ContextLost { detail, reply }) => {
            let recovered = recover_gl(&mut window, &surface_size);
            let binding = match recovered {
              Ok(binding) => Some(binding),
              Err(e) => {
                log::error!("[alloy] GPU context recovery failed: {e} (after: {detail})");
                None
              }
            };
            let recovered = binding.is_some();
            reply.send(binding).ok();
            if recovered {
              // The window the raster thread draws into now may be a new
              // one: ore re-applied what was set on the old (the icon, the
              // text input session, the pointer lock); let the liveness
              // policy repaint it like any surface that came back.
              let e = current_resize_event(&window);
              apply_main_thread_effects(&e, &surface_size);
              if liveness.on_event(&e, Instant::now()) {
                raster.send(RasterCmd::RebindWindowSurface).ok();
              }
              event_tx.send(e).ok();
            }
          }
          Ok(FrameOutput::ContextRestored { handles, lost, guilty, losses }) => {
            event_tx.send(AlloyEvent::ContextRestored { handles, lost, guilty, losses }).ok();
          }
          Err(mpsc::TryRecvError::Empty) => break,
          Err(mpsc::TryRecvError::Disconnected) => {
            disconnected = true;
            break;
          }
        }
      }
      if disconnected {
        log::info!("[alloy] raster thread gone; ending the run loop");
        break;
      }

      // Rolled every iteration (not only on present) so fps decays to zero
      // when no frames are produced.
      if fps_last_second.elapsed().as_secs_f32() >= 1.0 {
        fps = fps_frame_count;
        fps_frame_count = 0;
        fps_last_second = Instant::now();
        if pointer_moves > 0 {
          log::debug!("[alloy] input: {pointer_moves} pointer moves/s");
          pointer_moves = 0;
        }
        if vsync.is_some() && fps > 0 {
          let delay_ms = release.current_delay_ms();
          log::debug!("[alloy] pacing: signal delay {delay_ms:.1}ms");
        }
        // The refresh counts of the last second (see CountTally).
        let t = refreshes.take_tally();
        if t.signals > 0 {
          let work_mean = if t.work_n > 0 { t.work_sum_ms / t.work_n as f32 } else { 0.0 };
          stats.work_mean_micros.store((work_mean * 1000.0) as u32, Ordering::Relaxed);
          stats.work_max_micros.store((t.work_max_ms * 1000.0) as u32, Ordering::Relaxed);
          log::debug!(
            "[alloy] refresh count: {} signals, {} refreshes, zero {}, multi {}, max {}, hold {}, work mean {:.1}ms max {:.1}ms",
            t.signals,
            t.refreshes,
            t.zero,
            t.multi,
            t.max,
            cadence.hold(),
            work_mean,
            t.work_max_ms
          );
          let (cpu_sum, gpu_sum, n) = std::mem::take(&mut work_split);
          let n = n.max(1) as f32;
          log::debug!(
            "[alloy] cadence: cpu mean {:.1}ms, gpu mean {:.1}ms, {}",
            cpu_sum / n,
            gpu_sum / n,
            cadence.diagnostics(ms_since_start(Instant::now()))
          );
        }
      }

      // No idle Tick while a present awaits its vsync signal: the real frame
      // signal is at most a refresh period away (fallback included). And no
      // idle Tick while raster commands are queued or executing: a backlogged
      // raster thread also shows pending_presents == 0 (nothing has come back
      // to present), and ticking through that backlog feeds it more per-frame
      // work than it retires - frame time diverges without bound (see
      // okf/backlog/idle-tick-gpu-backlog-runaway.md). And no idle Tick
      // while the UI thread is executing a batch: the frame it is building
      // will present and bring its own signal, so a Tick now is a second
      // frame signal for one present - a JS-bound app under vsync pacing
      // ran two frames per present because the fallback deadline gave the
      // in-flight window up mid-build (see AlloyCommand::SetUiBusyFlag).
      // Idle means idle: no presents in flight, an empty raster queue AND a
      // free UI thread. The deadline resets on suppression too, or
      // `remaining` above stays zero and the loop spins through the
      // backlog instead of sleeping; ticks resume within one refresh
      // period of the queue draining.
      if release.idle() && last_frame_signal.elapsed() >= tick_period {
        let ui_free = !ui_busy.as_ref().is_some_and(|b| b.load(Ordering::Acquire));
        if stats.queue_depth.load(Ordering::Acquire) == 0 && ui_free {
          // The Tick is the loop's heartbeat, so an idle app with a finished
          // producer winds down within one tick period (see 'run).
          let now = Instant::now();
          let counted = count_signal(&mut refreshes, now, frame, false, false, None);
          let present_at = now + tick_period;
          let signal = AlloyEvent::Tick {
            frame,
            fps,
            refreshes: counted.refreshes,
            present_at,
            reference: now,
            grid: grid_instant(counted),
          };
          if event_tx.send(signal).is_err() {
            log::info!("[alloy] event receiver gone at the idle tick; ending the run loop");
            break 'run;
          }
          stats.idle_ticks.fetch_add(1, Ordering::Relaxed);
          last_emission = Instant::now();
          liveness.on_frame_signal(last_emission, frame);
        }
        last_frame_signal = Instant::now();
      }
      // The dev-tool user-input mute (user_input_muted), read once per
      // iteration: the translated-event path below drops muted input
      // (is_muted_input); the level-read pads apply it themselves
      // (Gamepads::set_muted).
      let muted = user_input_muted.load(Ordering::Relaxed);
      if let Some(g) = platform.gamepads() {
        g.set_muted(muted);
      }
      // A muted client must not hold the user's mouse captive: their input is
      // dropped below anyway, so the capture buys nothing, and the Escape an
      // app releases the lock on is itself a muted keydown - without this the
      // mouse is stuck until the mute lifts. The SDL read is the transition
      // check; the app sees a plain unlock and can lock again once unmuted.
      if muted && window.pointer_locked() {
        window.set_pointer_lock(false);
        event_tx.send(AlloyEvent::PointerLock { locked: false }).ok();
      }
      liveness.begin_pump();
      {
        // One translated input or window event on its way out: `at` is when
        // it happened (the platform's event time, a touch sample's time).
        // False when the embedder is gone.
        let mut deliver = |e: AlloyEvent, at: Instant| -> bool {
          // A failed send below means the embedder's event receiver is
          // gone (its producer finished); named here, since the loop then
          // winds the process down without another word.
          apply_main_thread_effects(&e, &surface_size);
          if liveness.on_event(&e, Instant::now()) {
            raster.send(RasterCmd::RebindWindowSurface).ok();
          }
          // The mute: while the dev tools hold it, the user's own input ends
          // here, ahead of the resampler and the channel, so the app sees
          // only the synthetic input injected past this pump. Releases still
          // pass (is_muted_input), so a button or key held when the mute
          // began cannot stay stuck; window and display facts (resize,
          // visibility, quit) are not input.
          if muted && crate::event::is_muted_input(&e) {
            return true;
          }
          if matches!(e, AlloyEvent::PointerMove { .. }) {
            pointer_moves += 1;
          }
          // Producer-side resampler feed (see resample.rs): moves are
          // consumed here - the UI side dispatches one position per pointer
          // per frame - while a down seeds the history and an up flushes it
          // before the event travels.
          let delivered = resampler.feed(e, at, |e, at| event_tx.send_at(e, at)).is_ok();
          if !delivered {
            log::info!("[alloy] event receiver gone; ending the run loop");
          }
          delivered
        };
        for (e, at) in events {
          if let PlatformEvent::DisplayRefreshRate { hz } = &e {
            refresh_rate = *hz;
            crate::set_refresh_rate(*hz);
          }
          if !deliver(AlloyEvent::from(e), at) {
            break 'run;
          }
        }
      }
      // A held redraw (see ore's redraw.rs), taken after this iteration's events so
      // the rebinds they caused are queued ahead of the frame that answers.
      if let Some(generation) = ore::redraw::take() {
        liveness.on_redraw_requested(generation);
      }
      // Flush presents deferred to vsync or to a cadence-hold slot end -
      // after the SDL event drain, so the input delivered at the same vsync
      // is already in the event channel and the UI batch runs it before
      // this frame signal (a signal processed ahead of its vsync's input
      // finds nothing dirty and wastes the frame). One signal releases all
      // pending (at most one in practice: the UI thread builds the next
      // frame only after this emission). try_take drains
      // superseded-generation signals internally, so a late one, arriving
      // after its present was released by the fallback, cannot release a
      // future present early. The release names the reference the released
      // signals' refreshes are counted from (see Wake::Release).
      {
        let woke = Instant::now();
        let signal = vsync.as_ref().and_then(|v| v.try_take());
        match release.on_wake(woke, tick_period, signal) {
          crate::vsync::Wake::Idle => {}
          // The signal beat the in-flight frame's present (the swap is
          // still blocking) and releases it on return, or fell inside the
          // held slot and is skipped: keep the next vsync armed meanwhile.
          crate::vsync::Wake::Banked { arm } | crate::vsync::Wake::Held { arm } => {
            if let (Some(v), Some(delay)) = (&vsync, arm) {
              v.request(delay);
            }
          }
          crate::vsync::Wake::Release { emit, timed_out, arm, reference } => {
            if timed_out {
              // Debug, not warn: a GPU-saturated device (Android TV) misses
              // vsyncs in steady state, one line per missed frame.
              // SOLIDRT_LOG=debug surfaces them when diagnosing the vsync source
              // itself.
              log::debug!("[alloy] vsync signal missed; emitting frame signal after timeout");
            }
            // Several presents released at once share one reference: the
            // first takes the refreshes, the rest count as zero.
            for _ in 0..emit {
              let (demanded, work) = deferred.pop_front().map_or((true, None), |(d, w)| (d, Some(w)));
              let counted = count_signal(&mut refreshes, reference, frame, true, demanded, work.map(|(c, g)| c + g));
              if let Some(work) = work {
                learn(&mut cadence, &mut release, &mut refreshes, counted, work, reference, tick_period);
              }
              let present_at = reference + tick_period * cadence.hold();
              let grid = grid_instant(counted);
              event_tx
                .send(AlloyEvent::FrameRendered {
                  frame,
                  fps,
                  refreshes: counted.refreshes,
                  present_at,
                  reference,
                  grid,
                })
                .ok();
              frame += 1;
            }
            last_frame_signal = Instant::now();
            last_emission = last_frame_signal;
            liveness.on_frame_signal(last_frame_signal, frame);
            if let (Some(v), Some(delay)) = (&vsync, arm) {
              v.request(delay);
            }
          }
        }
      }

      // At most one gamepad snapshot per iteration, however many pad events
      // were drained above. The back-button edge follows the snapshot so an
      // app that watches pads sees the final state before the back intent.
      if let Some(g) = platform.gamepads() {
        if let Some(pads) = g.take_snapshot_if_dirty() {
          event_tx.send(AlloyEvent::Gamepads { pads }).ok();
        }
        if g.take_back_edge() {
          event_tx.send(AlloyEvent::Back).ok();
        }
      }
      while let Ok(cmd) = cmd_rx.try_recv() {
        match cmd {
          AlloyCommand::EmitInitEvents => {
            let e = current_resize_event(&window);
            apply_main_thread_effects(&e, &surface_size);
            if liveness.on_event(&e, Instant::now()) {
              raster.send(RasterCmd::RebindWindowSurface).ok();
            }
            event_tx.send(e).ok();
            event_tx.send(AlloyEvent::DisplayRefreshRate { hz: refresh_rate }).ok();
            event_tx.send(current_system_theme_event()).ok();
            event_tx.send(current_input_devices_event()).ok();
            event_tx.send(current_orientation_event(&window)).ok();
            if let Some(g) = platform.gamepads() {
              event_tx.send(AlloyEvent::Gamepads { pads: g.snapshot() }).ok();
            }
            // Pointer lock survives an engine reload (window state); the new
            // engine must observe it.
            event_tx.send(AlloyEvent::PointerLock { locked: window.pointer_locked() }).ok();
            event_tx.send(AlloyEvent::CadenceHold { policy: cadence.policy(), hold: cadence.hold() }).ok();
          }
          AlloyCommand::SetFrameRequestLatch(latch) => {
            // The raster thread samples the same latch at present time for
            // demand flag on each present notification (see demand_at_present).
            raster.send(RasterCmd::SetDemandLatch { latch: latch.clone() }).ok();
            liveness.set_latch(latch);
          }
          AlloyCommand::SetFramePacing(p) => match release.set_pacing(p) {
            crate::vsync::PacingChange::Unchanged => {}
            crate::vsync::PacingChange::Changed { released } => {
              log::info!("[alloy] frame pacing: {p:?}");
              // The clock reset belongs to an actual emission: a switch that
              // released nothing must not delay the next idle Tick.
              if released > 0 {
                let reference = Instant::now();
                for _ in 0..released {
                  let (demanded, work) = deferred.pop_front().map_or((true, None), |(d, w)| (d, Some(w)));
                  let counted =
                    count_signal(&mut refreshes, reference, frame, true, demanded, work.map(|(c, g)| c + g));
                  if let Some(work) = work {
                    learn(&mut cadence, &mut release, &mut refreshes, counted, work, reference, tick_period);
                  }
                  let present_at = reference + tick_period * cadence.hold();
                  let grid = grid_instant(counted);
                  event_tx
                    .send(AlloyEvent::FrameRendered {
                      frame,
                      fps,
                      refreshes: counted.refreshes,
                      present_at,
                      reference,
                      grid,
                    })
                    .ok();
                  frame += 1;
                }
                last_frame_signal = Instant::now();
                last_emission = last_frame_signal;
                liveness.on_frame_signal(last_frame_signal, frame);
              }
            }
          },
          AlloyCommand::SetUiBusyFlag(flag) => {
            ui_busy = Some(flag);
          }
          AlloyCommand::SetCadenceHold(p) => {
            let before = (cadence.policy(), cadence.hold());
            cadence.set_policy(p);
            cadence_moved(before, &cadence, &mut release, &mut refreshes, &stats, &event_tx, tick_period);
          }
          AlloyCommand::ResetCadenceHold => {
            let before = (cadence.policy(), cadence.hold());
            cadence.reset();
            cadence_moved(before, &cadence, &mut release, &mut refreshes, &stats, &event_tx, tick_period);
          }
          AlloyCommand::SetTitle(t) => {
            if let Err(e) = window.set_title(&t) {
              log::warn!("set_title failed: {e}");
            }
          }
          AlloyCommand::RaiseWindow => {
            if let Err(e) = window.raise() {
              log::warn!("raise_window failed: {e}");
            }
          }
          AlloyCommand::SetIcon { width, height, rgba } => {
            // Debug, not warn: macOS reports failure on every app switch
            // because the platform has no window icons at all.
            if let Err(e) = window.set_icon(width, height, &rgba) {
              log::debug!("set_window_icon failed: {e}");
            }
          }
          AlloyCommand::SetFullscreen(fs) => {
            if let Err(e) = window.set_fullscreen(fs) {
              log::warn!("set_fullscreen failed: {e}");
            }
          }
          AlloyCommand::SetCursor(cursor) => window.set_cursor(cursor),
          AlloyCommand::CreateCursor { id, frames, hot_x, hot_y } => window.register_cursor(id, &frames, hot_x, hot_y),
          AlloyCommand::DropCursor(id) => window.drop_cursor(id),
          // Applied and reported in this iteration: the snapshot above has
          // already run, and a driven session should not wait a tick.
          AlloyCommand::Gamepad(cmd) => match platform.gamepads() {
            Some(g) => match g.apply(cmd) {
              Ok(()) => {
                if let Some(pads) = g.take_snapshot_if_dirty() {
                  event_tx.send(AlloyEvent::Gamepads { pads }).ok();
                }
                if g.take_back_edge() {
                  event_tx.send(AlloyEvent::Back).ok();
                }
              }
              Err(e) => log::warn!("[alloy] synthetic gamepad: {e}"),
            },
            None => log::warn!("[alloy] synthetic gamepad ignored: no gamepad subsystem"),
          },
          AlloyCommand::SetPointerLock(locked) => {
            // Report the applied state, not the request: SDL can refuse
            // (platform without relative mode), and the answer is the fact
            // the reactive accessor reflects.
            let applied = window.set_pointer_lock(locked);
            event_tx.send(AlloyEvent::PointerLock { locked: applied }).ok();
          }
          AlloyCommand::SetTextInputActive(active, options) => {
            // ore feeds SDL's screen keyboard policy the platform's
            // keyboard fact first (see Window::start_text_input).
            if active {
              window.start_text_input(&options);
            } else {
              window.stop_text_input();
            }
          }
          AlloyCommand::Background => {
            if let Err(e) = window.minimize() {
              log::warn!("[alloy] background (minimize) failed: {e}");
            }
          }
          // The interactive window is sized by the user (stepped.rs).
          AlloyCommand::SetHeadlessWindow { .. } => {
            log::warn!("[alloy] a headless window size was requested on an interactive window; ignored");
          }
        }
      }
    }
  }
}
