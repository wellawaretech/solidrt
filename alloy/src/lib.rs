mod gl;
pub mod sdl_utils;

pub use impellers;
use impellers::{Context as ImpellerContext, DisplayList, ISize, Rect, Texture};
pub use sdl3;

#[macro_export]
macro_rules! log {
    ($($arg:tt)*) => {
        $crate::sdl3::log::log(&format!($($arg)*))
    };
}

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{mpsc, Arc};

#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub enum Backend {
  Gl,
  Vulkan,
  Metal,
}

#[allow(dead_code)]
pub enum DisplayContext {
  Gl {
    window_opaque: *const std::ffi::c_void,
    main_context: sdl3::video::GLContext,
    ui_context: sdl3::video::GLContext,
  },
  Vulkan {},
  Metal {},
}

#[allow(dead_code)]
impl DisplayContext {
  pub fn new_opengl(
    video: &sdl3::VideoSubsystem,
    window: &sdl3::video::Window,
  ) -> Result<Self, Box<dyn std::error::Error>> {
    gl::setup_opengl_platform(video, window)
  }

  pub fn backend(&self) -> Backend {
    match self {
      DisplayContext::Gl { .. } => Backend::Gl,
      DisplayContext::Vulkan { .. } => Backend::Vulkan,
      DisplayContext::Metal { .. } => Backend::Metal,
    }
  }
}

#[allow(dead_code)]
pub trait RenderSurface {
  fn draw_display_list(&mut self, dl: &DisplayList) -> Result<(), Box<dyn std::error::Error>>;
  fn present(&mut self);
  fn resize(&mut self, size: ISize);
}

pub struct TextureEntry {
  pub gpu: GpuTexture,
  pub impeller: Texture,
}

impl std::ops::Deref for TextureEntry {
  type Target = Texture;
  fn deref(&self) -> &Texture {
    &self.impeller
  }
}

pub struct TextureRegistry {
  entries: RefCell<HashMap<u64, Rc<TextureEntry>>>,
}

impl TextureRegistry {
  fn new() -> Self {
    TextureRegistry {
      entries: RefCell::new(HashMap::new()),
    }
  }

  pub fn get(&self, id: u64) -> Option<Rc<TextureEntry>> {
    self.entries.borrow().get(&id).map(Rc::clone)
  }

  pub fn insert(&self, id: u64, entry: TextureEntry) {
    self.entries.borrow_mut().insert(id, Rc::new(entry));
  }
}

pub struct Context {
  backend: Backend,
  wgpu_device: wgpu::Device,
  wgpu_queue: wgpu::Queue,
  impeller_ctx: ImpellerContext,
  pub textures: TextureRegistry,
  tx: mpsc::Sender<DisplayList>,
}

// Safety: Context is thread-safe (Send + Sync) because:
// - wgpu::Device and Queue are thread-safe (Send + Sync)
// - Impeller::Context uses thread-local GL state, but we ensure proper synchronization
//   by making the GL context current on the rendering thread before any GPU operations
// - We never access the Impeller context concurrently; only the UI thread with its GL context current
unsafe impl Send for Context {}
unsafe impl Sync for Context {}

pub struct GpuTexture {
  pub wgpu_texture: wgpu::Texture,
  pub backend: Backend,
}

impl GpuTexture {
  pub fn new(device: &wgpu::Device, backend: Backend, size: ISize) -> Self {
    let wgpu_texture = device.create_texture(&wgpu::TextureDescriptor {
      label: Some("gpu_render_texture"),
      size: wgpu::Extent3d {
        width: size.width as u32,
        height: size.height as u32,
        depth_or_array_layers: 1,
      },
      mip_level_count: 1,
      sample_count: 1,
      dimension: wgpu::TextureDimension::D2,
      format: wgpu::TextureFormat::Rgba8Unorm,
      usage: wgpu::TextureUsages::RENDER_ATTACHMENT
        | wgpu::TextureUsages::COPY_SRC
        | wgpu::TextureUsages::COPY_DST,
      view_formats: &[],
    });
    GpuTexture {
      wgpu_texture,
      backend,
    }
  }

  pub fn upload(&self, device: &wgpu::Device, queue: &wgpu::Queue, data: &[u8], size: ISize) {
    let (width, height) = (size.width as u32, size.height as u32);
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
      label: Some("texture_upload_buffer"),
      size: data.len() as u64,
      usage: wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::MAP_WRITE,
      mapped_at_creation: true,
    });
    {
      let mut mapped = buffer.slice(..).get_mapped_range_mut();
      mapped.copy_from_slice(data);
    }
    buffer.unmap();

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
      label: Some("texture_copy_encoder"),
    });
    encoder.copy_buffer_to_texture(
      wgpu::TexelCopyBufferInfo {
        buffer: &buffer,
        layout: wgpu::TexelCopyBufferLayout {
          offset: 0,
          bytes_per_row: Some(width * 4),
          rows_per_image: Some(height),
        },
      },
      wgpu::TexelCopyTextureInfo {
        texture: &self.wgpu_texture,
        mip_level: 0,
        origin: wgpu::Origin3d::ZERO,
        aspect: wgpu::TextureAspect::All,
      },
      wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
      },
    );
    queue.submit(std::iter::once(encoder.finish()));
    let _ = device.poll(wgpu::PollType::Poll);
  }
}

pub fn create_render_surface(
  platform: &DisplayContext,
  size: ISize,
) -> Result<Box<dyn RenderSurface>, Box<dyn std::error::Error>> {
  match platform {
    DisplayContext::Gl { window_opaque, .. } => {
      let window = unsafe { &*(*window_opaque as *const sdl3::video::Window) };
      gl::GlSurface::create(window, size).map(|s| Box::new(s) as Box<dyn RenderSurface>)
    }
    DisplayContext::Vulkan { .. } => Err("Vulkan backend not yet implemented".into()),
    DisplayContext::Metal { .. } => Err("Metal backend not yet implemented".into()),
  }
}

impl Context {
  pub fn new(
    backend: Backend,
    wgpu_device: wgpu::Device,
    wgpu_queue: wgpu::Queue,
    impeller_ctx: ImpellerContext,
    tx: mpsc::Sender<DisplayList>,
  ) -> Self {
    Context {
      backend,
      wgpu_device,
      wgpu_queue,
      impeller_ctx,
      textures: TextureRegistry::new(),
      tx,
    }
  }

  pub fn submit(&self, dl: DisplayList) -> Result<(), ()> {
    self.tx.send(dl).map_err(|_| ())
  }

  pub fn get_or_create_texture(
    &self,
    id: u64,
    size: ISize,
    make_pixels: impl FnOnce() -> Vec<u8>,
  ) -> Rc<TextureEntry> {
    if self.textures.get(id).is_none() {
      let pixels = make_pixels();
      let gpu = GpuTexture::new(&self.wgpu_device, self.backend, size);
      gpu.upload(&self.wgpu_device, &self.wgpu_queue, &pixels, size);
      let impeller = self
        .adopt_texture(&gpu, size)
        .expect("adopt texture failed");
      self.textures.insert(id, TextureEntry { gpu, impeller });
    }
    self.textures.get(id).unwrap()
  }

  pub fn get_or_update_texture(
    &self,
    id: u64,
    size: ISize,
    make_pixels: impl FnOnce() -> Vec<u8>,
  ) -> Rc<TextureEntry> {
    let pixels = make_pixels();
    if self.textures.get(id).is_none() {
      let gpu = GpuTexture::new(&self.wgpu_device, self.backend, size);
      gpu.upload(&self.wgpu_device, &self.wgpu_queue, &pixels, size);
      let impeller = self
        .adopt_texture(&gpu, size)
        .expect("adopt texture failed");
      self.textures.insert(id, TextureEntry { gpu, impeller });
    } else {
      let entry = self.textures.get(id).unwrap();
      entry
        .gpu
        .upload(&self.wgpu_device, &self.wgpu_queue, &pixels, size);
    }
    self.textures.get(id).unwrap()
  }

  pub fn adopt_texture(&self, gpu_texture: &GpuTexture, size: ISize) -> Option<Texture> {
    match gpu_texture.backend {
      Backend::Gl => gl::adopt_texture(gpu_texture, &self.impeller_ctx, size),
      Backend::Vulkan => {
        panic!("Vulkan backend not yet implemented");
      }
      Backend::Metal => {
        panic!("Metal backend not yet implemented");
      }
    }
  }
}

impl DisplayContext {
  pub fn run_context(
    &self,
    closure: impl FnOnce(Arc<Context>) + Send + 'static,
    tx: mpsc::Sender<DisplayList>,
  ) {
    match self {
      DisplayContext::Gl { ui_context, .. } => gl::run_context(ui_context, closure, tx),
      DisplayContext::Vulkan { .. } => unimplemented!("Vulkan backend not yet implemented"),
      DisplayContext::Metal { .. } => unimplemented!("Metal backend not yet implemented"),
    }
  }
}

pub struct App {
  sdl_context: sdl3::Sdl,
  window: sdl3::video::Window,
  platform: DisplayContext,
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
    .fullscreen()
    .high_pixel_density()
    .build()
    .expect("Failed to create window");

  let platform = DisplayContext::new_opengl(&video, &window).expect("Failed to set up platform");

  let (w, h) = window.size_in_pixels();
  let window_size = ISize::new(w as i64, h as i64);
  let render_surface =
    create_render_surface(&platform, window_size).expect("Failed to create render surface");

  App {
    sdl_context,
    window,
    platform,
    render_surface,
  }
}

pub enum Event {
  Quit,
  KeyDown {
    keycode: Option<sdl3::keyboard::Keycode>,
    scancode: Option<sdl3::keyboard::Scancode>,
  },
  Resize {
    size: ISize,
    safe_area: Rect,
    display_scale: f32,
  },
}

fn translate_event(
  sdl_event: sdl3::event::Event,
  window_ptr: *mut sdl3::sys::video::SDL_Window,
) -> Option<Event> {
  match sdl_event {
    sdl3::event::Event::Quit { .. } => Some(Event::Quit),
    sdl3::event::Event::KeyDown { keycode, scancode, .. } => {
      Some(Event::KeyDown { keycode, scancode })
    }
    sdl3::event::Event::Window {
      win_event: sdl3::event::WindowEvent::PixelSizeChanged(w, h),
      ..
    } => {
      let size = ISize::new(w as i64, h as i64);
      let r = sdl_utils::window_safe_area(window_ptr);
      let safe_area = Rect::new(
        impellers::Point::new(r.x as f32, r.y as f32),
        impellers::Size::new(r.w as f32, r.h as f32),
      );
      let display_scale = sdl_utils::window_display_scale(window_ptr);
      Some(Event::Resize { size, safe_area, display_scale })
    }
    _ => None,
  }
}

pub struct RenderHooks {
  pub pre_render: Box<dyn FnMut()>,
  pub post_render: Box<dyn FnMut()>,
}

impl App {
  pub fn run(
    self,
    dl_producer: impl FnOnce(Arc<Context>, mpsc::Receiver<Event>) + Send + 'static,
    mut hooks: RenderHooks,
  ) {
    let App {
      sdl_context: _sdl_context,
      window,
      platform,
      mut render_surface,
    } = self;

    let window_ptr = window.raw() as *mut sdl3::sys::video::SDL_Window;
    let _window = window;

    let (tx, rx) = mpsc::channel::<DisplayList>();
    let (event_tx, event_rx) = mpsc::channel::<Event>();
    platform.run_context(move |ctx| dl_producer(ctx, event_rx), tx);

    loop {
      match rx.recv_timeout(std::time::Duration::from_millis(8)) {
        Ok(mut dl) => {
          while let Ok(newer) = rx.try_recv() {
            dl = newer;
          }
          (hooks.pre_render)();
          render_surface
            .draw_display_list(&dl)
            .expect("Failed to draw display list");
          render_surface.present();
          (hooks.post_render)();
        }
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
      }
      sdl_utils::drain_events(|sdl_event| {
        if let Some(e) = translate_event(sdl_event, window_ptr) {
          event_tx.send(e).ok();
        }
      });
    }
  }
}
