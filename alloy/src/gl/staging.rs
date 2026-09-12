//! Pixel-unpack staging buffers for per-frame texture uploads.
//!
//! `glTexSubImage2D` from client memory hands the driver a pointer it must
//! consume before the call returns: it copies (and on a tiled GPU often
//! re-tiles) the pixels on the calling thread, and may first wait for any
//! in-flight use of the texture. Measured on the Philips TV, that path moves
//! a 1080p NV12 frame at ~110 MB/s - 25 ms of raster thread for 3.1 MB,
//! against a 20 ms refresh period.
//!
//! Uploading from a buffer object splits that in two: our own memcpy into
//! mapped storage, then a `glTexSubImage2D` whose source is already GPU
//! memory, which the driver can service asynchronously. The buffers are a
//! small ring so a frame never writes into storage the previous upload may
//! still be reading.

use glow::HasContext;

use super::prev_buffer;

/// Staging buffers in the ring. Three covers the deepest pipelining the
/// present fences allow (see PRESENT_FENCE_DEPTH), so a reused buffer is
/// always one the GPU is done with.
const RING: usize = 3;

/// Smallest upload worth staging. Mapping costs a fixed ~0.3 ms on the TV
/// however few bytes follow it, while the driver's own copy is what scales;
/// the two cross somewhere well below a video plane (1 MB: 13.4 ms direct
/// against 4.3 ms staged) and well above a small data texture. Anything
/// under this uploads from client memory, where the fixed cost would be the
/// whole cost.
const MIN_BYTES: usize = 256 * 1024;

/// A ring of pixel-unpack buffers, grown to the largest upload seen.
pub struct UploadStaging {
  buffers: Vec<glow::Buffer>,
  /// Bytes each buffer in the ring holds.
  size: usize,
  next: usize,
  /// Staging is a pure optimization: any failure (no buffer, a map that
  /// returns null) turns it off for good and every upload goes direct.
  available: bool,
}

impl UploadStaging {
  pub fn new() -> Self {
    UploadStaging { buffers: Vec::new(), size: 0, next: 0, available: true }
  }

  /// Copy `data` into the next staging buffer and leave it bound as
  /// GL_PIXEL_UNPACK_BUFFER, so the caller's `glTexSubImage2D` reads from
  /// it at offset 0. `Some(())` means the caller must call `unbind` after
  /// the upload; `None` means staging is unavailable and the caller should
  /// upload from the client pointer.
  pub fn stage(&mut self, gl: &glow::Context, data: &[u8]) -> Option<()> {
    if !self.available || data.len() < MIN_BYTES {
      return None;
    }
    if self.buffers.len() < RING || self.size < data.len() {
      if !self.allocate(gl, data.len()) {
        return None;
      }
    }
    let buffer = self.buffers[self.next];
    self.next = (self.next + 1) % self.buffers.len();
    unsafe {
      gl.bind_buffer(glow::PIXEL_UNPACK_BUFFER, Some(buffer));
      // INVALIDATE_BUFFER is the orphaning contract: the previous contents
      // are dead, so a driver hands back fresh storage instead of waiting
      // for whoever is still reading the old. Deliberately WITHOUT
      // UNSYNCHRONIZED: that bit makes drivers skip the orphaning and hand
      // back the same storage, which the ring alone does not protect - a
      // frame is several plane uploads, so a slot comes around again well
      // before the present fences prove the GPU is done with it. Measured
      // on the TV: invalidate-buffer, invalidate-range and plain reuse all
      // map in ~0.3 ms and copy at the same rate.
      let access = glow::MAP_WRITE_BIT | glow::MAP_INVALIDATE_BUFFER_BIT;
      // Exactly the bytes being written, not the buffer's size: the ring
      // grows to the largest upload ever made and never shrinks, so mapping
      // its full extent would charge every small upload for the largest one.
      let ptr = gl.map_buffer_range(glow::PIXEL_UNPACK_BUFFER, 0, data.len() as i32, access);
      if ptr.is_null() {
        gl.bind_buffer(glow::PIXEL_UNPACK_BUFFER, None);
        log::warn!("[alloy] pixel-unpack staging unavailable (map failed); uploading from client memory");
        self.release(gl);
        return None;
      }
      std::ptr::copy_nonoverlapping(data.as_ptr(), ptr, data.len());
      // glUnmapBuffer can report that the contents were lost (a GPU reset
      // while mapped); glow discards the result, so a torn frame here is a
      // frame, not a hang, and the next one replaces it.
      gl.unmap_buffer(glow::PIXEL_UNPACK_BUFFER);
    }
    Some(())
  }

  /// Restore the default unpack binding after a staged upload. Nothing else
  /// in the process binds one, so the default is what everyone assumes.
  pub fn unbind(&self, gl: &glow::Context) {
    unsafe { gl.bind_buffer(glow::PIXEL_UNPACK_BUFFER, None) };
  }

  /// (Re)create the ring at `size` bytes per buffer. False turns staging off.
  fn allocate(&mut self, gl: &glow::Context, size: usize) -> bool {
    self.release(gl);
    self.available = true;
    unsafe {
      let prev = gl.get_parameter_i32(glow::PIXEL_UNPACK_BUFFER_BINDING);
      for _ in 0..RING {
        let Ok(buffer) = gl.create_buffer() else {
          log::warn!("[alloy] pixel-unpack staging unavailable (glGenBuffers failed)");
          self.release(gl);
          gl.bind_buffer(glow::PIXEL_UNPACK_BUFFER, prev_buffer(prev));
          return false;
        };
        gl.bind_buffer(glow::PIXEL_UNPACK_BUFFER, Some(buffer));
        // STREAM_DRAW: written once by the CPU, read once by the GPU.
        gl.buffer_data_size(glow::PIXEL_UNPACK_BUFFER, size as i32, glow::STREAM_DRAW);
        self.buffers.push(buffer);
      }
      gl.bind_buffer(glow::PIXEL_UNPACK_BUFFER, prev_buffer(prev));
    }
    self.size = size;
    self.next = 0;
    true
  }

  fn release(&mut self, gl: &glow::Context) {
    for buffer in self.buffers.drain(..) {
      unsafe { gl.delete_buffer(buffer) };
    }
    self.size = 0;
    self.next = 0;
    self.available = false;
  }
}
