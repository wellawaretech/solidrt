use impellers::{Context as ImpellerContext, DisplayList, ISize, Texture};
use std::rc::Rc;
use std::sync::mpsc;

use crate::backend::Backend;
use crate::gl;
use crate::texture::{GpuTexture, TextureEntry, TextureRegistry};

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