mod display;

use impellers::{Color, DisplayList, DisplayListBuilder, Paint, Point, Rect, Size, ISize, TextureSampling};
use sdl3::event::Event;
use std::time::Duration;

fn make_pixels(size: ISize, color: u32) -> Vec<u8> {
    let bytes = color.to_be_bytes();
    let mut pixels = vec![0u8; (size.width * size.height * 4) as usize];
    for chunk in pixels.chunks_exact_mut(4) {
        chunk.copy_from_slice(&bytes);
    }
    pixels
}

fn draw(mut builder: DisplayListBuilder, ctx: &display::GpuContext) -> DisplayList {
    let size = ISize::new(256, 256);
    let src_rect = Rect::new(Point::new(0.0, 0.0), size.cast());

    const BLUE_TEX: u64 = 1;
    let entry = ctx.get_or_create_texture(BLUE_TEX, size, || make_pixels(size, 0x334D80FF));
    let dst_rect = Rect::new(Point::new(10.0, 10.0), size.cast());
    builder.draw_texture_rect(&entry.impeller, &src_rect, &dst_rect, TextureSampling::Linear, None);

    let rect = Rect::new(Point::new(100.0, 100.0), Size::new(200.0, 200.0));
    let mut paint = Paint::default();
    paint.set_color(Color::new_srgba(1.0, 0.0, 0.0, 1.0));
    builder.draw_rect(&rect, &paint);

    const GREEN_TEX: u64 = 2;
    let entry = ctx.get_or_create_texture(GREEN_TEX, size, || make_pixels(size, 0x4D8033FF));
    let dst_rect = Rect::new(Point::new(280.0, 10.0), size.cast());
    builder.draw_texture_rect(&entry.impeller, &src_rect, &dst_rect, TextureSampling::Linear, None);

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
    let window_size = ISize::new(w as i64, h as i64);

    let mut render_surface = display::create_render_surface(&platform, window_size)
        .expect("Failed to create render surface");

    // Spawn UI thread (creates wGPU device and queue there)
    let rx = display::setup_ui_thread(&platform, |gpu_ctx, tx| {
        eprintln!("[UI thread] Starting display list generation loop");
        loop {
            let builder = DisplayListBuilder::new(None);
            let dl = draw(builder, gpu_ctx);

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
