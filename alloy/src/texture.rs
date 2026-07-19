use glow::HasContext;
use impellers::{ISize, Texture};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::num::NonZeroU32;
use std::rc::Rc;

use crate::backend::Backend;

/// The UI thread's view of a registered texture: the adopted Impeller handle
/// (all a display list needs) plus dimensions for layout measure and update
/// validation. The GL name behind it lives in the raster thread's map.
pub struct TextureEntry {
  pub impeller: Texture,
  pub width: u32,
  pub height: u32,
}

impl TextureEntry {
  pub fn width(&self) -> u32 {
    self.width
  }
  pub fn height(&self) -> u32 {
    self.height
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
  // Bumped on insert (create or replace at an id). Content uploads into an
  // existing texture do not count. Lets a painter detect that a retained
  // display list may reference replaced textures and must be rebuilt.
  generation: Cell<u64>,
}

impl TextureRegistry {
  pub(crate) fn new() -> Self {
    TextureRegistry { entries: RefCell::new(HashMap::new()), next_id: RefCell::new(1), generation: Cell::new(0) }
  }

  pub fn get(&self, id: u64) -> Option<Rc<TextureEntry>> {
    self.entries.borrow().get(&id).map(Rc::clone)
  }

  /// Number of textures currently held in the registry.
  pub fn len(&self) -> usize {
    self.entries.borrow().len()
  }

  /// (id, width, height) of every registered texture, unordered.
  pub fn list(&self) -> Vec<(u64, u32, u32)> {
    self.entries.borrow().iter().map(|(id, e)| (*id, e.width(), e.height())).collect()
  }

  pub fn is_empty(&self) -> bool {
    self.entries.borrow().is_empty()
  }

  pub fn insert(&self, id: u64, entry: TextureEntry) {
    self.entries.borrow_mut().insert(id, Rc::new(entry));
    self.generation.set(self.generation.get().wrapping_add(1));
  }

  pub fn generation(&self) -> u64 {
    self.generation.get()
  }

  pub fn allocate_id(&self) -> u64 {
    let mut id = self.next_id.borrow_mut();
    let result = *id;
    *id += 1;
    result
  }

  pub fn remove(&self, id: u64) -> Option<Rc<TextureEntry>> {
    self.entries.borrow_mut().remove(&id)
  }
}

/// A GL texture that is adopted into Impeller right after creation. Impeller
/// takes ownership of the GL name and deletes it when its Texture drops, so
/// GpuTexture deliberately does NOT delete the name (no Drop impl) - doing so
/// would double-free the name and corrupt whatever live texture reuses it.
/// Raster-thread-only: creation and uploads are GL work.
pub struct GpuTexture {
  pub gl_texture: glow::Texture,
  pub backend: Backend,
  pub width: u32,
  pub height: u32,
}

impl GpuTexture {
  pub fn new(gl: &glow::Context, backend: Backend, size: ISize) -> Self {
    let (width, height) = (size.width as u32, size.height as u32);
    unsafe {
      let prev = gl.get_parameter_i32(glow::TEXTURE_BINDING_2D);
      let gl_texture = gl.create_texture().expect("glGenTextures failed");
      gl.bind_texture(glow::TEXTURE_2D, Some(gl_texture));
      gl.tex_image_2d(
        glow::TEXTURE_2D,
        0,
        glow::RGBA8 as i32,
        width as i32,
        height as i32,
        0,
        glow::RGBA,
        glow::UNSIGNED_BYTE,
        glow::PixelUnpackData::Slice(None),
      );
      // No mips exist: the default MIN_FILTER references mipmaps, which would
      // make the texture sampling-incomplete (reads as black) when Impeller
      // samples it.
      gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MIN_FILTER, glow::LINEAR as i32);
      gl.tex_parameter_i32(glow::TEXTURE_2D, glow::TEXTURE_MAG_FILTER, glow::LINEAR as i32);
      gl.bind_texture(glow::TEXTURE_2D, NonZeroU32::new(prev as u32).map(glow::NativeTexture));
      GpuTexture { gl_texture, backend, width, height }
    }
  }

  pub fn upload(&self, gl: &glow::Context, data: &[u8], size: ISize) {
    let (width, height) = (size.width as i32, size.height as i32);
    unsafe {
      let prev = gl.get_parameter_i32(glow::TEXTURE_BINDING_2D);
      gl.bind_texture(glow::TEXTURE_2D, Some(self.gl_texture));
      // RGBA8 rows are width*4, always a multiple of 4, so the default unpack
      // alignment is fine; no per-row staging is needed (unlike wgpu's 256-byte
      // copy alignment).
      gl.pixel_store_i32(glow::UNPACK_ALIGNMENT, 4);
      gl.tex_sub_image_2d(
        glow::TEXTURE_2D,
        0,
        0,
        0,
        width,
        height,
        glow::RGBA,
        glow::UNSIGNED_BYTE,
        glow::PixelUnpackData::Slice(Some(data)),
      );
      gl.bind_texture(glow::TEXTURE_2D, NonZeroU32::new(prev as u32).map(glow::NativeTexture));
      // No glFinish: the texture is sampled later on this same (single) GL
      // context, so program order already sequences the upload first.
    }
  }
}
