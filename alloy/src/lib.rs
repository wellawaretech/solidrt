pub mod display;

pub use display::{GpuContext, RenderSurface, TextureEntry};

use impellers::{DisplayList, ISize};
use sdl3::event::Event;
use std::time::Duration;

pub struct App {
    sdl_context: sdl3::Sdl,
    // Kept alive because DisplayContext stores a raw pointer into it.
    _window: sdl3::video::Window,
    platform: display::DisplayContext,
    render_surface: Box<dyn RenderSurface>,
}

pub fn setup(title: &str, size: ISize) -> App {
    let (width, height) = (size.width as u32, size.height as u32);
    let sdl_context = sdl3::init().expect("Failed to initialize SDL3");
    let video = sdl_context.video().expect("Failed to get video subsystem");

    let window = video
        .window(title, width, height)
        .opengl()
        .position_centered()
        .resizable()
        .high_pixel_density()
        .build()
        .expect("Failed to create window");

    let platform =
        display::DisplayContext::new_opengl(&video, &window).expect("Failed to set up platform");

    let (w, h) = window.size_in_pixels();
    let window_size = ISize::new(w as i64, h as i64);
    let render_surface = display::create_render_surface(&platform, window_size)
        .expect("Failed to create render surface");

    App { sdl_context, _window: window, platform, render_surface }
}

impl App {
    pub fn run(
        self,
        ui: impl FnOnce(&GpuContext) + Send + 'static,
        mut render: impl FnMut(&mut dyn RenderSurface, &DisplayList),
    ) {
        let App { sdl_context, _window, platform, mut render_surface } = self;

        let rx = display::setup_ui_thread(&platform, ui);

        let mut current_dl = rx.recv().expect("Failed to receive initial display list");
        let mut event_pump = sdl_context.event_pump().expect("Failed to get event pump");

        'running: loop {
            while let Ok(new_dl) = rx.try_recv() {
                current_dl = new_dl;
            }

            render(render_surface.as_mut(), &current_dl);

            loop {
                match event_pump.wait_event_timeout(Duration::from_millis(100)) {
                    Some(Event::Quit { .. }) => break 'running,
                    Some(_) => continue,
                    None => break,
                }
            }
        }
    }
}
