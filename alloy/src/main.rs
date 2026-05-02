mod display;

use impellers::{Color, DisplayList, DisplayListBuilder, Paint, Point, Rect, Size, TextureSampling};
use sdl3::event::Event;
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

        ctx.render_to_texture(texture, 256, 256);

        if let Some(tex) = ctx.adopt_texture(texture, 256, 256) {
            let src_rect = Rect::new(Point::new(0.0, 0.0), Size::new(256.0, 256.0));
            let dst_rect = Rect::new(Point::new(10.0, 10.0), Size::new(256.0, 256.0));
            builder.draw_texture_rect(&tex, &src_rect, &dst_rect, TextureSampling::Linear, Some(&Paint::default()));
        }
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
        eprintln!("[UI thread] Starting display list generation loop");
        loop {
            let builder = DisplayListBuilder::new(None);
            let dl = draw(builder, Some(gpu_ctx));

            if tx.send(dl).is_err() {
                break;
            }

            std::thread::sleep(Duration::from_millis(16));
        }
    });

    // ----- main --------------------

    let mut current_dl = rx.recv().expect("Failed to receive initial display list");
    let mut event_pump = sdl_context.event_pump().expect("Failed to get event pump");

    'running: loop {
        // Drain all pending display lists, keep the latest
        while let Ok(new_dl) = rx.try_recv() {
            current_dl = new_dl;
        }

        render_surface
            .draw_display_list(&current_dl)
            .expect("Failed to draw display list");

        render_surface.present();

        // Wait for events with timeout — properly yields to the OS so the
        // system can detect idle and enter sleep mode.
        // The timeout ensures we still pick up new display lists from the channel.
        loop {
            match event_pump.wait_event_timeout(Duration::from_millis(100)) {
                Some(Event::Quit { .. }) => break 'running,
                Some(_) => continue,
                None => break,
            }
        }
    }
}
