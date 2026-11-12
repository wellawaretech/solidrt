//! An SDL surface over caller-owned pixels, shared by the window icon and
//! the image cursors. Main thread only, like its callers.

use sdl3::sys::pixels::SDL_PixelFormat;
use sdl3::sys::surface::{SDL_CreateSurfaceFrom, SDL_Surface};

/// Bytes per pixel of straight-alpha RGBA8.
const RGBA_BYTES_PER_PIXEL: u32 = 4;

/// A surface over straight-alpha RGBA8 pixels. No copy: `rgba` must outlive
/// the surface, which the caller destroys (SDL_DestroySurface).
pub(crate) fn rgba_surface(width: u32, height: u32, rgba: &[u8]) -> Result<*mut SDL_Surface, String> {
  let pitch = width * RGBA_BYTES_PER_PIXEL;
  if rgba.len() != (pitch as usize) * (height as usize) {
    return Err(format!("pixel buffer is {} bytes, expected {}x{}x{RGBA_BYTES_PER_PIXEL}", rgba.len(), width, height));
  }
  // SAFETY: `rgba` holds exactly `height` rows of `pitch` bytes (checked
  // above). SDL only borrows the pixels; the caller keeps them alive until
  // it destroys the surface.
  let surface = unsafe {
    SDL_CreateSurfaceFrom(
      width as i32,
      height as i32,
      SDL_PixelFormat::RGBA32,
      rgba.as_ptr() as *mut std::ffi::c_void,
      pitch as i32,
    )
  };
  if surface.is_null() {
    Err(crate::sdl_error())
  } else {
    Ok(surface)
  }
}
