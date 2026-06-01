use impellers::{DisplayList, ISize};
use std::sync::{mpsc, Arc};
use std::time::Instant;

use crate::backend::{create_render_surface, DisplayContext, RenderSurface};
use crate::context::Context;
use crate::event::{current_resize_event, translate_event, AlloyCommand, AlloyEvent};
use crate::gl;
use crate::record::{run_record_loop, RecordConfig};

// Per-present fraction by which the frame clock is pulled toward real elapsed
// time. Higher converges faster after a framerate drop but leaks more
// present-timestamp jitter into the animation; lower stays smoother but lags
// longer before reaching real-time speed.
const CLOCK_CORRECTION_GAIN: f64 = 0.05;

pub struct App {
  sdl_context: sdl3::Sdl,
  window: sdl3::video::Window,
  platform: DisplayContext,
  render_surface: Box<dyn RenderSurface>,
  record: Option<RecordConfig>,
}

pub fn setup(title: &str, size: ISize) -> App {
  let (width, height) = (size.width as u32, size.height as u32);

  // Keep touch and mouse streams separate. Without this, SDL synthesizes
  // mouse events from touches (and vice versa) using SDL_TOUCH_MOUSEID
  // as `which`, which would arrive on our mouse arm and be misclassified
  // as PointerType::Mouse with a sentinel pointer_id.
  sdl3::hint::set("SDL_TOUCH_MOUSE_EVENTS", "0");
  sdl3::hint::set("SDL_MOUSE_TOUCH_EVENTS", "0");

  let sdl_context = sdl3::init().expect("Failed to initialize SDL3");
  let video = sdl_context.video().expect("Failed to get video subsystem");

  gl::configure_opengl(&video);

  let window = video
    .window(title, width, height)
    .opengl()
    .position_centered()
    // .fullscreen()
    .resizable()
    .high_pixel_density()
    .build()
    .expect("Failed to create window");

  let platform = DisplayContext::new_opengl(&video, &window).expect("Failed to set up platform");

  let (w, h) = window.size_in_pixels();
  let window_size = ISize::new(w as i64, h as i64);
  let render_surface = create_render_surface(&platform, window_size).expect("Failed to create render surface");

  App { sdl_context, window, platform, render_surface, record: None }
}

fn apply_main_thread_effects(event: &AlloyEvent, render_surface: &mut Box<dyn RenderSurface>) {
  if let AlloyEvent::Resize { size, display_scale, .. } = event {
    let phys = ISize::new((size.width as f32 * display_scale) as i64, (size.height as f32 * display_scale) as i64);
    render_surface.resize(phys);
  }
}

// Seconds per refresh of the window's current display, with a 60Hz fallback.
fn display_period(window: &sdl3::video::Window) -> f64 {
  window
    .get_display()
    .and_then(|d| d.get_mode())
    .map(|m| m.refresh_rate)
    .ok()
    .filter(|&hz| hz > 0.0)
    .map(|hz| 1.0 / hz as f64)
    .unwrap_or(1.0 / 60.0)
}

impl App {
  pub fn with_recording(mut self, config: RecordConfig) -> Self {
    self.window.hide();
    self.record = Some(config);
    self
  }

  pub fn run(
    self,
    dl_producer: impl FnOnce(Arc<Context>, mpsc::Sender<AlloyCommand>, mpsc::Receiver<AlloyEvent>) + Send + 'static,
  ) {
    let App { sdl_context, mut window, platform, mut render_surface, record } = self;

    let (tx, rx) = mpsc::channel::<DisplayList>();
    let (event_tx, event_rx) = mpsc::channel::<AlloyEvent>();
    let (cmd_tx, cmd_rx) = mpsc::channel::<AlloyCommand>();
    platform.run_context(move |ctx| dl_producer(ctx, cmd_tx, event_rx), tx);

    let initial = current_resize_event(&window);
    apply_main_thread_effects(&initial, &mut render_surface);
    event_tx.send(initial).ok();

    if let Some(record) = record {
      run_record_loop(window, render_surface, rx, event_tx, record);
      return;
    }

    let mut event_pump = sdl_context.event_pump().expect("Failed to get SDL event pump");
    let mut frame: u64 = 0;

    // Pace the frame clock by present count, not by the swap-return wall clock:
    // the swap returns at a jittery time relative to scanout, but the compositor
    // displays one present per vblank, so a fixed step per present is steady.
    // Accumulate rather than compute frame * period, because the refresh rate can
    // change at runtime (e.g. Android 90 <-> 60Hz); each present advances by the
    // period current at that moment, keeping the clock continuous across a change.
    let mut frame_period = display_period(&window);
    let mut clock: f64 = 0.0;
    let clock_start = Instant::now();

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
            // Safety net for refresh-rate changes the display event might miss.
            frame_period = display_period(&window);
          }
          render_surface.draw_display_list(&dl).expect("Failed to draw display list");
          render_surface.present();
          let time = clock;
          event_tx.send(AlloyEvent::FrameRendered { frame, fps, time }).ok();
          // Advance one refresh period, then gently pull the clock toward real
          // elapsed time so a sustained framerate drop (one present spanning
          // several vblanks) keeps real-time speed without leaking per-frame jitter.
          clock += frame_period;
          let drift = clock_start.elapsed().as_secs_f64() - clock;
          clock += drift * CLOCK_CORRECTION_GAIN;
          frame += 1;
        }
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
      }
      for sdl_event in event_pump.poll_iter() {
        if let sdl3::event::Event::Display { display_event, .. } = &sdl_event {
          use sdl3::event::DisplayEvent;
          if matches!(display_event, DisplayEvent::CurrentModeChanged | DisplayEvent::DesktopModeChanged) {
            frame_period = display_period(&window);
          }
        }
        if let Some(e) = translate_event(sdl_event, &window) {
          apply_main_thread_effects(&e, &mut render_surface);
          event_tx.send(e).ok();
        }
      }
      while let Ok(cmd) = cmd_rx.try_recv() {
        match cmd {
          AlloyCommand::EmitInitEvents => {
            let e = current_resize_event(&window);
            apply_main_thread_effects(&e, &mut render_surface);
            event_tx.send(e).ok();
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
