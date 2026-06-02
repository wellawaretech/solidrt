use impellers::{DisplayList, ISize};
use std::sync::{mpsc, Arc};
use std::time::Instant;

use crate::backend::{create_render_surface, DisplayContext, RenderSurface};
use crate::context::Context;
use crate::event::{current_resize_event, translate_event, AlloyCommand, AlloyEvent};
use crate::gl;
use crate::mode::Mode;
use crate::record::run_record_loop;

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
  // For recording, force 1:1 pixel mapping so the window is exactly the
  // requested size in physical pixels regardless of display scale.
  if mode.is_record() {
    sdl3::hint::set("SDL_VIDEO_WAYLAND_SCALE_TO_DISPLAY", "1");
  }

  let sdl_context = sdl3::init().expect("Failed to initialize SDL3");
  let video = sdl_context.video().expect("Failed to get video subsystem");

  gl::configure_opengl(&video);

  let mut builder = video.window(title, width, height);
  builder.opengl().position_centered().high_pixel_density();
  // A recording window is hidden and fixed-size: keeping it non-resizable stops
  // the compositor from negotiating a different surface size on a scaled display,
  // which would diverge from the requested capture dimensions.
  if !mode.is_record() {
    builder.resizable();
  }
  let mut window = builder.build().expect("Failed to create window");
  if mode.is_record() {
    window.hide();
  }

  let platform = DisplayContext::new_opengl(&video, &window).expect("Failed to set up platform");

  let (w, h) = window.size_in_pixels();
  let window_size = ISize::new(w as i64, h as i64);
  let render_surface = create_render_surface(&platform, window_size).expect("Failed to create render surface");

  App { sdl_context, window, platform, render_surface, mode }
}

fn apply_main_thread_effects(event: &AlloyEvent, render_surface: &mut Box<dyn RenderSurface>, mode: &Mode) {
  // In record mode the surface is fixed at the size captured in setup, which is
  // exactly what the frame readback assumes; ignore resize events.
  if mode.is_record() {
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

impl App {
  pub fn run(
    self,
    dl_producer: impl FnOnce(Arc<Context>, mpsc::Sender<AlloyCommand>, mpsc::Receiver<AlloyEvent>) + Send + 'static,
  ) {
    let App { sdl_context, mut window, platform, mut render_surface, mode } = self;

    let (tx, rx) = mpsc::channel::<DisplayList>();
    let (event_tx, event_rx) = mpsc::channel::<AlloyEvent>();
    let (cmd_tx, cmd_rx) = mpsc::channel::<AlloyCommand>();
    platform.run_context(move |ctx| dl_producer(ctx, cmd_tx, event_rx), tx);

    let initial = current_resize_event(&window);
    apply_main_thread_effects(&initial, &mut render_surface, &mode);
    event_tx.send(initial).ok();

    if let Mode::Record(record) = mode {
      run_record_loop(window, render_surface, rx, event_tx, record);
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

    loop {
      match rx.recv_timeout(std::time::Duration::from_millis(8)) {
        Ok(mut dl) => {
          while let Ok(newer) = rx.try_recv() {
            dl = newer;
          }
          let frame_time = Instant::now();
          fps_frame_count += 1;
          if frame_time.saturating_duration_since(fps_last_second).as_secs_f32() >= 1.0 {
            fps = fps_frame_count;
            fps_frame_count = 0;
            fps_last_second = frame_time;
            // Safety net: report a refresh-rate change the display event might miss.
            let hz = display_refresh_rate(&window);
            if hz != refresh_rate {
              refresh_rate = hz;
              event_tx.send(AlloyEvent::DisplayRefreshRate { hz }).ok();
            }
          }
          render_surface.draw_display_list(&dl).expect("Failed to draw display list");
          render_surface.present();
          let time = start_time.elapsed().as_secs_f64();
          event_tx.send(AlloyEvent::FrameRendered { frame, fps, time }).ok();
          frame += 1;
        }
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
      }
      for sdl_event in event_pump.poll_iter() {
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
        if shown != prev_keyboard_shown {
          prev_keyboard_shown = shown;
          event_tx.send(AlloyEvent::KeyboardVisibility { shown }).ok();
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
