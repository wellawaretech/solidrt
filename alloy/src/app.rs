use impellers::{DisplayList, DisplayListBuilder, ISize, Point, Rect, Size};
use std::sync::{mpsc, Arc};
use std::time::Instant;

use crate::backend::{create_render_surface, DisplayContext, RenderSurface};
use crate::context::Context;
use crate::event::{current_resize_event, translate_event, AlloyCommand, AlloyEvent};
use crate::gl;
use crate::sdl_utils;

pub struct App {
  sdl_context: sdl3::Sdl,
  window: sdl3::video::Window,
  platform: DisplayContext,
  render_surface: Box<dyn RenderSurface>,
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
  let render_surface =
    create_render_surface(&platform, window_size).expect("Failed to create render surface");

  App {
    sdl_context,
    window,
    platform,
    render_surface,
  }
}

pub struct FrameInfo {
  pub frame: u64,
  pub frame_time: Instant,
  pub scale: f32,
  pub size: ISize,
  pub safe_area: Rect,
}

pub struct RenderHooks {
  pub pre_render: Box<dyn FnMut(&mut DisplayListBuilder, &FrameInfo)>,
  pub post_render: Box<dyn FnMut(&mut DisplayListBuilder, &FrameInfo)>,
}

fn apply_main_thread_effects(
  event: &AlloyEvent,
  render_surface: &mut Box<dyn RenderSurface>,
  current_scale: &mut f32,
  current_size: &mut ISize,
  current_safe_area: &mut Rect,
) {
  if let AlloyEvent::Resize { size, display_scale, safe_area } = event {
    let phys = ISize::new(
      (size.width as f32 * display_scale) as i64,
      (size.height as f32 * display_scale) as i64,
    );
    render_surface.resize(phys);
    *current_scale = *display_scale;
    *current_size = phys;
    *current_safe_area = *safe_area;
  }
}

impl App {
  pub fn run(
    self,
    dl_producer: impl FnOnce(Arc<Context>, mpsc::Sender<AlloyCommand>, mpsc::Receiver<AlloyEvent>) + Send + 'static,
    mut hooks: RenderHooks,
  ) {
    let App {
      sdl_context,
      window,
      platform,
      mut render_surface,
    } = self;

    let window = window;
    let mut event_pump = sdl_context.event_pump().expect("Failed to get SDL event pump");

    let (tx, rx) = mpsc::channel::<DisplayList>();
    let (event_tx, event_rx) = mpsc::channel::<AlloyEvent>();
    let (cmd_tx, cmd_rx) = mpsc::channel::<AlloyCommand>();
    platform.run_context(move |ctx| dl_producer(ctx, cmd_tx, event_rx), tx);
    let mut frame: u64 = 0;

    let mut current_scale = sdl_utils::window_display_scale(&window);
    let (w0, h0) = window.size_in_pixels();
    let mut current_size = ISize::new(w0 as i64, h0 as i64);
    let mut current_safe_area = Rect::new(Point::new(0.0, 0.0), Size::new(w0 as f32, h0 as f32));

    let initial = current_resize_event(&window);
    apply_main_thread_effects(&initial, &mut render_surface, &mut current_scale, &mut current_size, &mut current_safe_area);
    event_tx.send(initial).ok();

    loop {
      match rx.recv_timeout(std::time::Duration::from_millis(8)) {
        Ok(mut dl) => {
          while let Ok(newer) = rx.try_recv() {
            dl = newer;
          }
          let info = FrameInfo {
            frame,
            frame_time: Instant::now(),
            scale: current_scale,
            size: current_size,
            safe_area: current_safe_area,
          };
          let mut builder = DisplayListBuilder::new(None);
          builder.scale(current_scale, current_scale);
          (hooks.pre_render)(&mut builder, &info);
          builder.draw_display_list(&dl, 1.0);
          (hooks.post_render)(&mut builder, &info);
          if let Some(scaled) = builder.build() {
            render_surface
              .draw_display_list(&scaled)
              .expect("Failed to draw display list");
          }
          render_surface.present();
          event_tx.send(AlloyEvent::FrameRendered { frame }).ok();
          frame += 1;
        }
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
      }
      for sdl_event in event_pump.poll_iter() {
        if let Some(e) = translate_event(sdl_event, &window) {
          apply_main_thread_effects(&e, &mut render_surface, &mut current_scale, &mut current_size, &mut current_safe_area);
          event_tx.send(e).ok();
        }
      }
      while let Ok(cmd) = cmd_rx.try_recv() {
        match cmd {
          AlloyCommand::EmitInitEvents => {
            let e = current_resize_event(&window);
            apply_main_thread_effects(&e, &mut render_surface, &mut current_scale, &mut current_size, &mut current_safe_area);
            event_tx.send(e).ok();
          }
        }
      }
    }
  }
}