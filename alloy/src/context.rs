use impellers::{Context as ImpellerContext, DisplayList, ISize, Texture};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;

use crate::backend::Backend;
use crate::camera::CameraRegistry;
use crate::gl;
use crate::microphone::MicrophoneRegistry;
use crate::texture::{GpuTexture, TextureEntry, TextureRegistry};

pub struct Context {
  backend: Backend,
  wgpu_device: wgpu::Device,
  wgpu_queue: wgpu::Queue,
  // RefCell because wrap_fbo needs &mut while Context is shared behind Arc;
  // like the registries, it is only ever touched from the UI thread.
  impeller_ctx: RefCell<ImpellerContext>,
  pub textures: TextureRegistry,
  pub(crate) cameras: CameraRegistry,
  pub(crate) microphones: MicrophoneRegistry,
  tx: mpsc::Sender<DisplayList>,
}

// Safety: Context is thread-safe (Send + Sync) because:
// - wgpu::Device and Queue are thread-safe (Send + Sync)
// - Impeller::Context uses thread-local GL state, but we ensure proper synchronization
//   by making the GL context current on the rendering thread before any GPU operations
// - We never access the Impeller context concurrently; only the UI thread with its GL context current
unsafe impl Send for Context {}
unsafe impl Sync for Context {}

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
      impeller_ctx: RefCell::new(impeller_ctx),
      textures: TextureRegistry::new(),
      cameras: CameraRegistry::default(),
      microphones: MicrophoneRegistry::default(),
      tx,
    }
  }

  pub fn submit(&self, dl: DisplayList) -> Result<(), ()> {
    self.tx.send(dl).map_err(|_| ())
  }

  pub fn get_or_create_texture(&self, id: u64, size: ISize, make_pixels: impl FnOnce() -> Vec<u8>) -> Rc<TextureEntry> {
    if self.textures.get(id).is_none() {
      let pixels = make_pixels();
      let gpu = GpuTexture::new(&self.wgpu_device, self.backend, size);
      gpu.upload(&self.wgpu_device, &self.wgpu_queue, &pixels, size);
      let impeller = self.adopt_texture(&gpu, size).expect("adopt texture failed");
      self.textures.insert(id, TextureEntry { gpu, impeller });
    }
    self.textures.get(id).expect("texture must exist after insert")
  }

  pub fn get_or_update_texture(&self, id: u64, size: ISize, make_pixels: impl FnOnce() -> Vec<u8>) -> Rc<TextureEntry> {
    let pixels = make_pixels();
    if self.textures.get(id).is_none() {
      let gpu = GpuTexture::new(&self.wgpu_device, self.backend, size);
      gpu.upload(&self.wgpu_device, &self.wgpu_queue, &pixels, size);
      let impeller = self.adopt_texture(&gpu, size).expect("adopt texture failed");
      self.textures.insert(id, TextureEntry { gpu, impeller });
    } else {
      let entry = self.textures.get(id).expect("texture must exist in else branch");
      entry.gpu.upload(&self.wgpu_device, &self.wgpu_queue, &pixels, size);
    }
    self.textures.get(id).expect("texture must exist after insert or update")
  }

  /// Create a sampleable texture from RGBA8 pixels and adopt into Impeller.
  /// Returns the registry id assigned to the new texture.
  pub fn create_texture_from_pixels(&self, width: u32, height: u32, pixels: &[u8]) -> u64 {
    let id = self.textures.allocate_id();
    self.create_texture_at(id, width, height, pixels);
    id
  }

  /// Create (or replace) the texture stored at `id`, e.g. to resize a stream
  /// texture without invalidating the id handed out to consumers. Lookups pick
  /// up the new texture immediately; in-flight users of the old entry keep it
  /// alive until released.
  pub fn create_texture_at(&self, id: u64, width: u32, height: u32, pixels: &[u8]) {
    let size = ISize::new(width as i64, height as i64);
    let gpu = GpuTexture::new(&self.wgpu_device, self.backend, size);
    gpu.upload(&self.wgpu_device, &self.wgpu_queue, pixels, size);
    let impeller = self.adopt_texture(&gpu, size).expect("adopt texture failed");
    self.textures.insert(id, TextureEntry { gpu, impeller });
  }

  /// Re-upload RGBA8 pixels into an existing texture. `pixels` may be a larger
  /// buffer holding multiple frames; `offset` selects the frame start. The
  /// frame must match the texture's dimensions exactly.
  pub fn update_texture(&self, id: u64, pixels: &[u8], offset: usize) -> Result<(), String> {
    let entry = self.textures.get(id).ok_or_else(|| format!("texture {id} not found"))?;
    let (width, height) = (entry.width(), entry.height());
    let frame_size = (width as usize) * (height as usize) * 4;
    let end = offset.checked_add(frame_size).ok_or_else(|| "offset overflow".to_string())?;
    if end > pixels.len() {
      return Err(format!(
        "need {frame_size} bytes at offset {offset} for {width}x{height}, buffer has {}",
        pixels.len()
      ));
    }
    let size = ISize::new(width as i64, height as i64);
    entry.gpu.upload(&self.wgpu_device, &self.wgpu_queue, &pixels[offset..end], size);
    Ok(())
  }

  /// Rasterize a display list into a new GPU texture of the given pixel size
  /// and adopt it into Impeller for sampling. The texture is not placed in the
  /// registry; the caller owns the returned entry.
  pub fn render_display_list_to_texture(&self, dl: &DisplayList, width: u32, height: u32) -> Result<TextureEntry, String> {
    let size = ISize::new(width as i64, height as i64);
    let gpu = GpuTexture::new(&self.wgpu_device, self.backend, size);
    match self.backend {
      Backend::Gl => {
        // A wrapped FBO is treated like a window backbuffer, which GL stores
        // bottom-up; pre-flip the content so the texture ends up upright.
        let mut flipped = impellers::DisplayListBuilder::new(None);
        flipped.translate(0.0, height as f32);
        flipped.scale(1.0, -1.0);
        flipped.draw_display_list(dl, 1.0);
        let flipped = flipped.build().ok_or_else(|| "failed to build flipped display list".to_string())?;
        gl::render_display_list_to_texture(&gpu, &mut self.impeller_ctx.borrow_mut(), &flipped, size)?
      }
      Backend::Vulkan => panic!("Vulkan backend not yet implemented"),
      Backend::Metal => panic!("Metal backend not yet implemented"),
    }
    let impeller = self.adopt_texture(&gpu, size).ok_or_else(|| "adopt texture failed".to_string())?;
    Ok(TextureEntry { gpu, impeller })
  }

  /// Read back a texture's RGBA8 pixels (tightly packed top-to-bottom rows).
  pub fn read_texture(&self, entry: &TextureEntry) -> Result<Vec<u8>, String> {
    let size = ISize::new(entry.width() as i64, entry.height() as i64);
    match self.backend {
      Backend::Gl => gl::read_texture_pixels(&entry.gpu, size),
      Backend::Vulkan => panic!("Vulkan backend not yet implemented"),
      Backend::Metal => panic!("Metal backend not yet implemented"),
    }
  }

  pub fn adopt_texture(&self, gpu_texture: &GpuTexture, size: ISize) -> Option<Texture> {
    match gpu_texture.backend {
      Backend::Gl => gl::adopt_texture(gpu_texture, &self.impeller_ctx.borrow(), size),
      Backend::Vulkan => {
        panic!("Vulkan backend not yet implemented");
      }
      Backend::Metal => {
        panic!("Metal backend not yet implemented");
      }
    }
  }
}
