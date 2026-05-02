mod display;

use impellers::{Color, Context, DisplayList, DisplayListBuilder, Paint, Point, Rect, Size};
use sdl3::event::Event;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

// Generic wrapper to make non-Send types safe for thread boundaries
// Safe because we ensure proper synchronization (GL context binding, etc.)
#[repr(transparent)]
struct SendableHandle<T>(T);
unsafe impl<T> Send for SendableHandle<T> {}
unsafe impl<T> Sync for SendableHandle<T> {}

// Wrapper to make raw pointers sendable between threads (safe because they're opaque handles)
struct SendablePtr(*mut std::ffi::c_void);
unsafe impl Send for SendablePtr {}
unsafe impl Sync for SendablePtr {}

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

fn create_ui_pbuffer(
    display: *mut std::ffi::c_void,
    gl_context: *mut std::ffi::c_void,
) -> *mut std::ffi::c_void {
    const EGL_NONE: i32 = 0x3038;
    const EGL_CONFIG_ID: i32 = 0x3028;
    const EGL_WIDTH: i32 = 0x3057;
    const EGL_HEIGHT: i32 = 0x3056;

    type EglQueryContextFn = extern "C" fn(
        *mut std::ffi::c_void, *mut std::ffi::c_void, i32, *mut i32,
    ) -> u32;
    type EglChooseConfigFn = extern "C" fn(
        *mut std::ffi::c_void, *const i32, *mut *mut std::ffi::c_void, i32, *mut i32,
    ) -> u32;
    type EglCreatePbufferFn = extern "C" fn(
        *mut std::ffi::c_void, *mut std::ffi::c_void, *const i32,
    ) -> *mut std::ffi::c_void;

    unsafe {
        let egl_query_context: EglQueryContextFn = std::mem::transmute(
            sdl3::sys::video::SDL_EGL_GetProcAddress(c"eglQueryContext".as_ptr()).unwrap()
        );
        let egl_choose_config: EglChooseConfigFn = std::mem::transmute(
            sdl3::sys::video::SDL_EGL_GetProcAddress(c"eglChooseConfig".as_ptr()).unwrap()
        );
        let egl_create_pbuffer: EglCreatePbufferFn = std::mem::transmute(
            sdl3::sys::video::SDL_EGL_GetProcAddress(c"eglCreatePbufferSurface".as_ptr()).unwrap()
        );

        let mut config_id: i32 = 0;
        let r = egl_query_context(display, gl_context, EGL_CONFIG_ID, &mut config_id);
        assert!(r != 0, "eglQueryContext(EGL_CONFIG_ID) failed");

        let select = [EGL_CONFIG_ID, config_id, EGL_NONE];
        let mut config: *mut std::ffi::c_void = std::ptr::null_mut();
        let mut num_configs: i32 = 0;
        let r = egl_choose_config(display, select.as_ptr(), &mut config, 1, &mut num_configs);
        assert!(r != 0 && num_configs > 0 && !config.is_null(), "eglChooseConfig failed");

        let pb_attribs = [EGL_WIDTH, 1, EGL_HEIGHT, 1, EGL_NONE];
        let pbuffer = egl_create_pbuffer(display, config, pb_attribs.as_ptr());
        assert!(!pbuffer.is_null(), "eglCreatePbufferSurface failed");
        pbuffer
    }
}

fn make_current(
    display: *mut std::ffi::c_void,
    surface: *mut std::ffi::c_void,
    gl_context: *mut std::ffi::c_void,
) {
    let egl_make_current: extern "C" fn(
        *mut std::ffi::c_void,
        *mut std::ffi::c_void,
        *mut std::ffi::c_void,
        *mut std::ffi::c_void,
    ) -> u32 = unsafe {
        std::mem::transmute(
            sdl3::sys::video::SDL_EGL_GetProcAddress(c"eglMakeCurrent".as_ptr()).unwrap()
        )
    };
    let result = egl_make_current(display, surface, surface, gl_context);
    assert!(result != 0, "eglMakeCurrent failed on UI thread");
}

fn ui_thread_main(gl_context_ptr: SendablePtr, tx: mpsc::Sender<DisplayList>) {
    let gl_context_ptr = gl_context_ptr.0;
    // Setup EGL on UI thread
    let egl_display = unsafe { sdl3::sys::video::SDL_EGL_GetCurrentDisplay() };
    assert!(!egl_display.is_null(), "no EGL display");
    eprintln!("[UI thread] EGL display obtained");

    let ui_pbuffer = create_ui_pbuffer(egl_display, gl_context_ptr);
    make_current(egl_display, ui_pbuffer, gl_context_ptr);
    eprintln!("[UI thread] GL context made current on pbuffer");

    // Create wGPU using the existing GL context
    let (device, queue) = unsafe {
        use wgpu::hal::gles;

        let hal_exposed = gles::Adapter::new_external(
            |name| {
                let cname = std::ffi::CString::new(name).unwrap();
                sdl3::sys::video::SDL_GL_GetProcAddress(cname.as_ptr())
                    .map(|f| f as *const std::ffi::c_void)
                    .unwrap_or(std::ptr::null())
            },
            wgpu::GlBackendOptions::default(),
        )
        .expect("Failed to create wgpu GL adapter on UI thread");

        let wgpu_instance = wgpu::Instance::new({
            let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
            desc.backends = wgpu::Backends::GL;
            desc
        });

        let adapter = wgpu_instance.create_adapter_from_hal(hal_exposed);

        pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("ui-thread"),
                required_features: wgpu::Features::empty(),
                required_limits: adapter.limits(),
                memory_hints: wgpu::MemoryHints::MemoryUsage,
                ..Default::default()
            },
        ))
        .expect("Failed to create wgpu device on UI thread")
    };
    eprintln!("[UI thread] wGPU device created");

    // Create Impeller context on UI thread
    let impeller_ctx = unsafe {
        Context::new_opengl_es(|name| {
            sdl3::sys::video::SDL_GL_GetProcAddress(
                name.as_ptr() as *const _,
            )
            .map(|f| f as *mut _)
            .unwrap_or(std::ptr::null_mut())
        })
    }
    .expect("Failed to create Impeller context on UI thread");
    eprintln!("[UI thread] Impeller context created");

    let gpu_ctx = display::GpuContext::new(display::Backend::Gl, device, queue, impeller_ctx);

    let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
    rt.block_on(async {
        eprintln!("[UI thread] Starting display list generation loop");
        loop {
            let builder = DisplayListBuilder::new(None);
            let dl = draw(builder, Some(&gpu_ctx));

            // Send display list to main thread (exit if receiver hung up)
            if tx.send(dl).is_err() {
                break;
            }

            // Rebuild at ~60 FPS
            tokio::time::sleep(Duration::from_millis(16)).await;
        }
    });
}

fn spawn_ui_thread(
    _w: u32,
    _h: u32,
    ui_gl_context: &sdl3::video::GLContext,
) -> mpsc::Receiver<DisplayList> {
    let (tx, rx) = mpsc::channel();

    // Store the raw pointer before moving into the thread
    let gl_context_ptr = SendablePtr(unsafe { std::mem::transmute_copy::<_, *mut std::ffi::c_void>(ui_gl_context) });
    let _builder_thread = thread::spawn(move || {
        ui_thread_main(gl_context_ptr, tx);
    });

    rx
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
    let rx = spawn_ui_thread(w, h, platform.ui_context());

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
