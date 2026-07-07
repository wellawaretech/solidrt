use impellers::ISize;
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use crate::backend::{create_render_surface, DisplayContext, Frame, RenderSurface};
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
  render_surface: Box<dyn RenderSurface>,
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

  // MSAA is requested in configure_opengl. Some drivers (notably the Android
  // emulator's GLES translator) expose no multisampled EGL config, which makes
  // window creation fail outright instead of silently dropping MSAA. Retry once
  // without MSAA so rendering proceeds (losing path anti-aliasing).
  let build_window = |video: &sdl3::VideoSubsystem| {
    let mut builder = video.window(title, width, height);
    builder.opengl().position_centered().high_pixel_density();
    // A playback window is hidden and fixed-size: keeping it non-resizable stops
    // the compositor from negotiating a different surface size on a scaled display,
    // which would diverge from the requested capture dimensions.
    if !mode.is_playback() {
      builder.resizable();
    }
    builder.build()
  };
  let mut window = match build_window(&video) {
    Ok(window) => window,
    Err(e) => {
      log::warn!("[alloy] GL window creation failed ({e}); retrying without MSAA");
      gl::disable_msaa(&video);
      build_window(&video).expect("Failed to create window")
    }
  };
  if mode.is_playback() {
    window.hide();
  }

  let platform = DisplayContext::new_opengl(&video, &window).expect("Failed to set up platform");

  let (w, h) = window.size_in_pixels();
  let window_size = ISize::new(w as i64, h as i64);
  let render_surface = create_render_surface(&platform, window_size).expect("Failed to create render surface");

  App { sdl_context, window, platform, render_surface, mode }
}

fn apply_main_thread_effects(event: &AlloyEvent, render_surface: &mut Box<dyn RenderSurface>, mode: &Mode) {
  // In playback mode the surface is fixed at the size captured in setup, which is
  // exactly what the frame readback assumes; ignore resize events.
  if mode.is_playback() {
    return;
  }
  if let AlloyEvent::Resize { size, display_scale, .. } = event {
    let phys = ISize::new((size.width as f32 * display_scale) as i64, (size.height as f32 * display_scale) as i64);
    render_surface.resize(phys);
  }
}

// Refresh rate in Hz of the window's current display, with a 60Hz fallback.
fn display_refresh_rate(window: &sdl3::video::Window) -> f32 {
  window.get_display().and_then(|d| d.get_mode()).map(|m| m.refresh_rate).ok().filter(|&hz| hz > 0.0).unwrap_or(60.0)
}

// Payload-less user event the UI thread pushes onto the SDL queue after
// submitting a frame, so the main loop's event wait returns immediately
// instead of at its timeout. It carries nothing: the frame itself travels
// over the mpsc channel, which the woken loop drains.
struct FrameReady;

impl App {
  pub fn run(
    self,
    dl_producer: impl FnOnce(Arc<Context>, mpsc::Sender<AlloyCommand>, mpsc::Receiver<AlloyEvent>) + Send + 'static,
  ) {
    let App { sdl_context, mut window, platform, mut render_surface, mode } = self;

    let (tx, rx) = mpsc::channel::<Frame>();
    let (event_tx, event_rx) = mpsc::channel::<AlloyEvent>();
    let (cmd_tx, cmd_rx) = mpsc::channel::<AlloyCommand>();
    // Frame wakeup for the interactive loop below: it sleeps on the SDL event
    // queue, so a submitted frame must push an event to be noticed before the
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
    platform.run_context(move |ctx| dl_producer(ctx, cmd_tx, event_rx), tx, wake);

    let initial = current_resize_event(&window);
    apply_main_thread_effects(&initial, &mut render_surface, &mode);
    event_tx.send(initial).ok();

    if let Mode::Playback(playback) = mode {
      run_playback_loop(window, render_surface, rx, event_tx, playback);
      return;
    }

    let mut event_pump = sdl_context.event_pump().expect("Failed to get SDL event pump");
    let mut frame: u64 = 0;

    // Raw facts only: a wall-clock timestamp sampled at present, plus the display
    // refresh rate (its own event, delivered on init and on change). Smoothing
    // and pacing are userspace policy.
    let start_time = Instant::now();
    let mut refresh_rate = display_refresh_rate(&window);

    let mut fps_last_second = Instant::now();
    let mut fps_frame_count: u32 = 0;
    let mut fps: u32 = 0;
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
      let remaining = tick_period.saturating_sub(last_frame_signal.elapsed());
      let first_event = if remaining.is_zero() {
        None
      } else {
        // SDL waits in whole milliseconds; round up so a sub-millisecond
        // remainder does not degrade into a spin on zero-length waits.
        let ms = remaining.as_millis() as u32 + (remaining.as_micros() % 1000 != 0) as u32;
        event_pump.wait_event_timeout_ms(ms)
      };

      // Drain queued frames and present only the newest. Superseded frames'
      // GPU work is subsumed by the newer frame's fence (fences are monotonic
      // on the UI context), so their sync objects are released without waiting.
      let mut newest: Option<Frame> = None;
      let mut disconnected = false;
      loop {
        match rx.try_recv() {
          Ok(f) => {
            if let Some(mut superseded) = newest.replace(f) {
              render_surface.consume_fence(superseded.fence.take(), false);
            }
          }
          Err(mpsc::TryRecvError::Empty) => break,
          Err(mpsc::TryRecvError::Disconnected) => {
            disconnected = true;
            break;
          }
        }
      }
      if let Some(mut sub) = newest {
        fps_frame_count += 1;
        // Wait on the GPU for the UI thread's frame work to finish before
        // Impeller samples its textures (replaces the UI thread's glFinish).
        render_surface.consume_fence(sub.fence.take(), true);
        render_surface.draw_display_list(&sub.dl).expect("Failed to draw display list");
        render_surface.present(&window);
        let time = start_time.elapsed().as_secs_f64();
        event_tx.send(AlloyEvent::FrameRendered { frame, fps, time }).ok();
        frame += 1;
        last_frame_signal = Instant::now();
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
        // Safety net: report a refresh-rate change the display event might miss.
        let hz = display_refresh_rate(&window);
        if hz != refresh_rate {
          refresh_rate = hz;
          event_tx.send(AlloyEvent::DisplayRefreshRate { hz }).ok();
        }
      }

      if last_frame_signal.elapsed() >= tick_period {
        event_tx.send(AlloyEvent::Tick { frame, fps }).ok();
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
        if let Some(e) = translate_event(sdl_event, &window) {
          apply_main_thread_effects(&e, &mut render_surface, &mode);
          event_tx.send(e).ok();
        }
      }
      while let Ok(cmd) = cmd_rx.try_recv() {
        match cmd {
          AlloyCommand::EmitInitEvents => {
            let e = current_resize_event(&window);
            apply_main_thread_effects(&e, &mut render_surface, &mode);
            event_tx.send(e).ok();
            event_tx.send(AlloyEvent::DisplayRefreshRate { hz: refresh_rate }).ok();
            event_tx.send(current_system_theme_event()).ok();
            event_tx.send(current_input_devices_event()).ok();
            event_tx.send(current_orientation_event(&window)).ok();
          }
          AlloyCommand::SetTitle(t) => {
            if let Err(e) = window.set_title(&t) {
              log::warn!("set_title failed: {e}");
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
