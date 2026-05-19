use impellers::{ISize, Texture};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::backend::Backend;

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
  pub(crate) fn new() -> Self {
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