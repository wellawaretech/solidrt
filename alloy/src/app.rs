use impellers::{DisplayList, DisplayListBuilder, ISize};
use std::sync::{mpsc, Arc};

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

pub struct RenderHooks {
  pub pre_render: Box<dyn FnMut()>,
  pub post_render: Box<dyn FnMut()>,
}

fn apply_main_thread_effects(
  event: &AlloyEvent,
  render_surface: &mut Box<dyn RenderSurface>,
  current_scale: &mut f32,
) {
  if let AlloyEvent::Resize { size, display_scale, .. } = event {
    let phys = ISize::new(
      (size.width as f32 * display_scale) as i64,
      (size.height as f32 * display_scale) as i64,
    );
    render_surface.resize(phys);
    *current_scale = *display_scale;
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

    let initial = current_resize_event(&window);
    apply_main_thread_effects(&initial, &mut render_surface, &mut current_scale);
    event_tx.send(initial).ok();

    loop {
      match rx.recv_timeout(std::time::Duration::from_millis(8)) {
        Ok(mut dl) => {
          while let Ok(newer) = rx.try_recv() {
            dl = newer;
          }
          (hooks.pre_render)();
          let mut builder = DisplayListBuilder::new(None);
          builder.scale(current_scale, current_scale);
          builder.draw_display_list(&dl, 1.0);
          if let Some(scaled) = builder.build() {
            render_surface
              .draw_display_list(&scaled)
              .expect("Failed to draw display list");
          }
          render_surface.present();
          event_tx.send(AlloyEvent::FrameRendered { frame }).ok();
          frame += 1;
          (hooks.post_render)();
        }
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
      }
      for sdl_event in event_pump.poll_iter() {
        if let Some(e) = translate_event(sdl_event, &window) {
          apply_main_thread_effects(&e, &mut render_surface, &mut current_scale);
          event_tx.send(e).ok();
        }
      }
      while let Ok(cmd) = cmd_rx.try_recv() {
        match cmd {
          AlloyCommand::EmitInitEvents => {
            let e = current_resize_event(&window);
            apply_main_thread_effects(&e, &mut render_surface, &mut current_scale);
            event_tx.send(e).ok();
          }
        }
      }
    }
  }
}