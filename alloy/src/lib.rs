pub mod display;

pub use display::{GpuContext, RenderSurface, TextureEntry};

use impellers::{DisplayList, DisplayListBuilder, ISize};
use sdl3::event::Event;
use std::time::Duration;

pub fn run(
    title: &str,
    size: ISize,
    mut build: impl FnMut(DisplayListBuilder, &GpuContext) -> DisplayList + Send + 'static,
    mut render: impl FnMut(&mut dyn RenderSurface, &DisplayList),
) {
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

    let rx = display::setup_ui_thread(&platform, move |gpu_ctx, tx| {
        loop {
            let builder = DisplayListBuilder::new(None);
            let dl = build(builder, gpu_ctx);
            if tx.send(dl).is_err() {
                break;
            }
            std::thread::sleep(Duration::from_millis(16));
        }
    });

    let (w, h) = window.size_in_pixels();
    let window_size = ISize::new(w as i64, h as i64);
    let mut render_surface = display::create_render_surface(&platform, window_size)
        .expect("Failed to create render surface");

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
