//! The GL half of the window tap (see raster/window_tap.rs): a retained
//! single-sample target of the tap's size, a ring of pixel pack buffers and
//! the readback in flight. `issue` blits a rect of the window framebuffer
//! (FBO 0, or the headless window target) into the target (scaled,
//! `LINEAR`) and starts the read into the ring's next buffer behind a fence;
//! `complete` maps the buffer once the fence signalled and hands the pixels
//! out. Nothing here waits on the GPU in the frame path: the read is
//! asynchronous and the map comes a frame later.
//!
//! A multisampled FBO 0 (the Android in-tile path) cannot be blitted with a
//! scale, so it resolves 1:1 into a retained full-size intermediate first,
//! and the scaled blit reads that. No reversed-Y blit is issued anywhere
//! (see `render_display_list_to_window`): the rows come out bottom-up and
//! the consumer reverses them in its copy out of the mapped buffer, which
//! costs nothing extra.

use glow::HasContext;

use super::prev_buffer;
use super::rig::{prev_framebuffer, prev_renderbuffer};

/// Pack buffers in the ring: one the GPU reads into while the previous one
/// is mapped, and a spare so a frame issued right after an idle completion
/// never reuses a buffer still in flight.
const PACK_RING: usize = 3;

/// The bounded wait on a readback's fence when it is taken before its
/// natural time (a detach): a GPU that has not finished a frame's copy by
/// then is stalled, and the frame is dropped.
const FENCE_TIMEOUT_NS: i32 = 100_000_000;

const RGBA_BYTES: usize = 4;

/// A cut-out of the window in device pixels, top-left origin.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TapRect {
  pub x: u32,
  pub y: u32,
  pub width: u32,
  pub height: u32,
}

/// One tap's GL objects, at a fixed output size.
pub(crate) struct TapReadback {
  width: u32,
  height: u32,
  target: glow::NativeRenderbuffer,
  target_fbo: glow::NativeFramebuffer,
  // The 1:1 resolve of a multisampled FBO 0, at the surface size it was
  // made for; None on a single-sample window.
  resolve: Option<(glow::NativeRenderbuffer, glow::NativeFramebuffer, (i32, i32))>,
  ring: Vec<glow::Buffer>,
  next: usize,
  pending: Option<Pending>,
}

/// A readback in flight: its buffer, its fence, and the time the frame it
/// carries was drawn at.
struct Pending {
  buffer: usize,
  fence: glow::Fence,
  timestamp_ns: u64,
}

impl TapReadback {
  /// The target and the ring at `width` x `height`.
  pub(crate) fn new(gl: &glow::Context, width: u32, height: u32) -> Result<TapReadback, String> {
    let (target, target_fbo) = color_target(gl, width as i32, height as i32)?;
    let len = width as usize * height as usize * RGBA_BYTES;
    let mut this =
      TapReadback { width, height, target, target_fbo, resolve: None, ring: Vec::new(), next: 0, pending: None };
    unsafe {
      let prev = gl.get_parameter_i32(glow::PIXEL_PACK_BUFFER_BINDING);
      for _ in 0..PACK_RING {
        match gl.create_buffer() {
          Ok(buffer) => {
            gl.bind_buffer(glow::PIXEL_PACK_BUFFER, Some(buffer));
            // STREAM_READ: written once by the GPU, read once by the CPU.
            gl.buffer_data_size(glow::PIXEL_PACK_BUFFER, len as i32, glow::STREAM_READ);
            this.ring.push(buffer);
          }
          Err(e) => {
            gl.bind_buffer(glow::PIXEL_PACK_BUFFER, prev_buffer(prev));
            this.release(gl);
            return Err(format!("pack buffer: {e}"));
          }
        }
      }
      gl.bind_buffer(glow::PIXEL_PACK_BUFFER, prev_buffer(prev));
    }
    Ok(this)
  }

  pub(crate) fn pending(&self) -> bool {
    self.pending.is_some()
  }

  /// Blit `src` out of the window framebuffer (`window`: None is FBO 0, Some
  /// the headless window target; a `surface`-sized, bottom-up framebuffer,
  /// `multisampled` or not) into the target and start the read into the
  /// ring's next buffer, stamped `timestamp_ns`. A readback still pending
  /// is completed first (waited for, bounded), so no frame is lost to the
  /// ring's rotation; `publish` receives it like a `complete`.
  #[allow(clippy::too_many_arguments)]
  pub(crate) fn issue(
    &mut self,
    gl: &glow::Context,
    window: Option<glow::NativeFramebuffer>,
    surface: (i32, i32),
    src: TapRect,
    multisampled: bool,
    timestamp_ns: u64,
    publish: &mut dyn FnMut(u64, &[u8]),
  ) -> Result<(), String> {
    if self.pending.is_some() {
      self.complete(gl, true, publish);
    }
    let (sw, sh) = surface;
    // GL's bottom-up rect of the top-left `src`.
    let (x0, x1) = (src.x as i32, (src.x + src.width) as i32);
    let (y0, y1) = (sh - (src.y + src.height) as i32, sh - src.y as i32);
    let (width, height) = (self.width as i32, self.height as i32);
    unsafe {
      let prev_read = gl.get_parameter_i32(glow::READ_FRAMEBUFFER_BINDING);
      let prev_draw = gl.get_parameter_i32(glow::DRAW_FRAMEBUFFER_BINDING);
      let prev_pack = gl.get_parameter_i32(glow::PIXEL_PACK_BUFFER_BINDING);
      let result = (|| -> Result<(), String> {
        let mut read_from = window;
        if multisampled {
          let fbo = self.ensure_resolve(gl, (sw, sh))?;
          gl.bind_framebuffer(glow::READ_FRAMEBUFFER, window);
          gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, Some(fbo));
          gl.blit_framebuffer(x0, y0, x1, y1, x0, y0, x1, y1, glow::COLOR_BUFFER_BIT, glow::NEAREST);
          read_from = Some(fbo);
        }
        gl.bind_framebuffer(glow::READ_FRAMEBUFFER, read_from);
        gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, Some(self.target_fbo));
        gl.blit_framebuffer(x0, y0, x1, y1, 0, 0, width, height, glow::COLOR_BUFFER_BIT, glow::LINEAR);
        gl.bind_framebuffer(glow::READ_FRAMEBUFFER, Some(self.target_fbo));
        let slot = self.next;
        self.next = (self.next + 1) % self.ring.len();
        gl.bind_buffer(glow::PIXEL_PACK_BUFFER, Some(self.ring[slot]));
        gl.read_pixels(0, 0, width, height, glow::RGBA, glow::UNSIGNED_BYTE, glow::PixelPackData::BufferOffset(0));
        let fence = gl.fence_sync(glow::SYNC_GPU_COMMANDS_COMPLETE, 0).map_err(|e| format!("fence: {e}"))?;
        // The fence must reach the GPU with the read (see `present` for the
        // ANGLE/D3D11 deferral this guards against).
        gl.flush();
        self.pending = Some(Pending { buffer: slot, fence, timestamp_ns });
        Ok(())
      })();
      gl.bind_buffer(glow::PIXEL_PACK_BUFFER, prev_buffer(prev_pack));
      gl.bind_framebuffer(glow::READ_FRAMEBUFFER, prev_framebuffer(prev_read));
      gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, prev_framebuffer(prev_draw));
      result
    }
  }

  /// Take the pending readback when its fence has signalled (or after a
  /// bounded wait when `wait`): map the buffer and hand the mapped pixels
  /// (`width` x `height` RGBA8, bottom-up rows as GL stores them, valid for
  /// the call only) to `publish` with the frame's stamp. A fence not yet
  /// signalled stays pending unless waited for; a timed-out one is dropped
  /// with its frame. True when a frame was published.
  pub(crate) fn complete(&mut self, gl: &glow::Context, wait: bool, publish: &mut dyn FnMut(u64, &[u8])) -> bool {
    let Some(pending) = self.pending.as_ref() else { return false };
    let timeout = if wait { FENCE_TIMEOUT_NS } else { 0 };
    let status = unsafe { gl.client_wait_sync(pending.fence, glow::SYNC_FLUSH_COMMANDS_BIT, timeout) };
    let signalled = matches!(status, glow::ALREADY_SIGNALED | glow::CONDITION_SATISFIED);
    if !signalled && !wait {
      return false;
    }
    let pending = self.pending.take().expect("checked above");
    unsafe { gl.delete_sync(pending.fence) };
    if !signalled {
      log::debug!("[alloy] window tap readback not finished in time; frame dropped");
      return false;
    }
    let len = self.width as usize * self.height as usize * RGBA_BYTES;
    let published = unsafe {
      let prev = gl.get_parameter_i32(glow::PIXEL_PACK_BUFFER_BINDING);
      gl.bind_buffer(glow::PIXEL_PACK_BUFFER, Some(self.ring[pending.buffer]));
      let ptr = gl.map_buffer_range(glow::PIXEL_PACK_BUFFER, 0, len as i32, glow::MAP_READ_BIT);
      let published = if ptr.is_null() {
        log::warn!("[alloy] window tap: mapping the pack buffer failed; frame dropped");
        false
      } else {
        let mapped = std::slice::from_raw_parts(ptr as *const u8, len);
        publish(pending.timestamp_ns, mapped);
        gl.unmap_buffer(glow::PIXEL_PACK_BUFFER);
        true
      };
      gl.bind_buffer(glow::PIXEL_PACK_BUFFER, prev_buffer(prev));
      published
    };
    published
  }

  /// Free everything, the pending readback's fence included.
  pub(crate) fn release(self, gl: &glow::Context) {
    unsafe {
      if let Some(pending) = self.pending {
        gl.delete_sync(pending.fence);
      }
      for buffer in self.ring {
        gl.delete_buffer(buffer);
      }
      if let Some((rbo, fbo, _)) = self.resolve {
        gl.delete_framebuffer(fbo);
        gl.delete_renderbuffer(rbo);
      }
      gl.delete_framebuffer(self.target_fbo);
      gl.delete_renderbuffer(self.target);
    }
  }

  /// The full-size single-sample resolve target for a multisampled FBO 0,
  /// (re)allocated at the surface size.
  fn ensure_resolve(&mut self, gl: &glow::Context, surface: (i32, i32)) -> Result<glow::NativeFramebuffer, String> {
    if let Some((rbo, fbo, size)) = self.resolve {
      if size == surface {
        return Ok(fbo);
      }
      unsafe {
        gl.delete_framebuffer(fbo);
        gl.delete_renderbuffer(rbo);
      }
      self.resolve = None;
    }
    let (rbo, fbo) = color_target(gl, surface.0, surface.1)?;
    self.resolve = Some((rbo, fbo, surface));
    Ok(fbo)
  }
}

/// A single-sample RGBA8 renderbuffer in a framebuffer of its own.
fn color_target(
  gl: &glow::Context,
  width: i32,
  height: i32,
) -> Result<(glow::NativeRenderbuffer, glow::NativeFramebuffer), String> {
  unsafe {
    let prev_rbo = gl.get_parameter_i32(glow::RENDERBUFFER_BINDING);
    let prev_fbo = gl.get_parameter_i32(glow::DRAW_FRAMEBUFFER_BINDING);
    let rbo = gl.create_renderbuffer().map_err(|e| format!("renderbuffer: {e}"))?;
    gl.bind_renderbuffer(glow::RENDERBUFFER, Some(rbo));
    gl.renderbuffer_storage(glow::RENDERBUFFER, glow::RGBA8, width, height);
    gl.bind_renderbuffer(glow::RENDERBUFFER, prev_renderbuffer(prev_rbo));
    let fbo = match gl.create_framebuffer() {
      Ok(fbo) => fbo,
      Err(e) => {
        gl.delete_renderbuffer(rbo);
        return Err(format!("framebuffer: {e}"));
      }
    };
    gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, Some(fbo));
    gl.framebuffer_renderbuffer(glow::DRAW_FRAMEBUFFER, glow::COLOR_ATTACHMENT0, glow::RENDERBUFFER, Some(rbo));
    let status = gl.check_framebuffer_status(glow::DRAW_FRAMEBUFFER);
    gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, prev_framebuffer(prev_fbo));
    if status != glow::FRAMEBUFFER_COMPLETE {
      gl.delete_framebuffer(fbo);
      gl.delete_renderbuffer(rbo);
      return Err(format!("tap framebuffer incomplete: {status:#x}"));
    }
    Ok((rbo, fbo))
  }
}
