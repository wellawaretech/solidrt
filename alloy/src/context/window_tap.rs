//! Window taps (see raster/window_tap.rs): the UI-side open, move and close
//! of a readback of the window, or a cut-out of it, into a `FrameTap`.

use std::cell::Cell;

use super::Context;
use crate::camera::FrameTap;
use crate::gl::TapRect;
use crate::raster::{RasterCmd, TapSize};

/// An open window tap: its id (for `set_window_tap_rect` and
/// `close_window_tap`), its slot, and the size its frames have.
pub struct WindowTap {
  pub id: u64,
  pub tap: FrameTap,
  pub width: u32,
  pub height: u32,
}

#[derive(Default)]
pub(crate) struct WindowTapRegistry {
  next_id: Cell<u64>,
}

impl Context {
  /// Open a tap on the window: every frame drawn from now on has `rect`
  /// (device pixels, top-left origin; None = the whole window) read back,
  /// scaled to `size` (resolved against the rect or the window as it
  /// stands now, see `TapSize`), into the tap, on the raster thread,
  /// asynchronously. The slot is the camera's: a consumer waits on it the
  /// same way, and reads `Closed` once the tap is closed. One blocking
  /// round trip, like a create. Errs when the window has no surface.
  pub fn open_window_tap(&self, rect: Option<TapRect>, size: TapSize) -> Result<WindowTap, String> {
    let id = self.window_taps.next_id.get() + 1;
    self.window_taps.next_id.set(id);
    let tap = FrameTap::new();
    let (width, height) =
      self.rpc(|reply| RasterCmd::AttachWindowTap { id, tap: tap.clone(), rect, size, reply })??;
    Ok(WindowTap { id, tap, width, height })
  }

  /// Move the tap's cut-out (None = the whole window); the next frame reads
  /// the new rect, scaled into the tap's fixed size.
  pub fn set_window_tap_rect(&self, id: u64, rect: Option<TapRect>) {
    self.send(RasterCmd::SetWindowTapRect { id, rect });
  }

  /// Close the tap: the frame in flight is delivered, then the slot closes
  /// and every waiter on it ends.
  pub fn close_window_tap(&self, id: u64) {
    self.send(RasterCmd::DetachWindowTap { id });
  }
}
