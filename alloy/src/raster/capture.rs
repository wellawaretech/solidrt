//! The capture and readback path: offscreen rasterization of display lists
//! into adopted textures (snapshot boundaries, node captures), independent of
//! the frame loop in the parent module.

use impellers::{DisplayList, ISize, Texture};

use super::RasterState;
use crate::backend::Backend;
use crate::gl;

impl RasterState {
  /// Rasterize a display list into a new adopted texture of the given pixel
  /// size, ready for sampling.
  pub(super) fn rasterize(&mut self, dl: &DisplayList, width: u32, height: u32, aa: bool) -> Result<Texture, String> {
    let size = ISize::new(width as i64, height as i64);
    match self.backend {
      Backend::Gl => {
        let flipped = flip_for_fbo(dl, height)?;
        gl::render_display_list_to_texture(
          &self.gl,
          &mut self.impeller_ctx,
          &mut self.offscreen_rig,
          &flipped,
          size,
          aa,
        )
      }
      Backend::Vulkan => panic!("Vulkan backend not yet implemented"),
      Backend::Metal => panic!("Metal backend not yet implemented"),
    }
  }

  /// Re-rasterize a display list into an existing adopted texture whose
  /// aligned backing fits `width` x `height` (the UI thread checks the fit).
  pub(super) fn rasterize_into(
    &mut self,
    dl: &DisplayList,
    texture: &Texture,
    width: u32,
    height: u32,
    aa: bool,
  ) -> Result<(), String> {
    let size = ISize::new(width as i64, height as i64);
    match self.backend {
      Backend::Gl => {
        let flipped = flip_for_fbo(dl, height)?;
        gl::render_display_list_into_texture(
          &self.gl,
          &mut self.impeller_ctx,
          &mut self.offscreen_rig,
          &flipped,
          texture,
          size,
          aa,
        )
      }
      Backend::Vulkan => panic!("Vulkan backend not yet implemented"),
      Backend::Metal => panic!("Metal backend not yet implemented"),
    }
  }
}

/// A wrapped FBO is treated like a window backbuffer, which GL stores
/// bottom-up; pre-flip the content so the texture ends up upright.
pub(super) fn flip_for_fbo(dl: &DisplayList, height: u32) -> Result<DisplayList, String> {
  let mut flipped = impellers::DisplayListBuilder::new(None);
  flipped.translate(0.0, height as f32);
  flipped.scale(1.0, -1.0);
  flipped.draw_display_list(dl, 1.0);
  flipped.build().ok_or_else(|| "failed to build flipped display list".to_string())
}
