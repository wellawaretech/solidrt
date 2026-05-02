use impellers::{Context as ImpellerContext, DisplayList, DisplayListBuilder, ISize, PixelFormat, Rect, Point, Size, TextureSampling, Paint};
use crate::display::{GpuTexture, RenderSurface, DisplayContext};

pub fn create_ui_pbuffer(
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

pub fn make_current(
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

pub fn create_gpu_context() -> crate::display::GpuContext {
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
        ImpellerContext::new_opengl_es(|name| {
            sdl3::sys::video::SDL_GL_GetProcAddress(
                name.as_ptr() as *const _,
            )
            .map(|f| f as *mut _)
            .unwrap_or(std::ptr::null_mut())
        })
    }
    .expect("Failed to create Impeller context on UI thread");
    eprintln!("[UI thread] Impeller context created");

    crate::display::GpuContext::new(crate::display::Backend::Gl, device, queue, impeller_ctx)
}

/// Extract the GL texture name from a wgpu texture (GL backend only).
fn wgpu_texture_gl_handle(texture: &wgpu::Texture) -> u32 {
    let hal_texture = unsafe { texture.as_hal::<wgpu::hal::gles::Api>() }
        .expect("not a GL-backed wgpu texture");
    match hal_texture.inner {
        wgpu::hal::gles::TextureInner::Texture { raw, .. } => raw.0.get() as u32,
        _ => panic!("wgpu texture is not a GL texture"),
    }
}

/// Adopt a wGPU GL texture into Impeller's display list (zero-copy).
pub fn texture_to_display_list(
    gpu_texture: &GpuTexture,
    impeller_ctx: &ImpellerContext,
    builder: &mut DisplayListBuilder,
    width: u32,
    height: u32,
) {
    let gl_handle = wgpu_texture_gl_handle(&gpu_texture.wgpu_texture);

    // Adopt the GL texture into Impeller (zero-copy)
    // Format code 1 = BGRA8888 (or RGBA8888 on little-endian systems)
    let impeller_texture = unsafe {
        impeller_ctx.adopt_opengl_texture(width, height, 1, gl_handle as u64)
    };

    if let Some(tex) = impeller_texture {
        let src_rect = Rect::new(Point::new(0.0, 0.0), Size::new(width as f32, height as f32));
        let dst_rect = Rect::new(Point::new(10.0, 10.0), Size::new(256.0, 256.0));
        builder.draw_texture_rect(&tex, &src_rect, &dst_rect, TextureSampling::Linear, Some(&Paint::default()));
    }
}

pub struct GlSurface {
    ctx: ImpellerContext,
    surface: impellers::Surface,
    window_raw: usize,
}

impl GlSurface {
    pub fn create(
        window: &sdl3::video::Window,
        width: u32,
        height: u32,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let mut ctx = unsafe {
            // Use SDL's FFI directly to get GL proc addresses
            ImpellerContext::new_opengl_es(|name| {
                let cname = match std::ffi::CString::new(name) {
                    Ok(s) => s,
                    Err(_) => return std::ptr::null_mut(),
                };
                sdl3::sys::video::SDL_GL_GetProcAddress(cname.as_ptr())
                    .map(|f| f as *mut std::ffi::c_void)
                    .unwrap_or(std::ptr::null_mut())
            })
        }
        .map_err(|_| Box::new(std::io::Error::other("Failed to create OpenGL ES context")) as Box<dyn std::error::Error>)?;

        let surface = unsafe {
            ctx.wrap_fbo(0, PixelFormat::RGBA8888, ISize::new(width as i64, height as i64))
        }
        .ok_or_else(|| Box::new(std::io::Error::other("Failed to wrap framebuffer")) as Box<dyn std::error::Error>)?;

        Ok(GlSurface {
            ctx,
            surface,
            window_raw: window.raw() as usize,
        })
    }
}

impl RenderSurface for GlSurface {
    fn draw_display_list(&mut self, dl: &DisplayList) -> Result<(), Box<dyn std::error::Error>> {
        self.surface.draw_display_list(dl)
            .map_err(|_| Box::new(std::io::Error::other("Failed to draw display list")) as Box<dyn std::error::Error>)
    }

    fn present(&mut self) {
        unsafe { sdl3::sys::video::SDL_GL_SwapWindow(self.window_raw as *mut _); }
    }

    fn resize(&mut self, width: u32, height: u32) {
        self.surface = unsafe {
            self.ctx
                .wrap_fbo(0, PixelFormat::RGBA8888, ISize::new(width as i64, height as i64))
        }
        .expect("Failed to resize GL surface");
    }
}

pub(crate) fn setup_opengl_platform<T>(
    video: &T,
    window: &sdl3::video::Window,
) -> Result<DisplayContext, Box<dyn std::error::Error>> {
    // SAFETY: T is sdl3::VideoSubsystem at the call site; casting is sound
    // since we're just reinterpreting and using the reference in this same scope.
    let video = unsafe { &*(video as *const T as *const sdl3::VideoSubsystem) };

    // Set SDL hints for OpenGL ES via FFI
    sdl3::hint::set("SDL_OPENGL_ES_DRIVER", "1");

    // Configure GL attributes BEFORE creating contexts
    let gl_attr = video.gl_attr();
    gl_attr.set_context_profile(sdl3::video::GLProfile::GLES);
    gl_attr.set_context_version(3, 0);

    // Create UI GL context
    let ui_context = window
        .gl_create_context()
        .map_err(|e| format!("Failed to create UI GL context: {}", e))?;

    // Enable context sharing for main GL context
    gl_attr.set_share_with_current_context(true);

    // Create main GL context
    let main_context = window
        .gl_create_context()
        .map_err(|e| format!("Failed to create main GL context: {}", e))?;

    // Make main context current on the render thread
    window
        .gl_make_current(&main_context)
        .map_err(|e| format!("Failed to make main GL context current: {}", e))?;

    // Set swap interval (vsync) via FFI
    unsafe {
        sdl3::sys::video::SDL_GL_SetSwapInterval(1);
    }

    Ok(DisplayContext::Gl {
        video_opaque: std::ptr::null(),
        window_opaque: window as *const _ as *const std::ffi::c_void,
        main_context,
        ui_context,
    })
}
