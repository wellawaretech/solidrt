use impellers::ISize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use crate::backend::{DisplayContext, FrameOutput};
use crate::context::Context;
use crate::event::{
  current_input_devices_event, current_orientation_event, current_resize_event, current_system_theme_event,
  translate_event, AlloyCommand, AlloyEvent,
};
use crate::gl;
use crate::mode::Mode;
use crate::playback::run_playback_loop;

pub struct App {
  sdl_context: sdl3::Sdl,
  window: sdl3::video::Window,
  platform: DisplayContext,
  mode: Mode,
}

pub fn setup(title: &str, size: ISize, mode: Mode) -> App {
  let (width, height) = (size.width as u32, size.height as u32);

  // Keep touch and mouse streams separate. Without this, SDL synthesizes
  // mouse events from touches (and vice versa) using SDL_TOUCH_MOUSEID
  // as `which`, which would arrive on our mouse arm and be misclassified
  // as PointerType::Mouse with a sentinel pointer_id.
  sdl3::hint::set("SDL_TOUCH_MOUSE_EVENTS", "0");
  sdl3::hint::set("SDL_MOUSE_TOUCH_EVENTS", "0");
  // For playback, force 1:1 pixel mapping so the window is exactly the
  // requested size in physical pixels regardless of display scale.
  if mode.is_playback() {
    sdl3::hint::set("SDL_VIDEO_WAYLAND_SCALE_TO_DISPLAY", "1");
  }

  let sdl_context = sdl3::init().expect("Failed to initialize SDL3");
  // On Android, hand SDL's JNI env + activity to ndk-context so JNI-using deps
  // (iroh's network monitoring via flux:p2p) can reach the Android context.
  #[cfg(target_os = "android")]
  crate::sdl_utils::init_android_context();
  let video = sdl_context.video().expect("Failed to get video subsystem");

  gl::configure_opengl(&video);

  let mut builder = video.window(title, width, height);
  builder.opengl().position_centered().high_pixel_density();
  // A playback window is hidden and fixed-size: keeping it non-resizable stops
  // the compositor from negotiating a different surface size on a scaled display,
  // which would diverge from the requested capture dimensions.
  if !mode.is_playback() {
    builder.resizable();
  }
  let mut window = builder.build().expect("Failed to create window");
  if mode.is_playback() {
    window.hide();
  }

  let platform = DisplayContext::new_opengl(&window).expect("Failed to set up platform");

  App { sdl_context, window, platform, mode }
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
  window.get_display().and_then(|d| d.get_mode()).map(|m| m.refresh_rate).ok().filter(|&hz| hz > 0.0).unwrap_or(60.0)
}

// Payload-less user event the raster thread pushes onto the SDL queue after
// presenting a frame, so the main loop's event wait returns immediately
// instead of at its timeout. It carries nothing: the frame itself travels
// over the mpsc channel, which the woken loop drains.
struct FrameReady;

impl App {
  pub fn run(
    self,
    dl_producer: impl FnOnce(Arc<Context>, mpsc::Sender<AlloyCommand>, mpsc::Receiver<AlloyEvent>) + Send + 'static,
  ) {
    let App { sdl_context, mut window, platform, mode } = self;
    let surface_size = platform.surface_size_handle();

    let (tx, rx) = mpsc::channel::<FrameOutput>();
    let (event_tx, event_rx) = mpsc::channel::<AlloyEvent>();
    let (cmd_tx, cmd_rx) = mpsc::channel::<AlloyCommand>();
    // Raster commands sent but not yet executed (see raster::RasterSender).
    // The loop below reads it to gate the idle Tick; the Context exposes it
    // for diagnostics.
    let raster_queue = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    // Cumulative idle Ticks emitted below, exposed through the Context
    // alongside the queue depth (see the Tick gate for why they pair).
    let idle_ticks = Arc::new(AtomicU64::new(0));
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
    platform.run_context(
      move |ctx| dl_producer(ctx, cmd_tx, event_rx),
      tx,
      wake,
      mode.is_playback(),
      raster_queue.clone(),
      idle_ticks.clone(),
    );

    let initial = current_resize_event(&window);
    apply_main_thread_effects(&initial, &surface_size, &mode);
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
    let _event_watch = sdl_context
      .event()
      .expect("Failed to get SDL event subsystem")
      .add_event_watch(move |event: sdl3::event::Event| {
        if matches!(event, sdl3::event::Event::AppDidEnterBackground { .. }) {
          watch_event_tx.send(AlloyEvent::Visibility { visible: false }).ok();
        }
      });

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
    // Presents whose FrameRendered awaits the vsync signal. pending_since
    // drives the fallback: if the vsync source stays silent, emit after two
    // refresh periods instead of stalling frame production.
    let mut pending_presents: u32 = 0;
    let mut pending_since = Instant::now();
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
    let mut last_power_check = Instant::now();

    // Polled each loop iteration; transitions emit KeyboardVisibility so the
    // JS side can react (auto-blur on hide, layout adjustments on show).
    let mut prev_keyboard_shown = false;
    let mut prev_keyboard_height = 0.0_f32;

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
        (pending_since + tick_period * 2).saturating_duration_since(Instant::now())
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
              Some(v) => {
                if let Some(emitted) = signal_emitted.take() {
                  pacing.record(emitted.elapsed().as_secs_f32() * 1000.0, tick_period);
                }
                if pending_presents == 0 {
                  pending_since = Instant::now();
                }
                pending_presents += 1;
                // Normally the signal releasing this present is already
                // armed (pre-armed when the previous one was emitted); this
                // request only starts the chain on the first present out of
                // idle.
                if !vsync_armed {
                  v.request(pacing.delay(tick_period));
                  vsync_armed = true;
                }
              }
              None => {
                let time = start_time.elapsed().as_secs_f64();
                event_tx.send(AlloyEvent::FrameRendered { frame, fps, time }).ok();
                frame += 1;
                last_frame_signal = Instant::now();
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
          let delay_ms = pacing.delay(tick_period).as_secs_f32() * 1000.0;
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
        if raster_queue.load(Ordering::Acquire) == 0 {
          event_tx.send(AlloyEvent::Tick { frame, fps }).ok();
          idle_ticks.fetch_add(1, Ordering::Relaxed);
        }
        last_frame_signal = Instant::now();
      }
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
        if let Some(e) = translate_event(sdl_event, &window) {
          if matches!(e, AlloyEvent::PointerMove { .. }) {
            pointer_moves += 1;
          }
          apply_main_thread_effects(&e, &surface_size, &mode);
          event_tx.send(e).ok();
        }
      }
      // Flush presents deferred to vsync - after the SDL event drain, so the
      // input delivered at the same vsync is already in the event channel and
      // the UI batch runs it before this frame signal (a signal processed
      // ahead of its vsync's input finds nothing dirty and wastes the frame).
      // One signal releases all pending (at most one in practice: the UI
      // thread builds the next frame only after this emission). Signals are
      // drained even with nothing pending so a late one, arriving after its
      // present was released by the fallback, cannot release a future present
      // early.
      if let Some(v) = &vsync {
        let mut due = false;
        while v.try_take() {
          due = true;
          vsync_armed = false;
        }
        if pending_presents > 0 {
          if !due && pending_since.elapsed() >= tick_period * 2 {
            log::warn!("[alloy] vsync signal missed; emitting frame signal after timeout");
            due = true;
          }
          if due {
            while pending_presents > 0 {
              pending_presents -= 1;
              let time = start_time.elapsed().as_secs_f64();
              event_tx.send(AlloyEvent::FrameRendered { frame, fps, time }).ok();
              frame += 1;
            }
            last_frame_signal = Instant::now();
            signal_emitted = Some(Instant::now());
            // Pre-arm the signal for the next vsync while this frame is
            // being built: the signal timing must not depend on when the
            // build's present returns (see vsync_armed). The frame this
            // emission triggers has until that signal - a full period plus
            // the delay - to present, or it slips a frame.
            if !vsync_armed {
              v.request(pacing.delay(tick_period));
              vsync_armed = true;
            }
          }
        }
      }

      // At most one gamepad snapshot per iteration, however many pad events
      // were drained above.
      if let Some(e) = gamepads.as_mut().and_then(|g| g.take_snapshot_if_dirty()) {
        event_tx.send(e).ok();
      }
      while let Ok(cmd) = cmd_rx.try_recv() {
        match cmd {
          AlloyCommand::EmitInitEvents => {
            let e = current_resize_event(&window);
            apply_main_thread_effects(&e, &surface_size, &mode);
            event_tx.send(e).ok();
            event_tx.send(AlloyEvent::DisplayRefreshRate { hz: refresh_rate }).ok();
            event_tx.send(current_system_theme_event()).ok();
            event_tx.send(current_input_devices_event()).ok();
            event_tx.send(current_orientation_event(&window)).ok();
            if let Some(g) = gamepads.as_ref() {
              event_tx.send(g.snapshot_event()).ok();
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
          AlloyCommand::SetTextInputActive(active) => {
            if let Ok(video) = sdl_context.video() {
              let ti = video.text_input();
              if active {
                ti.start(&window);
              } else {
                ti.stop(&window);
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

      //TODO how often do we check? configurable? send on AlloyCommand only?
      if last_power_check.elapsed().as_secs() >= 10 {
        last_power_check = Instant::now();
        event_tx.send(AlloyEvent::PowerStatus { info: crate::sdl_utils::get_power_info() }).ok();
      }
    }
  }
}
