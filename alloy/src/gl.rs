use crate::{Backend, Context, DisplayContext, GpuTexture, RenderSurface};
use impellers::{Context as ImpellerContext, DisplayList, ISize, PixelFormat, Texture};
use sdl3::video::SwapInterval;
use std::sync::{mpsc, Arc};

struct SendablePtr(*mut std::ffi::c_void);
unsafe impl Send for SendablePtr {}

pub fn create_ui_pbuffer(display: *mut std::ffi::c_void, gl_context: *mut std::ffi::c_void) -> *mut std::ffi::c_void {
  const EGL_NONE: i32 = 0x3038;
  const EGL_CONFIG_ID: i32 = 0x3028;
  const EGL_WIDTH: i32 = 0x3057;
  const EGL_HEIGHT: i32 = 0x3056;

  type EglQueryContextFn = extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void, i32, *mut i32) -> u32;
  type EglChooseConfigFn =
    extern "C" fn(*mut std::ffi::c_void, *const i32, *mut *mut std::ffi::c_void, i32, *mut i32) -> u32;
  type EglCreatePbufferFn =
    extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void, *const i32) -> *mut std::ffi::c_void;

  unsafe {
    let egl_query_context: EglQueryContextFn =
      std::mem::transmute(sdl3::sys::video::SDL_EGL_GetProcAddress(c"eglQueryContext".as_ptr()).unwrap());
    let egl_choose_config: EglChooseConfigFn =
      std::mem::transmute(sdl3::sys::video::SDL_EGL_GetProcAddress(c"eglChooseConfig".as_ptr()).unwrap());
    let egl_create_pbuffer: EglCreatePbufferFn =
      std::mem::transmute(sdl3::sys::video::SDL_EGL_GetProcAddress(c"eglCreatePbufferSurface".as_ptr()).unwrap());

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

pub fn make_current(display: *mut std::ffi::c_void, surface: *mut std::ffi::c_void, gl_context: *mut std::ffi::c_void) {
  let egl_make_current: extern "C" fn(
    *mut std::ffi::c_void,
    *mut std::ffi::c_void,
    *mut std::ffi::c_void,
    *mut std::ffi::c_void,
  ) -> u32 =
    unsafe { std::mem::transmute(sdl3::sys::video::SDL_EGL_GetProcAddress(c"eglMakeCurrent".as_ptr()).unwrap()) };
  let result = egl_make_current(display, surface, surface, gl_context);
  assert!(result != 0, "eglMakeCurrent failed on UI thread");
}

pub fn create_wgpu_device() -> (wgpu::Device, wgpu::Queue) {
  use wgpu::hal::gles;

  let hal_exposed = unsafe {
    gles::Adapter::new_external(
      |name| {
        let cname = std::ffi::CString::new(name).expect("GL proc name contains null byte");
        sdl3::sys::video::SDL_GL_GetProcAddress(cname.as_ptr())
          .map(|f| f as *const std::ffi::c_void)
          .unwrap_or(std::ptr::null())
      },
      wgpu::GlBackendOptions::default(),
    )
    .expect("Failed to create wgpu GL adapter")
  };

  let wgpu_instance = wgpu::Instance::new({
    let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
    desc.backends = wgpu::Backends::GL;
    desc
  });

  let adapter = unsafe { wgpu_instance.create_adapter_from_hal(hal_exposed) };

  pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
    label: Some("ui-thread"),
    required_features: wgpu::Features::empty(),
    required_limits: adapter.limits(),
    memory_hints: wgpu::MemoryHints::MemoryUsage,
    ..Default::default()
  }))
  .expect("Failed to create wgpu device")
}

pub fn create_impeller_context() -> ImpellerContext {
  unsafe {
    ImpellerContext::new_opengl_es(|name| {
      sdl3::sys::video::SDL_GL_GetProcAddress(name.as_ptr() as *const _)
        .map(|f| f as *mut _)
        .unwrap_or(std::ptr::null_mut())
    })
  }
  .expect("Failed to create Impeller context")
}

/// Extract the GL texture name from a wgpu texture (GL backend only).
fn wgpu_texture_gl_handle(texture: &wgpu::Texture) -> u32 {
  let hal_texture = unsafe { texture.as_hal::<wgpu::hal::gles::Api>() }.expect("not a GL-backed wgpu texture");
  match hal_texture.inner {
    wgpu::hal::gles::TextureInner::Texture { raw, .. } => raw.0.get() as u32,
    _ => panic!("wgpu texture is not a GL texture"),
  }
}

/// Adopt a wGPU GL texture into Impeller (zero-copy).
pub fn adopt_texture(gpu_texture: &GpuTexture, impeller_ctx: &ImpellerContext, size: ISize) -> Option<Texture> {
  let (width, height) = (size.width as u32, size.height as u32);
  let gl_handle = wgpu_texture_gl_handle(&gpu_texture.wgpu_texture);

  unsafe { impeller_ctx.adopt_opengl_texture(width, height, 1, gl_handle as u64) }
}

// GLES 3.0 constants for offscreen rasterization and readback.
const GL_FRAMEBUFFER: u32 = 0x8D40;
const GL_COLOR_ATTACHMENT0: u32 = 0x8CE0;
const GL_TEXTURE_2D: u32 = 0x0DE1;
const GL_FRAMEBUFFER_COMPLETE: u32 = 0x8CD5;
const GL_FRAMEBUFFER_BINDING: u32 = 0x8CA6;
const GL_TEXTURE_BINDING_2D: u32 = 0x8069;
const GL_RGBA: u32 = 0x1908;
const GL_RGBA8: u32 = 0x8058;
const GL_UNSIGNED_BYTE: u32 = 0x1401;
const GL_TEXTURE_MIN_FILTER: u32 = 0x2801;
const GL_TEXTURE_MAG_FILTER: u32 = 0x2800;
const GL_LINEAR: u32 = 0x2601;
const GL_RENDERBUFFER: u32 = 0x8D41;
const GL_DEPTH24_STENCIL8: u32 = 0x88F0;
const GL_DEPTH_STENCIL_ATTACHMENT: u32 = 0x821A;
const GL_RENDERBUFFER_BINDING: u32 = 0x8CA7;
const GL_COLOR_BUFFER_BIT: u32 = 0x4000;
const GL_DEPTH_BUFFER_BIT: u32 = 0x0100;
const GL_STENCIL_BUFFER_BIT: u32 = 0x0400;

fn gl_fn(name: &std::ffi::CStr) -> unsafe extern "C" fn() {
  unsafe {
    sdl3::sys::video::SDL_GL_GetProcAddress(name.as_ptr()).unwrap_or_else(|| panic!("missing GL function {name:?}"))
  }
}

/// Rasterize a display list into a new GL texture and adopt it into Impeller,
/// which becomes the single owner of the GL name (adoption transfers handle
/// ownership per the Impeller API; creating the texture through wgpu would
/// end in a double glDeleteTextures). The calling thread must have a GL
/// context current and `impeller_ctx` must have been created on it. The
/// trailing glFinish guarantees the offscreen render has completed before the
/// texture crosses to the render thread, which samples it from a separate
/// shared GL context (where the contents are otherwise undefined).
pub fn render_display_list_to_texture(
  impeller_ctx: &mut ImpellerContext,
  dl: &DisplayList,
  size: ISize,
) -> Result<Texture, String> {
  let gen_textures: extern "C" fn(i32, *mut u32) = unsafe { std::mem::transmute(gl_fn(c"glGenTextures")) };
  let bind_texture: extern "C" fn(u32, u32) = unsafe { std::mem::transmute(gl_fn(c"glBindTexture")) };
  let tex_image_2d: extern "C" fn(u32, i32, i32, i32, i32, i32, u32, u32, *const std::ffi::c_void) =
    unsafe { std::mem::transmute(gl_fn(c"glTexImage2D")) };
  let tex_parameteri: extern "C" fn(u32, u32, i32) = unsafe { std::mem::transmute(gl_fn(c"glTexParameteri")) };
  let delete_textures: extern "C" fn(i32, *const u32) = unsafe { std::mem::transmute(gl_fn(c"glDeleteTextures")) };
  let gen_framebuffers: extern "C" fn(i32, *mut u32) = unsafe { std::mem::transmute(gl_fn(c"glGenFramebuffers")) };
  let bind_framebuffer: extern "C" fn(u32, u32) = unsafe { std::mem::transmute(gl_fn(c"glBindFramebuffer")) };
  let framebuffer_texture_2d: extern "C" fn(u32, u32, u32, u32, i32) =
    unsafe { std::mem::transmute(gl_fn(c"glFramebufferTexture2D")) };
  let check_framebuffer_status: extern "C" fn(u32) -> u32 =
    unsafe { std::mem::transmute(gl_fn(c"glCheckFramebufferStatus")) };
  let delete_framebuffers: extern "C" fn(i32, *const u32) =
    unsafe { std::mem::transmute(gl_fn(c"glDeleteFramebuffers")) };
  let gen_renderbuffers: extern "C" fn(i32, *mut u32) = unsafe { std::mem::transmute(gl_fn(c"glGenRenderbuffers")) };
  let bind_renderbuffer: extern "C" fn(u32, u32) = unsafe { std::mem::transmute(gl_fn(c"glBindRenderbuffer")) };
  let renderbuffer_storage: extern "C" fn(u32, u32, i32, i32) =
    unsafe { std::mem::transmute(gl_fn(c"glRenderbufferStorage")) };
  let framebuffer_renderbuffer: extern "C" fn(u32, u32, u32, u32) =
    unsafe { std::mem::transmute(gl_fn(c"glFramebufferRenderbuffer")) };
  let delete_renderbuffers: extern "C" fn(i32, *const u32) =
    unsafe { std::mem::transmute(gl_fn(c"glDeleteRenderbuffers")) };
  let get_integerv: extern "C" fn(u32, *mut i32) = unsafe { std::mem::transmute(gl_fn(c"glGetIntegerv")) };
  let finish: extern "C" fn() = unsafe { std::mem::transmute(gl_fn(c"glFinish")) };
  let clear_color: extern "C" fn(f32, f32, f32, f32) = unsafe { std::mem::transmute(gl_fn(c"glClearColor")) };
  let clear: extern "C" fn(u32) = unsafe { std::mem::transmute(gl_fn(c"glClear")) };

  let (width, height) = (size.width as i32, size.height as i32);

  // Some Android GPUs require render-target dimensions aligned to a tile
  // boundary; an unaligned offscreen texture/renderbuffer can come back with
  // corrupted (shifted, channel-scrambled) content. Over-allocate storage to
  // the next multiple of 64 and render into the unaligned top-left corner
  // (via the wrap_fbo viewport below), so the content itself stays at
  // (width, height) but lives in aligned backing storage.
  let align_up = |v: i32| (v + 63) & !63;
  let (alloc_width, alloc_height) = (align_up(width), align_up(height));

  // Save the current bindings so wgpu's cached GL state stays valid.
  let mut prev_fbo: i32 = 0;
  let mut prev_tex: i32 = 0;
  let mut prev_rbo: i32 = 0;
  get_integerv(GL_FRAMEBUFFER_BINDING, &mut prev_fbo);
  get_integerv(GL_TEXTURE_BINDING_2D, &mut prev_tex);
  get_integerv(GL_RENDERBUFFER_BINDING, &mut prev_rbo);

  let mut tex: u32 = 0;
  gen_textures(1, &mut tex);
  bind_texture(GL_TEXTURE_2D, tex);
  tex_image_2d(GL_TEXTURE_2D, 0, GL_RGBA8 as i32, alloc_width, alloc_height, 0, GL_RGBA, GL_UNSIGNED_BYTE, std::ptr::null());
  // No mips exist: the default MIN_FILTER references mipmaps, which would
  // make the texture sampling-incomplete (reads as black).
  tex_parameteri(GL_TEXTURE_2D, GL_TEXTURE_MIN_FILTER, GL_LINEAR as i32);
  tex_parameteri(GL_TEXTURE_2D, GL_TEXTURE_MAG_FILTER, GL_LINEAR as i32);
  bind_texture(GL_TEXTURE_2D, prev_tex as u32);

  // Impeller fills non-convex paths with stencil-then-cover and culls clips
  // via depth; without this attachment the cover pass floods the path bounds.
  let mut rbo: u32 = 0;
  gen_renderbuffers(1, &mut rbo);
  bind_renderbuffer(GL_RENDERBUFFER, rbo);
  renderbuffer_storage(GL_RENDERBUFFER, GL_DEPTH24_STENCIL8, alloc_width, alloc_height);
  bind_renderbuffer(GL_RENDERBUFFER, prev_rbo as u32);

  let mut fbo: u32 = 0;
  gen_framebuffers(1, &mut fbo);
  bind_framebuffer(GL_FRAMEBUFFER, fbo);
  framebuffer_texture_2d(GL_FRAMEBUFFER, GL_COLOR_ATTACHMENT0, GL_TEXTURE_2D, tex, 0);
  framebuffer_renderbuffer(GL_FRAMEBUFFER, GL_DEPTH_STENCIL_ATTACHMENT, GL_RENDERBUFFER, rbo);
  let status = check_framebuffer_status(GL_FRAMEBUFFER);

  let result = if status != GL_FRAMEBUFFER_COMPLETE {
    Err(format!("offscreen framebuffer incomplete: {status:#x}"))
  } else {
    // glTexImage2D(..., null) leaves the texture's initial contents
    // driver-defined. Desktop GL tends to hand back zeroed memory, but on
    // Android's tile-based GPUs it can be leftover compressed tile data from
    // unrelated content. Force a defined transparent base regardless of
    // whether Impeller's own surface clear covers an externally-built FBO.
    clear_color(0.0, 0.0, 0.0, 0.0);
    clear(GL_COLOR_BUFFER_BIT | GL_DEPTH_BUFFER_BIT | GL_STENCIL_BUFFER_BIT);

    // A wrapped-FBO surface is use-and-throw: draw once, then drop it.
    match unsafe { impeller_ctx.wrap_fbo(fbo as u64, PixelFormat::RGBA8888, size) } {
      Some(mut surface) => surface.draw_display_list(dl).map_err(|e| format!("offscreen draw failed: {e}")),
      None => Err("wrap_fbo failed for offscreen framebuffer".to_string()),
    }
  };

  bind_framebuffer(GL_FRAMEBUFFER, prev_fbo as u32);
  delete_framebuffers(1, &fbo);
  delete_renderbuffers(1, &rbo);
  // glFlush only submits commands; it does not guarantee the offscreen render
  // has completed. The render thread samples this texture from a different
  // shared GL context, where shared-object contents stay undefined until the
  // producing context's writes actually finish. glFinish blocks until the GPU
  // is done, so the texture is complete before it crosses to the render thread.
  // (Stage 1 diagnostic: a fence sync would avoid stalling the UI thread.)
  finish();

  match result {
    Ok(()) => unsafe { impeller_ctx.adopt_opengl_texture(alloc_width as u32, alloc_height as u32, 1, tex as u64) }
      .ok_or_else(|| "failed to adopt offscreen texture".to_string()),
    Err(e) => {
      delete_textures(1, &tex);
      Err(e)
    }
  }
}

/// Read back an Impeller GL texture's RGBA8 pixels by attaching its handle to
/// a temporary framebuffer and calling glReadPixels. Returns memory-order rows
/// (row 0 first), which is image top-to-bottom for every texture alloy
/// produces. Goes through raw GL rather than wgpu: wgpu's lazy
/// zero-initialization tracking does not know about Impeller's writes, so a
/// wgpu copy from an offscreen-rendered texture would return (or even clear
/// to) zeros.
pub fn read_texture_pixels(texture: &Texture, size: ISize) -> Result<Vec<u8>, String> {
  let gen_framebuffers: extern "C" fn(i32, *mut u32) = unsafe { std::mem::transmute(gl_fn(c"glGenFramebuffers")) };
  let bind_framebuffer: extern "C" fn(u32, u32) = unsafe { std::mem::transmute(gl_fn(c"glBindFramebuffer")) };
  let framebuffer_texture_2d: extern "C" fn(u32, u32, u32, u32, i32) =
    unsafe { std::mem::transmute(gl_fn(c"glFramebufferTexture2D")) };
  let check_framebuffer_status: extern "C" fn(u32) -> u32 =
    unsafe { std::mem::transmute(gl_fn(c"glCheckFramebufferStatus")) };
  let delete_framebuffers: extern "C" fn(i32, *const u32) =
    unsafe { std::mem::transmute(gl_fn(c"glDeleteFramebuffers")) };
  let get_integerv: extern "C" fn(u32, *mut i32) = unsafe { std::mem::transmute(gl_fn(c"glGetIntegerv")) };
  let read_pixels: extern "C" fn(i32, i32, i32, i32, u32, u32, *mut std::ffi::c_void) =
    unsafe { std::mem::transmute(gl_fn(c"glReadPixels")) };

  let gl_handle = texture.get_opengl_handle();
  if gl_handle == 0 {
    return Err("texture has no GL handle".to_string());
  }
  let (width, height) = (size.width as i32, size.height as i32);

  let mut prev_fbo: i32 = 0;
  get_integerv(GL_FRAMEBUFFER_BINDING, &mut prev_fbo);

  let mut fbo: u32 = 0;
  gen_framebuffers(1, &mut fbo);
  bind_framebuffer(GL_FRAMEBUFFER, fbo);
  framebuffer_texture_2d(GL_FRAMEBUFFER, GL_COLOR_ATTACHMENT0, GL_TEXTURE_2D, gl_handle as u32, 0);
  let status = check_framebuffer_status(GL_FRAMEBUFFER);

  let result = if status != GL_FRAMEBUFFER_COMPLETE {
    Err(format!("readback framebuffer incomplete: {status:#x}"))
  } else {
    let mut pixels = vec![0u8; (width as usize) * (height as usize) * 4];
    read_pixels(0, 0, width, height, GL_RGBA, GL_UNSIGNED_BYTE, pixels.as_mut_ptr() as *mut _);
    Ok(pixels)
  };

  bind_framebuffer(GL_FRAMEBUFFER, prev_fbo as u32);
  delete_framebuffers(1, &fbo);
  result
}

#[allow(dead_code)]
pub struct GlSurface {
  ctx: ImpellerContext,
  surface: impellers::Surface,
}

impl GlSurface {
  pub fn create(_window: &sdl3::video::Window, size: ISize) -> Result<Self, Box<dyn std::error::Error>> {
    let mut ctx = create_impeller_context();

    let surface = unsafe { ctx.wrap_fbo(0, PixelFormat::RGBA8888, size) }
      .ok_or_else(|| Box::new(std::io::Error::other("Failed to wrap framebuffer")) as Box<dyn std::error::Error>)?;

    Ok(GlSurface { ctx, surface })
  }
}

impl RenderSurface for GlSurface {
  fn draw_display_list(&mut self, dl: &DisplayList) -> Result<(), Box<dyn std::error::Error>> {
    self
      .surface
      .draw_display_list(dl)
      .map_err(|_| Box::new(std::io::Error::other("Failed to draw display list")) as Box<dyn std::error::Error>)
  }

  fn present(&mut self, window: &sdl3::video::Window) {
    window.gl_swap_window();
  }

  fn resize(&mut self, size: ISize) {
    self.surface = unsafe { self.ctx.wrap_fbo(0, PixelFormat::RGBA8888, size) }.expect("Failed to resize GL surface");
  }
}

// Native stack for the UI/JS thread. Large and identical on every platform so
// deep JS recursion behaves the same everywhere (the SDL main thread's stack is
// irrelevant: the engine runs here, not there). This is virtual address space,
// committed only as it is used; it is the hard ceiling under which QuickJS's own
// (smaller, tunable) soft limit sits.
const UI_THREAD_STACK_SIZE: usize = 1024 * 1024 * 1024;

pub fn run_context(
  ui_context: &sdl3::video::GLContext,
  closure: impl FnOnce(Arc<Context>) + Send + 'static,
  tx: mpsc::Sender<DisplayList>,
) {
  let gl_context_ptr = Box::new(SendablePtr(unsafe { ui_context.raw() as *mut std::ffi::c_void }));

  let spawn_result = std::thread::Builder::new().name("srt-ui".into()).stack_size(UI_THREAD_STACK_SIZE).spawn(move || {
    let egl_display = unsafe { sdl3::sys::video::SDL_EGL_GetCurrentDisplay() };
    assert!(!egl_display.is_null(), "no EGL display");
    log::info!("[alloy] EGL display obtained");

    let ui_pbuffer = create_ui_pbuffer(egl_display, gl_context_ptr.0);
    make_current(egl_display, ui_pbuffer, gl_context_ptr.0);
    log::info!("[alloy] GL context made current on pbuffer");

    let (device, queue) = create_wgpu_device();
    log::info!("[alloy] wGPU device created");

    let impeller_ctx = create_impeller_context();
    log::info!("[alloy] Impeller context created");

    let gpu_ctx = Arc::new(Context::new(Backend::Gl, device, queue, impeller_ctx, tx));
    closure(gpu_ctx);
  });
  spawn_result.expect("failed to spawn UI thread");
}

/// Must be called before window creation so SDL selects ANGLE (EGL) on macOS.
pub(crate) fn configure_opengl(video: &sdl3::VideoSubsystem) {
  sdl3::hint::set("SDL_OPENGL_ES_DRIVER", "1");
  let gl_attr = video.gl_attr();
  gl_attr.set_context_profile(sdl3::video::GLProfile::GLES);
  gl_attr.set_context_version(3, 0);
  gl_attr.set_stencil_size(8);

  // Request 4x MSAA for path anti-aliasing. Not all drivers expose a
  // multisampled config; on those, window creation retries without MSAA
  // (see disable_msaa and app::setup).
  gl_attr.set_multisample_buffers(1);
  gl_attr.set_multisample_samples(4);
}

/// Drop the MSAA request so window and GL-context creation can succeed on
/// drivers that expose no multisampled EGL config (notably the Android
/// emulator's GLES translator). Path anti-aliasing is lost; rendering proceeds.
pub(crate) fn disable_msaa(video: &sdl3::VideoSubsystem) {
  let gl_attr = video.gl_attr();
  gl_attr.set_multisample_buffers(0);
  gl_attr.set_multisample_samples(0);
}

pub(crate) fn setup_opengl_platform(
  video: &sdl3::VideoSubsystem,
  window: &sdl3::video::Window,
) -> Result<DisplayContext, Box<dyn std::error::Error>> {
  // Create UI GL context
  let ui_context = window.gl_create_context().map_err(|e| format!("Failed to create UI GL context: {}", e))?;

  // Enable context sharing for main GL context
  let gl_attr = video.gl_attr();
  gl_attr.set_share_with_current_context(true);

  // Create main GL context
  let main_context = window.gl_create_context().map_err(|e| format!("Failed to create main GL context: {}", e))?;

  // Make main context current on the render thread
  window.gl_make_current(&main_context).map_err(|e| format!("Failed to make main GL context current: {}", e))?;

  // Set swap interval (vsync) via FFI
  video.gl_set_swap_interval(SwapInterval::VSync).map_err(|e| format!("Failed to set swap interval: {}", e))?;

  Ok(DisplayContext::Gl { window_opaque: window as *const _ as *const std::ffi::c_void, main_context, ui_context })
}
