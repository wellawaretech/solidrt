use impellers::{ISize, Texture};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::backend::Backend;

pub struct TextureEntry {
  pub gpu: GpuTexture,
  pub impeller: Texture,
}

impl TextureEntry {
  pub fn width(&self) -> u32 {
    self.gpu.wgpu_texture.width()
  }
  pub fn height(&self) -> u32 {
    self.gpu.wgpu_texture.height()
  }
}

impl std::ops::Deref for TextureEntry {
  type Target = Texture;
  fn deref(&self) -> &Texture {
    &self.impeller
  }
}

pub struct TextureRegistry {
  entries: RefCell<HashMap<u64, Rc<TextureEntry>>>,
  next_id: RefCell<u64>,
}

impl TextureRegistry {
  pub(crate) fn new() -> Self {
    TextureRegistry {
      entries: RefCell::new(HashMap::new()),
      next_id: RefCell::new(1),
    }
  }

  pub fn get(&self, id: u64) -> Option<Rc<TextureEntry>> {
    self.entries.borrow().get(&id).map(Rc::clone)
  }

  pub fn insert(&self, id: u64, entry: TextureEntry) {
    self.entries.borrow_mut().insert(id, Rc::new(entry));
  }

  pub fn allocate_id(&self) -> u64 {
    let mut id = self.next_id.borrow_mut();
    let result = *id;
    *id += 1;
    result
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
        | wgpu::TextureUsages::COPY_DST
        | wgpu::TextureUsages::TEXTURE_BINDING,
      view_formats: &[],
    });
    GpuTexture {
      wgpu_texture,
      backend,
    }
  }

  pub fn upload(&self, device: &wgpu::Device, queue: &wgpu::Queue, data: &[u8], size: ISize) {
    let (width, height) = (size.width as u32, size.height as u32);
    let bytes_per_row = width * 4;
    // wgpu requires bytes_per_row to be a multiple of COPY_BYTES_PER_ROW_ALIGNMENT (256).
    // Stage rows into a padded buffer when the natural row stride isn't aligned.
    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let bytes_per_row_aligned = bytes_per_row.div_ceil(align) * align;
    let staging_size = (bytes_per_row_aligned as u64) * (height as u64);

    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
      label: Some("texture_upload_buffer"),
      size: staging_size,
      usage: wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::MAP_WRITE,
      mapped_at_creation: true,
    });
    {
      let mut mapped = buffer.slice(..).get_mapped_range_mut();
      if bytes_per_row == bytes_per_row_aligned {
        mapped.copy_from_slice(data);
      } else {
        // Pad rows out to bytes_per_row_aligned via a temporary Vec, then copy
        // the padded buffer in one shot. BufferViewMut doesn't support partial
        // indexing in this wgpu version.
        let mut padded = vec![0u8; staging_size as usize];
        for row in 0..height as usize {
          let src_start = row * bytes_per_row as usize;
          let dst_start = row * bytes_per_row_aligned as usize;
          padded[dst_start..dst_start + bytes_per_row as usize]
            .copy_from_slice(&data[src_start..src_start + bytes_per_row as usize]);
        }
        mapped.copy_from_slice(&padded);
      }
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
          bytes_per_row: Some(bytes_per_row_aligned),
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