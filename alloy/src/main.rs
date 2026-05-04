mod display;

use impellers::{Color, DisplayList, DisplayListBuilder, Paint, Point, Rect, Size, TextureSampling};
use sdl3::event::Event;
use std::time::Duration;

static GPU_TEXTURE: std::sync::OnceLock<display::GpuTexture> = std::sync::OnceLock::new();
// Adopted once; kept alive so Impeller never calls glDeleteTextures while wgpu still owns the GL object.
static IMPELLER_TEXTURE: std::sync::OnceLock<impellers::Texture> = std::sync::OnceLock::new();

fn make_blue_pixels(width: u32, height: u32) -> Vec<u8> {
    let mut pixels = vec![0u8; (width * height * 4) as usize];
    for i in (0..pixels.len()).step_by(4) {
        pixels[i] = 51;
        pixels[i + 1] = 77;
        pixels[i + 2] = 128;
        pixels[i + 3] = 255;
    }
    pixels
}

fn draw(mut builder: DisplayListBuilder, gpu_ctx: Option<&display::GpuContext>) -> DisplayList {
    // Draw a red rectangle
    let rect = Rect::new(Point::new(100.0, 100.0), Size::new(200.0, 200.0));
    let mut paint = Paint::default();
    paint.set_color(Color::new_srgba(1.0, 0.0, 0.0, 1.0));
    builder.draw_rect(&rect, &paint);

    if let Some(ctx) = gpu_ctx {
        let (w, h) = (256u32, 256u32);
        let pixels = make_blue_pixels(w, h);

        let texture = GPU_TEXTURE.get_or_init(|| {
            display::GpuTexture::new(&ctx.wgpu_device, ctx.backend, w, h)
        });
        texture.upload(&ctx.wgpu_device, &ctx.wgpu_queue, &pixels, w, h);

        let tex = IMPELLER_TEXTURE.get_or_init(|| {
            ctx.adopt_texture(texture, w, h).expect("adopt texture failed")
        });
        let src_rect = Rect::new(Point::new(0.0, 0.0), Size::new(w as f32, h as f32));
        let dst_rect = Rect::new(Point::new(10.0, 10.0), Size::new(w as f32, h as f32));
        builder.draw_texture_rect(tex, &src_rect, &dst_rect, TextureSampling::Linear, Some(&Paint::default()));
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
