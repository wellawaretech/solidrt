mod display;

use impellers::{Color, DisplayList, DisplayListBuilder, Paint, Point, Rect, Size};
use sdl3::event::Event;
use std::thread;
use std::time::Duration;

// Static texture created once and reused across frames
static GPU_TEXTURE: std::sync::OnceLock<display::GpuTexture> = std::sync::OnceLock::new();

fn draw(mut builder: DisplayListBuilder, gpu_ctx: Option<&display::GpuContext>) -> DisplayList {
    // Draw a red rectangle
    let rect = Rect::new(Point::new(100.0, 100.0), Size::new(200.0, 200.0));
    let mut paint = Paint::default();
    paint.set_color(Color::new_srgba(1.0, 0.0, 0.0, 1.0));
    builder.draw_rect(&rect, &paint);

    // Create texture once and reuse it, updating pixel data each frame
    if let Some(ctx) = gpu_ctx {
        let texture = GPU_TEXTURE.get_or_init(|| {
            let tex = ctx.create_texture(256, 256);
            ctx.render_to_texture(&tex, 256, 256);
            tex
        });

        // Update texture data each frame (in case we want to animate it later)
        ctx.render_to_texture(texture, 256, 256);
        ctx.texture_to_display_list(texture, &mut builder, 256, 256);
    }

    builder.build().expect("Failed to build display list")
}



fn main() {
    // ----- setup --------------------
    let sdl_context = sdl3::init().expect("Failed to initialize SDL3");

    let video = sdl_context.video().expect("Failed to get video subsystem");

    let window = video
        .window("wgpu test", 1200, 800)
        .opengl()
        .position_centered()
        .resizable()
        .high_pixel_density()
        .build()
        .expect("Failed to create window");

    // Platform setup handles all GL context creation and configuration
    let platform =
        display::DisplayContext::new_opengl(&video, &window).expect("Failed to set up platform");

    let (w, h) = window.size_in_pixels();

    let mut render_surface = display::create_render_surface(&platform, w, h)
        .expect("Failed to create render surface");

    // Spawn UI thread (creates wGPU device and queue there)
    let rx = display::setup_ui_thread(&platform, |gpu_ctx, tx| {
        let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
        rt.block_on(async {
            eprintln!("[UI thread] Starting display list generation loop");
            loop {
                let builder = DisplayListBuilder::new(None);
                let dl = draw(builder, Some(gpu_ctx));

                if tx.send(dl).is_err() {
                    break;
                }

                tokio::time::sleep(Duration::from_millis(16)).await;
            }
        });
    });

    // ----- main --------------------

    let mut current_dl = rx.recv().expect("Failed to receive initial display list");
    let mut event_pump = sdl_context.event_pump().expect("Failed to get event pump");

    'running: loop {
        // Try to get latest display list (non-blocking)
        if let Ok(new_dl) = rx.try_recv() {
            current_dl = new_dl;
        }

        render_surface
            .draw_display_list(&current_dl)
            .expect("Failed to draw display list");

        render_surface.present();

        for event in event_pump.poll_iter() {
            match event {
                Event::Quit { .. } => {
                    break 'running;
                }
                _ => {}
            }
        }

        thread::sleep(Duration::from_millis(10));
    }
}
