use impellers::{Color, Context, DisplayList, DisplayListBuilder, ISize, Paint, PixelFormat, Point, Rect, Size};
use sdl3::event::Event;
use sdl3::video::GLProfile;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

fn draw(mut builder: DisplayListBuilder, _w: u32, _h: u32) -> DisplayList {
    let rect = Rect::new(Point::new(100.0, 100.0), Size::new(200.0, 200.0));
    let mut paint = Paint::default();
    paint.set_color(Color::new_srgba(1.0, 0.0, 0.0, 1.0));
    builder.draw_rect(&rect, &paint);
    builder.build().expect("Failed to build display list")
}

fn spawn_ui_thread(w: u32, h: u32) -> mpsc::Receiver<DisplayList> {
    let (tx, rx) = mpsc::channel();

    let _builder_thread = thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
        rt.block_on(async {
            loop {
                let builder = DisplayListBuilder::new(None);
                let dl = draw(builder, w, h);

                // Send display list (exit if receiver hung up)
                if tx.send(dl).is_err() {
                    break;
                }

                // Rebuild at ~60 FPS
                tokio::time::sleep(Duration::from_millis(16)).await;
            }
        });
    });

    rx
}

fn main() {
    // ----- setup --------------------
    let sdl_context = sdl3::init().expect("Failed to initialize SDL3");

    sdl3::hint::set("SDL_OPENGL_ES_DRIVER", "1");

    let video = sdl_context.video().expect("Failed to get video subsystem");
    let gl_attr = video.gl_attr();
    gl_attr.set_context_profile(GLProfile::GLES);
    gl_attr.set_context_version(3, 0);

    let window = video
        .window("wgpu test", 1200, 800)
        .position_centered()
        .opengl()
        .resizable()
        .high_pixel_density()
        .build()
        .expect("Failed to create window");

    let gl_context = window
        .gl_create_context()
        .expect("Failed to create GL context");
    window
        .gl_make_current(&gl_context)
        .expect("Failed to make GL context current");
    video
        .gl_set_swap_interval(sdl3::video::SwapInterval::VSync)
        .expect("Failed to set swap interval");

    let mut itx = unsafe {
        Context::new_opengl_es(|name| {
            video
                .gl_get_proc_address(name)
                .map(|f| f as *mut _)
                .unwrap_or(std::ptr::null_mut())
        })
    }
    .expect("Failed to create Impeller context");

    let (w, h) = window.size_in_pixels();

    let mut surface =
        unsafe { itx.wrap_fbo(0, PixelFormat::RGBA8888, ISize::new(w as i64, h as i64)) }
            .expect("Failed to wrap framebuffer");

    let rx = spawn_ui_thread(w, h);

    // ----- main --------------------

    let mut current_dl = rx.recv().expect("Failed to receive initial display list");
    let mut event_pump = sdl_context.event_pump().expect("Failed to get event pump");

    'running: loop {
        // Try to get latest display list (non-blocking)
        if let Ok(new_dl) = rx.try_recv() {
            current_dl = new_dl;
        }

        surface
            .draw_display_list(&current_dl)
            .expect("Failed to draw display list");

        window.gl_swap_window();

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
