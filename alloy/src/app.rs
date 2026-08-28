use impellers::ISize;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use crate::backend::{DisplayContext, FrameOutput};
use crate::context::Context;
use crate::event::{
  current_input_devices_event, current_orientation_event, current_resize_event, current_system_theme_event,
  playback_resize_event, translate_event, AlloyCommand, AlloyEvent,
};
use crate::gl;
use crate::liveness::SurfaceLiveness;
use crate::mode::Mode;
use crate::playback::run_playback_loop;
use crate::raster::RasterCmd;

pub struct App {
  sdl_context: sdl3::Sdl,
  window: sdl3::video::Window,
  platform: DisplayContext,
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

pub fn setup(title: &str, size: ISize, mode: Mode) -> App {
  let (width, height) = (size.width as u32, size.height as u32);

  // Keep touch and mouse streams separate. Without this, SDL synthesizes
  // mouse events from touches (and vice versa) using SDL_TOUCH_MOUSEID
  // as `which`, which would arrive on our mouse arm and be misclassified
  // as PointerType::Mouse with a sentinel pointer_id.
  sdl3::hint::set("SDL_TOUCH_MOUSE_EVENTS", "0");
  sdl3::hint::set("SDL_MOUSE_TOUCH_EVENTS", "0");
  // For the playback fallback below, force 1:1 pixel mapping so the hidden
  // window is exactly the requested size in physical pixels regardless of
  // display scale.
  if mode.is_playback() {
    sdl3::hint::set("SDL_VIDEO_WAYLAND_SCALE_TO_DISPLAY", "1");
  }

  let sdl_context = sdl3::init().expect("Failed to initialize SDL3");
  // On Android, hand SDL's JNI env + activity to ndk-context so JNI-using deps
  // (iroh's network monitoring via flux:p2p) can reach the Android context.
  #[cfg(target_os = "android")]
  crate::sdl_utils::init_android_context();

  // Playback wants no display at all: SDL's offscreen video driver backs the
  // window with an EGL pbuffer, so `srt render` runs in CI, over SSH, on any
  // headless box - and its fake display has no scale to inherit. Where the
  // driver fails only because the GL stack lacks EGL device enumeration
  // (ANGLE never implements it), the same pbuffer is built without SDL's video
  // subsystem (egl_headless.rs) behind SDL's dummy driver, which still
  // provides the Window the playback loop sizes from. Anything else falls
  // back to the interactive path's hidden window on the real display. Each
  // failed attempt dropped its video subsystem handles, so setting the hint
  // and re-entering setup re-initializes video on the next driver.
  let resampler = crate::resample::SharedResampler::new();
  let user_input_muted = Arc::new(AtomicBool::new(false));
  if mode.is_playback() {
    sdl3::hint::set("SDL_VIDEO_DRIVER", "offscreen");
    match setup_video(&sdl_context, title, (width, height), &mode) {
      Ok((window, platform)) => return App { sdl_context, window, platform, mode, resampler, user_input_muted },
      Err(e) if e.contains("EXT_device_enumeration") || e.contains("eglQueryDevicesEXT") => {
        log::info!("[alloy] offscreen video driver needs EGL device enumeration, which this GL stack (ANGLE) does not provide; using a headless EGL context");
        sdl3::hint::set("SDL_VIDEO_DRIVER", "dummy");
        match setup_headless(&sdl_context, title, (width, height)) {
          Ok((window, platform)) => return App { sdl_context, window, platform, mode, resampler, user_input_muted },
          Err(e) => log::warn!("[alloy] headless EGL context unavailable ({e}); falling back to a hidden window"),
        }
      }
      Err(e) => log::warn!("[alloy] offscreen video driver unavailable ({e}); falling back to a hidden window"),
    }
    sdl3::hint::set("SDL_VIDEO_DRIVER", "");
  }

  let (window, platform) = setup_video(&sdl_context, title, (width, height), &mode).expect("Failed to set up video");
  App { sdl_context, window, platform, mode, resampler, user_input_muted }
}

// On platforms where GLES comes from ANGLE's shipped libraries, SDL's "Could
// not initialize OpenGL / GLES library" almost always means those libraries
// were not found. Name them and where they were expected, instead of leaving
// the raw SDL error as the only clue.
#[cfg(any(target_os = "windows", target_os = "macos"))]
fn gl_library_hint(err: &str) -> String {
  if !err.contains("Could not initialize OpenGL / GLES library") {
    return String::new();
  }
  #[cfg(target_os = "windows")]
  let names = ["libEGL.dll", "libGLESv2.dll"];
  #[cfg(target_os = "macos")]
  let names = ["libEGL.dylib", "libGLESv2.dylib"];
  let Some(dir) = std::env::current_exe().ok().and_then(|exe| exe.parent().map(std::path::Path::to_path_buf)) else {
    return String::new();
  };
  let missing: Vec<&str> = names.iter().filter(|name| !dir.join(name).exists()).copied().collect();
  if missing.is_empty() {
    format!(" ({} are present next to the executable but failed to load)", names.join(" and "))
  } else {
    format!(" ({} not found next to the executable; the runtime needs ANGLE's GL libraries)", missing.join(" and "))
  }
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn gl_library_hint(_err: &str) -> String {
  String::new()
}

fn setup_video(
  sdl_context: &sdl3::Sdl,
  title: &str,
  (width, height): (u32, u32),
  mode: &Mode,
) -> Result<(sdl3::video::Window, DisplayContext), String> {
  let video = sdl_context.video().map_err(|e| format!("video subsystem: {e}"))?;
  // Platform fact worth having in every log: input behavior (pointer lock,
  // warp, coordinate spaces) differs per driver (wayland vs x11/XWayland).
  log::info!("[alloy] video driver: {}", video.current_video_driver());
  crate::set_video_driver(video.current_video_driver().to_string());

  gl::configure_opengl(&video);

  let mut builder = video.window(title, width, height);
  builder.opengl().position_centered().high_pixel_density();
  // A playback window is hidden and fixed-size: keeping it non-resizable stops
  // the compositor from negotiating a different surface size on a scaled display,
  // which would diverge from the requested capture dimensions.
  if !mode.is_playback() {
    builder.resizable();
  }
  let mut window = builder.build().map_err(|e| format!("window creation: {e}{}", gl_library_hint(&e.to_string())))?;
  if mode.is_playback() {
    window.hide();
  }

  let platform = DisplayContext::new_opengl(&window).map_err(|e| format!("GL setup: {e}"))?;
  Ok((window, platform))
}

// Playback on SDL's dummy video driver: a windowless Window (no GL flag: the
// dummy driver loads no GL library) sized like the capture, plus an EGL
// pbuffer context created outside SDL. See egl_headless.rs.
fn setup_headless(
  sdl_context: &sdl3::Sdl,
  title: &str,
  (width, height): (u32, u32),
) -> Result<(sdl3::video::Window, DisplayContext), String> {
  let video = sdl_context.video().map_err(|e| format!("video subsystem: {e}"))?;
  let window = video.window(title, width, height).hidden().build().map_err(|e| format!("window creation: {e}"))?;
  let platform = DisplayContext::new_egl_pbuffer(width, height)?;
  Ok((window, platform))
}

fn apply_main_thread_effects(event: &AlloyEvent, surface_size: &Arc<AtomicU64>, mode: &Mode) {
  // In playback mode the surface is fixed at the size captured in setup, which is
  // exactly what the frame readback assumes; ignore resize events.
  if mode.is_playback() {
    return;
  }
  if let AlloyEvent::Resize { size, display_scale, .. } = event {
    let (w, h) = ((size.width as f32 * display_scale) as u32, (size.height as f32 * display_scale) as u32);
    surface_size.store(crate::backend::pack_size(w, h), Ordering::Release);
  }
}

// Refresh rate in Hz of the window's current display, with a 60Hz fallback.
fn display_refresh_rate(window: &sdl3::video::Window) -> f32 {
  let hz = window.get_display().and_then(|d| d.get_mode()).map(|m| m.refresh_rate).ok().filter(|&hz| hz > 0.0).unwrap_or(60.0);
  // Every query publishes the fact for out-of-loop readers (crate::refresh_rate).
  crate::set_refresh_rate(hz);
  hz
}

// Payload-less user event the raster thread pushes onto the SDL queue after
// presenting a frame, so the main loop's event wait returns immediately
// instead of at its timeout. It carries nothing: the frame itself travels
// over the mpsc channel, which the woken loop drains.
struct FrameReady;

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

  pub fn run(
    self,
    dl_producer: impl FnOnce(Arc<Context>, mpsc::Sender<AlloyCommand>, mpsc::Receiver<AlloyEvent>) + Send + 'static,
  ) {
    let App { sdl_context, mut window, platform, mode, resampler, user_input_muted } = self;
    let surface_size = platform.surface_size_handle();

    let (tx, rx) = mpsc::channel::<FrameOutput>();
    let (event_tx, event_rx) = mpsc::channel::<AlloyEvent>();
    let (cmd_tx, cmd_rx) = mpsc::channel::<AlloyCommand>();
    // Live counters shared with the raster thread and the Context (see
    // raster::RasterStats). The loop below reads the queue depth to gate the
    // idle Tick and increments idle_ticks per emitted Tick.
    let stats = Arc::new(crate::raster::RasterStats::new());
    // Frame wakeup for the interactive loop below: it sleeps on the SDL event
    // queue, so a presented frame must push an event to be noticed before the
    // wait's timeout. Playback mode blocks on the frame channel directly.
    let wake: Option<Box<dyn Fn() + Send + Sync>> = if mode.is_playback() {
      None
    } else {
      let events = sdl_context.event().expect("Failed to get SDL event subsystem");
      events.register_custom_event::<FrameReady>().expect("Failed to register frame event");
      let sender = events.event_sender();
      Some(Box::new(move || {
        sender.push_custom_event(FrameReady).ok();
      }))
    };
    let raster =
      platform.run_context(move |ctx| dl_producer(ctx, cmd_tx, event_rx), tx, wake, mode.is_playback(), stats.clone());

    // Surface-liveness policy (rebind + repaint across the surface
    // lifecycle; see liveness.rs). The latch half arrives later via
    // AlloyCommand::SetFrameRequestLatch.
    let mut liveness = SurfaceLiveness::new();

    let initial = if mode.is_playback() { playback_resize_event(&window) } else { current_resize_event(&window) };
    apply_main_thread_effects(&initial, &surface_size, &mode);
    if liveness.on_event(&initial, Instant::now()) {
      raster.send(RasterCmd::RebindWindowSurface).ok();
    }
    event_tx.send(initial).ok();

    if let Mode::Playback(playback) = mode {
      run_playback_loop(window, rx, event_tx, playback);
      return;
    }

    // Timely "hidden" on Android: with BLOCK_ON_PAUSE the pump blocks before
    // the queued background events are drained, so through the normal path
    // the app would learn about backgrounding only at resume
    // (device-observed). An event watch runs synchronously on the thread
    // pushing the event - for DID_ENTER_BACKGROUND that is this thread,
    // inside the blocking wait - so the transition is forwarded the moment
    // it is queued and the JS side (which keeps running while the pump is
    // blocked) can persist state. The queue's own copy still arrives at
    // resume; consumers tolerate the repeat (see AlloyEvent::Visibility).
    // The binding must outlive the loop: dropping an EventWatch removes it.
    let watch_event_tx = event_tx.clone();
    let _event_watch = sdl_context.event().expect("Failed to get SDL event subsystem").add_event_watch(
      move |event: sdl3::event::Event| {
        if matches!(event, sdl3::event::Event::AppDidEnterBackground { .. }) {
          watch_event_tx.send(AlloyEvent::Visibility { visible: false }).ok();
        }
      },
    );

    // Where a platform vsync backend exists (Android today), FrameRendered is
    // deferred to the display's vsync (see vsync.rs) so frame production
    // phase-locks to the clock the platform batches input on, and a built
    // frame's buffer sits in the queue as briefly as possible. The vsync
    // thread wakes the blocked event wait exactly like the raster thread does
    // after a present. None = present-return pacing, unchanged.
    let vsync = {
      let sender = sdl_context.event().expect("Failed to get SDL event subsystem").event_sender();
      crate::vsync::VsyncSource::start(move || {
        sender.push_custom_event(FrameReady).ok();
      })
    };
    // Headroom on the vsync-signal deadline beyond its latest legitimate
    // arrival (request + period + delay): sleep overshoot on the vsync thread
    // plus channel/wake latency into the main loop.
    const VSYNC_SLACK: Duration = Duration::from_millis(4);
    // Presents whose FrameRendered awaits the vsync signal. vsync_deadline
    // drives the fallback: if the armed request's signal has not arrived by
    // the latest instant it legitimately could (request time + one period to
    // the next choreographer vsync + the armed delay + slack), release the
    // pending present instead of stalling frame production. Anchoring on the
    // request keeps the fallback tight - a lost signal costs a ~1.6-period
    // production gap instead of the 2-3 the old present-return anchor allowed
    // - while never firing before a healthy signal could still arrive. Racing
    // a merely-late one is harmless anyway: the fallback supersedes it (new
    // request generation) and the chain re-locks at the next vsync.
    let mut pending_presents: u32 = 0;
    let mut vsync_deadline = Instant::now();
    // Frame-release policy (see vsync::FramePacing). VsyncLocked until the
    // embedder's policy arrives; only consulted where a vsync backend exists.
    let mut frame_pacing = crate::vsync::FramePacing::VsyncLocked;
    // Whether a vsync request is outstanding (at most one ever is). Armed at
    // signal emission for the NEXT vsync - not at present-return, which lands
    // near the vsync boundary after the full build+draw pipeline and loses
    // the re-arm race often enough to halve the frame rate (measured
    // 41-51/60). Disarmed by taking the signal; a signal taken with nothing
    // pending ends the chain (demand stopped), costing one spare callback.
    let mut vsync_armed = false;
    // Pipeline cost estimator for the signal delay: signal_emitted marks each
    // vsync-released FrameRendered, and its matching Presented closes the
    // sample. Tick-triggered presents (first frame out of idle) have no open
    // mark and are not sampled.
    let mut pacing = crate::vsync::PacingBudget::new();
    let mut signal_emitted: Option<Instant> = None;

    let mut event_pump = sdl_context.event_pump().expect("Failed to get SDL event pump");
    // None when SDL has no gamepad support on this platform; pads already
    // plugged in surface through the Added events SDL emits on subsystem init.
    let mut gamepads = crate::gamepad::Gamepads::new(&sdl_context);
    let mut frame: u64 = 0;

    // Raw facts only: a wall-clock timestamp sampled at present, plus the display
    // refresh rate (its own event, delivered on init and on change). Smoothing
    // and pacing are userspace policy.
    let start_time = Instant::now();
    let mut refresh_rate = display_refresh_rate(&window);

    let mut fps_last_second = Instant::now();
    let mut fps_frame_count: u32 = 0;
    let mut fps: u32 = 0;
    // Pointer moves received from SDL in the current second, logged 1/s while
    // input flows: the arrival rate shows whether the platform delivers moves
    // at input-device rate or batched to vsync (drag latency diagnostics).
    let mut pointer_moves: u32 = 0;
    // Pointer-lock coordinate freeze (web parity): SDL does NOT freeze x/y
    // in relative mode - it reports a window-clamped simulated position -
    // so hit testing would follow an invisible point. While locked, mouse
    // events report the lock point instead; motion continues via rel.
    let mut pointer_lock_frozen: Option<(f32, f32)> = None;
    let mut last_mouse: (f32, f32) = (0.0, 0.0);
    let mut last_power_check = Instant::now();

    // Polled each loop iteration; transitions emit KeyboardVisibility so the
    // JS side can react (auto-blur on hide, layout adjustments on show).
    let mut prev_keyboard_shown = false;
    let mut prev_keyboard_height = 0.0_f32;

    // Android hardware-keyboard hotplug arrives via JNI (a Configuration
    // change), not as an SDL keyboard event, so the loop watches the fact and
    // re-emits InputDevices on change. Elsewhere SDL's own added/removed
    // events cover hotplug and this never transitions.
    let mut prev_physical_keyboard = crate::sdl_utils::physical_keyboard();

    // Instant of the last frame signal (FrameRendered or Tick). When the UI
    // thread submits nothing for a full refresh period, an idle Tick keeps its
    // per-frame logic running while the GPU stays idle; presents reset the
    // deadline so Ticks only fire when no frames are being produced.
    let mut last_frame_signal = Instant::now();

    loop {
      let tick_period = Duration::from_secs_f64(1.0 / refresh_rate.max(1.0) as f64);
      // Sleep on the SDL event queue until the next idle-tick deadline: input
      // wakes it directly and each submitted frame pushes a FrameReady user
      // event (see Context::submit), so nothing needs polling. The woken-for
      // event is handled below alongside the rest of the queue; a FrameReady
      // falls through translate_event as a no-op, its work is the rx drain.
      let remaining = if pending_presents > 0 {
        // A present awaits its vsync signal: Ticks are suppressed and the
        // wake comes from the vsync thread, so wait until the fallback
        // deadline instead (neither spinning nor sleeping through it).
        vsync_deadline.saturating_duration_since(Instant::now())
      } else {
        tick_period.saturating_sub(last_frame_signal.elapsed())
      };
      let first_event = if remaining.is_zero() {
        None
      } else {
        // SDL waits in whole milliseconds; round up so a sub-millisecond
        // remainder does not degrade into a spin on zero-length waits.
        let ms = remaining.as_millis() as u32 + (remaining.as_micros() % 1000 != 0) as u32;
        event_pump.wait_event_timeout_ms(ms)
      };

      // Drain the raster thread's frame notifications. Drawing and presenting
      // have already happened over there (it owns the process's single GL
      // context); each notification is one on-screen frame, so roll the
      // counters and emit its FrameRendered.
      let mut disconnected = false;
      loop {
        match rx.try_recv() {
          Ok(FrameOutput::Presented) => {
            fps_frame_count += 1;
            match &vsync {
              Some(v) if frame_pacing == crate::vsync::FramePacing::VsyncLocked => {
                if let Some(emitted) = signal_emitted.take() {
                  pacing.record(emitted.elapsed().as_secs_f32() * 1000.0, tick_period);
                }
                pending_presents += 1;
                // Normally the signal releasing this present is already
                // armed (pre-armed when the previous one was emitted); this
                // request only starts the chain on the first present out of
                // idle.
                if !vsync_armed {
                  let delay = pacing.delay(tick_period);
                  vsync_deadline = Instant::now() + tick_period + delay + VSYNC_SLACK;
                  v.request(delay);
                  vsync_armed = true;
                }
              }
              // No vsync backend, or SwapPaced policy: the frame signal
              // follows the present directly and the blocking swap paces.
              _ => {
                let time = start_time.elapsed().as_secs_f64();
                event_tx.send(AlloyEvent::FrameRendered { frame, fps, time }).ok();
                frame += 1;
                last_frame_signal = Instant::now();
                liveness.on_frame_signal(last_frame_signal);
              }
            }
          }
          // Captured frames only exist in playback mode, which never reaches
          // this loop.
          Ok(FrameOutput::Captured(_)) => {}
          Err(mpsc::TryRecvError::Empty) => break,
          Err(mpsc::TryRecvError::Disconnected) => {
            disconnected = true;
            break;
          }
        }
      }
      if disconnected {
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
          let delay_ms = pacing.current_ms();
          log::debug!("[alloy] pacing: signal delay {delay_ms:.1}ms");
        }
        // Safety net: report a refresh-rate change the display event might miss.
        let hz = display_refresh_rate(&window);
        if hz != refresh_rate {
          refresh_rate = hz;
          event_tx.send(AlloyEvent::DisplayRefreshRate { hz }).ok();
        }
      }

      // No idle Tick while a present awaits its vsync signal: the real frame
      // signal is at most a refresh period away (fallback included). And no
      // idle Tick while raster commands are queued or executing: a backlogged
      // raster thread also shows pending_presents == 0 (nothing has come back
      // to present), and ticking through that backlog feeds it more per-frame
      // work than it retires - frame time diverges without bound (see
      // okf/backlog/idle-tick-gpu-backlog-runaway.md). Idle means idle: no
      // presents in flight AND an empty raster queue. The deadline resets on
      // suppression too, or `remaining` above stays zero and the loop spins
      // through the backlog instead of sleeping; ticks resume within one
      // refresh period of the queue draining.
      if pending_presents == 0 && last_frame_signal.elapsed() >= tick_period {
        if stats.queue_depth.load(Ordering::Acquire) == 0 {
          event_tx.send(AlloyEvent::Tick { frame, fps }).ok();
          stats.idle_ticks.fetch_add(1, Ordering::Relaxed);
          liveness.on_frame_signal(Instant::now());
        }
        last_frame_signal = Instant::now();
      }
      // The dev-tool user-input mute (user_input_muted), read once per
      // iteration: the translated-event path below drops muted input
      // (is_muted_input); the level-read pads apply it themselves
      // (Gamepads::set_muted).
      let muted = user_input_muted.load(Ordering::Relaxed);
      if let Some(g) = gamepads.as_mut() {
        g.set_muted(muted);
      }
      liveness.begin_pump();
      for sdl_event in first_event.into_iter().chain(event_pump.poll_iter()) {
        if let sdl3::event::Event::Display { display_event, .. } = &sdl_event {
          use sdl3::event::DisplayEvent;
          if matches!(display_event, DisplayEvent::CurrentModeChanged | DisplayEvent::DesktopModeChanged) {
            let hz = display_refresh_rate(&window);
            if hz != refresh_rate {
              refresh_rate = hz;
              event_tx.send(AlloyEvent::DisplayRefreshRate { hz }).ok();
            }
          }
        }
        if let Some(g) = gamepads.as_mut() {
          g.handle_event(&sdl_event);
        }
        if let Some(mut e) = translate_event(sdl_event, &window) {
          // Mouse position bookkeeping and the pointer-lock freeze: while
          // locked, mouse events carry the lock point (see
          // pointer_lock_frozen); unlocked, remember the real position as
          // the next lock's freeze point.
          match &mut e {
            AlloyEvent::PointerMove { pointer_type: crate::PointerType::Mouse, x, y, .. }
            | AlloyEvent::PointerDown { pointer_type: crate::PointerType::Mouse, x, y, .. }
            | AlloyEvent::PointerUp { pointer_type: crate::PointerType::Mouse, x, y, .. }
            | AlloyEvent::Wheel { pointer_type: crate::PointerType::Mouse, x, y, .. } => match pointer_lock_frozen {
              Some((fx, fy)) => {
                *x = fx;
                *y = fy;
              }
              None => last_mouse = (*x, *y),
            },
            _ => {}
          }
          apply_main_thread_effects(&e, &surface_size, &mode);
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
            continue;
          }
          // Producer-side resampler feed (see resample.rs): moves are
          // consumed here - the UI side samples one position per pointer
          // per frame slot - while downs seed and ups drop the history
          // before their events travel.
          if resampler.feed(&e) {
            pointer_moves += 1;
            continue;
          }
          event_tx.send(e).ok();
        }
      }
      // Flush presents deferred to vsync - after the SDL event drain, so the
      // input delivered at the same vsync is already in the event channel and
      // the UI batch runs it before this frame signal (a signal processed
      // ahead of its vsync's input finds nothing dirty and wastes the frame).
      // One signal releases all pending (at most one in practice: the UI
      // thread builds the next frame only after this emission). try_take
      // drains superseded-generation signals internally, so a late one,
      // arriving after its present was released by the fallback, cannot
      // release a future present early.
      if let Some(v) = &vsync {
        let mut due = v.try_take();
        if due {
          vsync_armed = false;
        }
        if pending_presents > 0 {
          if !due && Instant::now() >= vsync_deadline {
            // Debug, not warn: a GPU-saturated device (Android TV) misses
            // vsyncs in steady state, one line per missed frame. SRT_LOG=debug
            // surfaces them when diagnosing the vsync source itself.
            log::debug!("[alloy] vsync signal missed; emitting frame signal after timeout");
            due = true;
            // The armed signal did not make it in time; disarm so the pre-arm
            // below sends a fresh request, superseding it - when it lands it
            // will be discarded instead of releasing the next present early.
            vsync_armed = false;
          }
          if due {
            while pending_presents > 0 {
              pending_presents -= 1;
              let time = start_time.elapsed().as_secs_f64();
              event_tx.send(AlloyEvent::FrameRendered { frame, fps, time }).ok();
              frame += 1;
            }
            last_frame_signal = Instant::now();
            liveness.on_frame_signal(last_frame_signal);
            signal_emitted = Some(Instant::now());
            // Pre-arm the signal for the next vsync while this frame is
            // being built: the signal timing must not depend on when the
            // build's present returns (see vsync_armed). The frame this
            // emission triggers has until that signal - a full period plus
            // the delay - to present, or it slips a frame.
            if !vsync_armed {
              let delay = pacing.delay(tick_period);
              vsync_deadline = Instant::now() + tick_period + delay + VSYNC_SLACK;
              v.request(delay);
              vsync_armed = true;
            }
          }
        }
      }

      // At most one gamepad snapshot per iteration, however many pad events
      // were drained above. The back-button edge follows the snapshot so an
      // app that watches pads sees the final state before the back intent.
      if let Some(g) = gamepads.as_mut() {
        if let Some(e) = g.take_snapshot_if_dirty() {
          event_tx.send(e).ok();
        }
        if g.take_back_edge() {
          event_tx.send(AlloyEvent::Back).ok();
        }
      }
      while let Ok(cmd) = cmd_rx.try_recv() {
        match cmd {
          AlloyCommand::EmitInitEvents => {
            let e = current_resize_event(&window);
            apply_main_thread_effects(&e, &surface_size, &mode);
            if liveness.on_event(&e, Instant::now()) {
              raster.send(RasterCmd::RebindWindowSurface).ok();
            }
            event_tx.send(e).ok();
            event_tx.send(AlloyEvent::DisplayRefreshRate { hz: refresh_rate }).ok();
            event_tx.send(current_system_theme_event()).ok();
            event_tx.send(current_input_devices_event()).ok();
            event_tx.send(current_orientation_event(&window)).ok();
            if let Some(g) = gamepads.as_ref() {
              event_tx.send(g.snapshot_event()).ok();
            }
            // Pointer lock survives an engine reload (window state); the new
            // engine must observe it.
            event_tx.send(AlloyEvent::PointerLock { locked: sdl_context.mouse().relative_mouse_mode(&window) }).ok();
          }
          AlloyCommand::SetFrameRequestLatch(latch) => liveness.set_latch(latch),
          AlloyCommand::SetFramePacing(p) => {
            if frame_pacing != p {
              log::info!("[alloy] frame pacing: {p:?}");
              frame_pacing = p;
              // Presents already deferred to a vsync signal must not strand
              // when leaving VsyncLocked; release them now. The outstanding
              // vsync request stays armed and its signal drains harmlessly
              // with nothing pending.
              if p == crate::vsync::FramePacing::SwapPaced {
                while pending_presents > 0 {
                  pending_presents -= 1;
                  let time = start_time.elapsed().as_secs_f64();
                  event_tx.send(AlloyEvent::FrameRendered { frame, fps, time }).ok();
                  frame += 1;
                }
                last_frame_signal = Instant::now();
                liveness.on_frame_signal(last_frame_signal);
              }
            }
          }
          AlloyCommand::SetTitle(t) => {
            if let Err(e) = window.set_title(&t) {
              log::warn!("set_title failed: {e}");
            }
          }
          AlloyCommand::SetIcon { width, height, rgba } => {
            // Debug, not warn: macOS reports failure on every app switch
            // because the platform has no window icons at all.
            if let Err(e) = crate::sdl_utils::set_window_icon(&window, width, height, &rgba) {
              log::debug!("set_window_icon failed: {e}");
            }
          }
          AlloyCommand::SetFullscreen(fs) => {
            if let Err(e) = window.set_fullscreen(fs) {
              log::warn!("set_fullscreen failed: {e}");
            }
          }
          AlloyCommand::SetCursor(cursor) => match sdl3::mouse::Cursor::from_system(cursor) {
            Ok(c) => c.set(),
            Err(e) => log::warn!("set_cursor failed: {e}"),
          },
          AlloyCommand::SetCursorVisible(visible) => {
            sdl_context.mouse().show_cursor(visible);
          }
          AlloyCommand::SetPointerLock(locked) => {
            let mouse = sdl_context.mouse();
            mouse.set_relative_mouse_mode(&window, locked);
            // Report the applied state, not the request: SDL can refuse
            // (platform without relative mode), and the answer is the fact
            // the reactive accessor reflects.
            let applied = mouse.relative_mouse_mode(&window);
            pointer_lock_frozen = if applied { Some(last_mouse) } else { None };
            event_tx.send(AlloyEvent::PointerLock { locked: applied }).ok();
          }
          AlloyCommand::SetTextInputActive(active, options) => {
            if let Ok(video) = sdl_context.video() {
              if active {
                // SDL's default policy ("auto": show the screen keyboard
                // unless SDL_HasKeyboard) is blind on Android, which never
                // registers keyboards; feed it the platform fact so an
                // attached hardware keyboard suppresses the on-screen one.
                let hint = if crate::sdl_utils::physical_keyboard() { "false" } else { "auto" };
                sdl3::hint::set("SDL_ENABLE_SCREEN_KEYBOARD", hint);
                crate::sdl_utils::start_text_input_with_options(&window, &options);
              } else {
                video.text_input().stop(&window);
              }
            }
          }
          AlloyCommand::Background => {
            if !window.minimize() {
              log::warn!("background (minimize) failed: {}", crate::sdl_utils::sdl_error());
            }
          }
        }
      }

      if let Ok(video) = sdl_context.video() {
        let shown = video.text_input().is_screen_keyboard_shown(&window);
        let scale = crate::sdl_utils::window_display_scale(&window);
        let height = crate::keyboard_inset_px() as f32 / scale;
        if shown != prev_keyboard_shown || height != prev_keyboard_height {
          prev_keyboard_shown = shown;
          prev_keyboard_height = height;
          event_tx.send(AlloyEvent::KeyboardVisibility { shown, height }).ok();
        }
      }

      let physical_keyboard = crate::sdl_utils::physical_keyboard();
      if physical_keyboard != prev_physical_keyboard {
        prev_physical_keyboard = physical_keyboard;
        event_tx.send(current_input_devices_event()).ok();
      }

      //TODO how often do we check? configurable? send on AlloyCommand only?
      if last_power_check.elapsed().as_secs() >= 10 {
        last_power_check = Instant::now();
        event_tx.send(AlloyEvent::PowerStatus { info: crate::sdl_utils::get_power_info() }).ok();
      }
    }
  }
}
