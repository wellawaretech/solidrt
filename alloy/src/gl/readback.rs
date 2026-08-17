//! Pixel readback: an adopted texture's contents, or the window backbuffer.

use super::rig::{prev_framebuffer, prev_renderbuffer, window_samples};
use glow::HasContext;
use impellers::{ISize, Texture};
use std::num::NonZeroU32;

/// Read back an Impeller GL texture's RGBA8 pixels by attaching its handle to
/// a temporary framebuffer and calling glReadPixels. Returns memory-order rows
/// (row 0 first), which is image top-to-bottom for every texture alloy
/// produces.
pub(crate) fn read_texture_pixels(gl: &glow::Context, texture: &Texture, size: ISize) -> Result<Vec<u8>, String> {
  let gl_handle = texture.get_opengl_handle();
  let tex =
    glow::NativeTexture(NonZeroU32::new(gl_handle as u32).ok_or_else(|| "texture has no GL handle".to_string())?);
  let (width, height) = (size.width as i32, size.height as i32);

  unsafe {
    let prev_fbo = gl.get_parameter_i32(glow::FRAMEBUFFER_BINDING);

    let fbo = gl.create_framebuffer().map_err(|e| format!("glGenFramebuffers failed: {e}"))?;
    gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
    gl.framebuffer_texture_2d(glow::FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::TEXTURE_2D, Some(tex), 0);
    let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);

    let result = if status != glow::FRAMEBUFFER_COMPLETE {
      Err(format!("readback framebuffer incomplete: {status:#x}"))
    } else {
      let mut pixels = vec![0u8; (width as usize) * (height as usize) * 4];
      gl.read_pixels(
        0,
        0,
        width,
        height,
        glow::RGBA,
        glow::UNSIGNED_BYTE,
        glow::PixelPackData::Slice(Some(&mut pixels)),
      );
      Ok(pixels)
    };

    gl.bind_framebuffer(glow::FRAMEBUFFER, prev_framebuffer(prev_fbo));
    gl.delete_framebuffer(fbo);
    result
  }
}

/// Read back the window backbuffer's RGBA8 pixels (FBO 0, bottom-up rows as GL
/// stores them; the playback encoder flips when writing). Called on the raster
/// thread right after the frame's draw, which glReadPixels implicitly waits on.
pub(crate) fn read_fbo0_pixels(gl: &glow::Context, size: ISize) -> Vec<u8> {
  let (width, height) = (size.width as i32, size.height as i32);
  let mut pixels = vec![0u8; (width.max(0) as usize) * (height.max(0) as usize) * 4];
  unsafe {
    let prev_fbo = gl.get_parameter_i32(glow::READ_FRAMEBUFFER_BINDING);
    if window_samples(gl) >= 2 {
      // glReadPixels cannot read a multisampled framebuffer: resolve the
      // window rect into a temporary single-sample FBO first.
      let prev_draw = gl.get_parameter_i32(glow::DRAW_FRAMEBUFFER_BINDING);
      let prev_rbo = gl.get_parameter_i32(glow::RENDERBUFFER_BINDING);
      if let (Ok(rbo), Ok(fbo)) = (gl.create_renderbuffer(), gl.create_framebuffer()) {
        gl.bind_renderbuffer(glow::RENDERBUFFER, Some(rbo));
        gl.renderbuffer_storage(glow::RENDERBUFFER, glow::RGBA8, width, height);
        gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, Some(fbo));
        gl.framebuffer_renderbuffer(glow::DRAW_FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::RENDERBUFFER, Some(rbo));
        gl.bind_framebuffer(glow::READ_FRAMEBUFFER, None);
        gl.blit_framebuffer(0, 0, width, height, 0, 0, width, height, glow::COLOR_BUFFER_BIT, glow::NEAREST);
        gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(fbo));
        gl.read_pixels(
          0,
          0,
          width,
          height,
          glow::RGBA,
          glow::UNSIGNED_BYTE,
          glow::PixelPackData::Slice(Some(&mut pixels)),
        );
        gl.delete_framebuffer(fbo);
        gl.delete_renderbuffer(rbo);
      }
      gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, prev_framebuffer(prev_draw));
      gl.bind_renderbuffer(glow::RENDERBUFFER, prev_renderbuffer(prev_rbo));
    } else {
      gl.bind_framebuffer(glow::READ_FRAMEBUFFER, None);
      gl.read_pixels(
        0,
        0,
        width,
        height,
        glow::RGBA,
        glow::UNSIGNED_BYTE,
        glow::PixelPackData::Slice(Some(&mut pixels)),
      );
    }
    gl.bind_framebuffer(glow::READ_FRAMEBUFFER, prev_framebuffer(prev_fbo));
  }
  pixels
}
