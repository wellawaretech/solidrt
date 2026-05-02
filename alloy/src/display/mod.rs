pub mod gl;

use impellers::{Context, DisplayList, DisplayListBuilder};
use wgpu::TextureFormat;
use std::sync::mpsc;

pub struct SendablePtr(pub *mut std::ffi::c_void);
unsafe impl Send for SendablePtr {}
unsafe impl Sync for SendablePtr {}

#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub enum Backend {
    Gl,
    Vulkan,
}

#[allow(dead_code)]
pub enum DisplayContext {
    Gl {
        window_opaque: *const std::ffi::c_void,
        main_context: sdl3::video::GLContext,
        ui_context: sdl3::video::GLContext,
    },
    Vulkan {},
}

#[allow(dead_code)]
impl DisplayContext {
    pub fn new_opengl(
        video: &sdl3::VideoSubsystem,
        window: &sdl3::video::Window,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        gl::setup_opengl_platform(video, window)
    }

    pub fn main_context(&self) -> &sdl3::video::GLContext {
        match self {
            DisplayContext::Gl { main_context, .. } => main_context,
            DisplayContext::Vulkan { .. } => panic!("No context in Vulkan platform"),
        }
    }

    pub fn ui_context(&self) -> &sdl3::video::GLContext {
        match self {
            DisplayContext::Gl { ui_context, .. } => ui_context,
            DisplayContext::Vulkan { .. } => panic!("No context in Vulkan platform"),
        }
    }

    pub fn backend(&self) -> Backend {
        match self {
            DisplayContext::Gl { .. } => Backend::Gl,
            DisplayContext::Vulkan { .. } => Backend::Vulkan,
        }
    }
}

#[allow(dead_code)]
pub trait RenderSurface {
    fn draw_display_list(&mut self, dl: &DisplayList) -> Result<(), Box<dyn std::error::Error>>;
    fn present(&mut self);
    fn resize(&mut self, width: u32, height: u32);
}

pub struct GpuContext {
    pub backend: Backend,
    pub wgpu_device: wgpu::Device,
    pub wgpu_queue: wgpu::Queue,
    pub impeller_ctx: Context,
}

// Safety: GpuContext is thread-safe (Send + Sync) because:
// - wgpu::Device and Queue are thread-safe (Send + Sync)
// - Impeller::Context uses thread-local GL state, but we ensure proper synchronization
//   by making the GL context current on the rendering thread before any GPU operations
// - We never access the Impeller context concurrently; only the UI thread with its GL context current
unsafe impl Send for GpuContext {}
unsafe impl Sync for GpuContext {}

pub struct GpuTexture {
    pub wgpu_texture: wgpu::Texture,
    pub backend: Backend,
}

pub fn create_render_surface(
    platform: &DisplayContext,
    width: u32,
    height: u32,
) -> Result<Box<dyn RenderSurface>, Box<dyn std::error::Error>> {
    match platform {
        DisplayContext::Gl {
            window_opaque,
            main_context: _,
            ui_context: _,
            ..
        } => {
            let window = unsafe { &*(*window_opaque as *const sdl3::video::Window) };
            gl::GlSurface::create(window, width, height)
                .map(|s| Box::new(s) as Box<dyn RenderSurface>)
        }
        DisplayContext::Vulkan { .. } => {
            Err("Vulkan backend not yet implemented".into())
        }
    }
}

impl GpuContext {
    /// Create a GPU context for the given backend and GL context.
    pub fn new(
        backend: Backend,
        wgpu_device: wgpu::Device,
        wgpu_queue: wgpu::Queue,
        impeller_ctx: Context,
    ) -> Self {
        GpuContext {
            backend,
            wgpu_device,
            wgpu_queue,
            impeller_ctx,
        }
    }

    /// Create a wGPU texture for rendering.
    pub fn create_texture(&self, width: u32, height: u32) -> GpuTexture {
        let wgpu_texture = self.wgpu_device.create_texture(&wgpu::TextureDescriptor {
            label: Some("gpu_render_texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        GpuTexture {
            wgpu_texture,
            backend: self.backend,
        }
    }

    /// Upload blue pixel data to a wGPU texture so it can be adopted by Impeller.
    pub fn render_to_texture(&self, gpu_texture: &GpuTexture, width: u32, height: u32) {
        // Create a buffer with blue pixels (RGBA format)
        let mut pixels = vec![0u8; (width * height * 4) as usize];
        for i in (0..pixels.len()).step_by(4) {
            pixels[i] = 51;      // R
            pixels[i + 1] = 77;  // G
            pixels[i + 2] = 128; // B (blue)
            pixels[i + 3] = 255; // A
        }

        // Write the pixels to the texture using a mapped buffer (avoids GL backend leak)
        let buffer = self.wgpu_device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("texture_upload_buffer"),
            size: pixels.len() as u64,
            usage: wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::MAP_WRITE,
            mapped_at_creation: true,
        });

        {
            let mut mapped = buffer.slice(..).get_mapped_range_mut();
            mapped.copy_from_slice(&pixels);
        }
        buffer.unmap();

        // Copy the buffer to the texture
        let mut encoder = self.wgpu_device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
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
                texture: &gpu_texture.wgpu_texture,
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

        self.wgpu_queue.submit(std::iter::once(encoder.finish()));
        // Ensure texture upload completes before adoption
        let _ = self.wgpu_device.poll(wgpu::PollType::Poll);
    }

    /// Convert a wGPU texture to an Impeller-compatible texture and add it to the display list.
    /// This is backend-specific: GL uses zero-copy adoption, Vulkan will use GPU copy.
    pub fn texture_to_display_list(
        &self,
        gpu_texture: &GpuTexture,
        builder: &mut DisplayListBuilder,
        width: u32,
        height: u32,
    ) {
        texture_to_display_list(gpu_texture, &self.impeller_ctx, builder, width, height);
    }
}

fn texture_to_display_list(
    gpu_texture: &GpuTexture,
    impeller_ctx: &Context,
    builder: &mut DisplayListBuilder,
    width: u32,
    height: u32,
) {
    match gpu_texture.backend {
        Backend::Gl => gl::texture_to_display_list(gpu_texture, impeller_ctx, builder, width, height),
        Backend::Vulkan => {
            // TODO: Implement Vulkan GPU copy path
            panic!("Vulkan backend not yet implemented");
        }
    }
}

pub fn setup_ui_thread(
    platform: &DisplayContext,
    closure: impl FnOnce(&GpuContext, mpsc::Sender<DisplayList>) + Send + 'static,
) -> mpsc::Receiver<DisplayList> {
    let (tx, rx) = mpsc::channel();

    // Extract GL context before moving into thread (platform has raw pointers not Send)
    let gl_context = platform.ui_context();
    let gl_context_ptr = Box::new(SendablePtr(unsafe {
        std::mem::transmute_copy::<_, *mut std::ffi::c_void>(gl_context)
    }));

    std::thread::spawn(move || {
        // Infrastructure setup
        let egl_display = unsafe { sdl3::sys::video::SDL_EGL_GetCurrentDisplay() };
        assert!(!egl_display.is_null(), "no EGL display");
        eprintln!("[UI thread] EGL display obtained");

        let ui_pbuffer = gl::create_ui_pbuffer(egl_display, gl_context_ptr.0);
        gl::make_current(egl_display, ui_pbuffer, gl_context_ptr.0);
        eprintln!("[UI thread] GL context made current on pbuffer");

        let (device, queue) = gl::create_wgpu_device();
        eprintln!("[UI thread] wGPU device created");

        let impeller_ctx = gl::create_impeller_context();
        eprintln!("[UI thread] Impeller context created");

        let gpu_ctx = GpuContext::new(Backend::Gl, device, queue, impeller_ctx);

        // Run user's closure with GPU context and sender
        closure(&gpu_ctx, tx);
    });

    rx
}
