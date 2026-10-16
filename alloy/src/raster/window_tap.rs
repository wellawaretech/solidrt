//! The window tap: the frame as it reached the window, or a cut-out of it,
//! read back into a `FrameTap` (the camera's slot type) after each drawn
//! frame, so a recorder takes the screen the way it takes a camera. The
//! GL side (the target, the pack buffer ring, the fenced read) is
//! `gl::TapReadback`; this is the entries' bookkeeping: the rect, the
//! output size, the issue after each frame's draw and the completion at
//! the next frame or after a refresh period of idleness, by which time the
//! fence has long signalled.
//!
//! A window at rest draws no frames and so produces none; the consumer
//! takes the gap as time (a recording holds the last frame). The tap's
//! output size is fixed at the attach: a resized window or rect scales
//! into it.

use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::time::Duration;

use super::RasterState;
use crate::camera::FrameTap;
use crate::gl::{window_samples, TapReadback, TapRect};

/// The size a tap was asked for: either dimension may be left to the aspect
/// of the source (`resolve_size`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TapSize {
  pub width: Option<u32>,
  pub height: Option<u32>,
}

/// How long a pending readback is allowed to stay in its buffer after the
/// frame that issued it, when no frame follows: the loop's wait between
/// commands while one is pending (see `pending_wait`). One refresh period
/// of a 60 Hz display, so a window that goes idle right after a change still
/// delivers that change within a frame's time.
const PENDING_WAIT: Duration = Duration::from_millis(16);

const RGBA_BYTES: usize = 4;

/// One tap's raster side: its slot, its rect, its output size, and its GL
/// objects once the first frame after the attach allocated them (again
/// after a context loss, which drops them).
pub(crate) struct WindowTapEntry {
  pub(crate) tap: FrameTap,
  pub(crate) rect: Option<TapRect>,
  pub(crate) width: u32,
  pub(crate) height: u32,
  readback: Option<TapReadback>,
  // Counters for the close log: frames issued and completed, the issue
  // (blit + read) and the map (fence check + copy + publish) time.
  issued: u64,
  completed: u64,
  issue_micros: u64,
  map_micros: u64,
}

impl WindowTapEntry {
  fn new(tap: FrameTap, rect: Option<TapRect>, width: u32, height: u32) -> WindowTapEntry {
    WindowTapEntry { tap, rect, width, height, readback: None, issued: 0, completed: 0, issue_micros: 0, map_micros: 0 }
  }

  /// Publish the mapped pixels (bottom-up, see `TapReadback::complete`)
  /// into the slot, top-down, in a buffer from the slot's spares.
  fn publish(&mut self, timestamp_ns: u64, mapped: &[u8]) {
    let row_bytes = self.width as usize * RGBA_BYTES;
    let mut pixels = self.tap.take_spare();
    pixels.clear();
    pixels.reserve(mapped.len());
    for row in mapped.chunks_exact(row_bytes).rev() {
      pixels.extend_from_slice(row);
    }
    self.completed += 1;
    self.tap.publish(timestamp_ns, self.width, self.height, pixels);
  }

  fn complete(&mut self, gl: &glow::Context, wait: bool) {
    let Some(mut readback) = self.readback.take() else { return };
    if readback.pending() {
      let start = std::time::Instant::now();
      readback.complete(gl, wait, &mut |ts, mapped| self.publish(ts, mapped));
      self.map_micros += start.elapsed().as_micros() as u64;
    }
    self.readback = Some(readback);
  }
}

/// The tap's size for a source of `src` pixels: both given = as asked, one
/// given = the other by the source's aspect, none = the source's size. Each
/// dimension is rounded up to even (I420 chroma is 2x2) and kept at least
/// 2; a source without size (a zero-sized window) yields None.
pub(crate) fn resolve_size(size: TapSize, src: (u32, u32)) -> Option<(u32, u32)> {
  let (sw, sh) = src;
  if sw == 0 || sh == 0 {
    return None;
  }
  let (w, h) = match (size.width, size.height) {
    (Some(w), Some(h)) => (w, h),
    (Some(w), None) => (w, (w as u64 * sh as u64 / sw as u64) as u32),
    (None, Some(h)) => ((h as u64 * sw as u64 / sh as u64) as u32, h),
    (None, None) => (sw, sh),
  };
  let even = |v: u32| v.max(2).div_ceil(2) * 2;
  Some((even(w), even(h)))
}

/// `rect` cut to a `width` x `height` surface; None when nothing remains.
pub(crate) fn clamp_rect(rect: TapRect, width: u32, height: u32) -> Option<TapRect> {
  let x1 = rect.x.saturating_add(rect.width).min(width);
  let y1 = rect.y.saturating_add(rect.height).min(height);
  if rect.x >= x1 || rect.y >= y1 {
    return None;
  }
  Some(TapRect { x: rect.x, y: rect.y, width: x1 - rect.x, height: y1 - rect.y })
}

impl RasterState {
  /// Attach a tap: the entry waits for the next drawn frame. The size is
  /// resolved against the rect, or the surface, as it stands now; the
  /// reply carries it, since the encoder is configured from it.
  pub(super) fn attach_window_tap(
    &mut self,
    id: u64,
    tap: FrameTap,
    rect: Option<TapRect>,
    size: TapSize,
  ) -> Result<(u32, u32), String> {
    let (sw, sh) = crate::backend::unpack_size(self.surface_size.load(Ordering::Acquire));
    let src = match rect {
      Some(r) => clamp_rect(r, sw, sh).map_or((0, 0), |r| (r.width, r.height)),
      None => (sw, sh),
    };
    let (width, height) = resolve_size(size, src).ok_or_else(|| "the window has no surface to record".to_string())?;
    self.window_taps.insert(id, WindowTapEntry::new(tap, rect, width, height));
    Ok((width, height))
  }

  /// Move a tap's cut-out (None = the whole window); the next frame reads
  /// the new rect.
  pub(super) fn set_window_tap_rect(&mut self, id: u64, rect: Option<TapRect>) {
    if let Some(entry) = self.window_taps.get_mut(&id) {
      entry.rect = rect;
    }
  }

  /// Detach a tap: its pending frame is completed (the slot's consumer may
  /// be waiting for exactly that one), the GL objects freed, the tap closed
  /// so every waiter on it ends.
  pub(super) fn detach_window_tap(&mut self, id: u64) {
    let Some(mut entry) = self.window_taps.remove(&id) else { return };
    entry.complete(&self.gl, true);
    if let Some(readback) = entry.readback.take() {
      readback.release(&self.gl);
    }
    entry.tap.close();
    if entry.issued > 0 {
      log::debug!(
        "[alloy] window tap closed: {} frames ({} completed), issue {:.2} ms, map {:.2} ms per frame",
        entry.issued,
        entry.completed,
        entry.issue_micros as f64 / 1000.0 / entry.issued as f64,
        entry.map_micros as f64 / 1000.0 / entry.completed.max(1) as f64
      );
    }
  }

  /// Whether a readback is in flight on any tap: the loop then waits with
  /// a timeout between commands, so an idle window still completes it.
  pub(super) fn window_tap_pending(&self) -> bool {
    self.window_taps.values().any(|e| e.readback.as_ref().is_some_and(|r| r.pending()))
  }

  /// The wait the loop uses between commands while a readback is pending.
  pub(super) fn pending_wait() -> Duration {
    PENDING_WAIT
  }

  /// Complete every pending readback whose fence has signalled. Called at
  /// the start of a frame and from the loop's idle timeout.
  pub(super) fn complete_window_taps(&mut self) {
    for entry in self.window_taps.values_mut() {
      entry.complete(&self.gl, false);
    }
  }

  /// Issue this frame's readback on every tap: blit the rect out of FBO 0
  /// into the tap's target and start the read. Called after the frame's
  /// draw, before the present.
  pub(super) fn issue_window_taps(&mut self, surface: (u32, u32), timestamp_ns: u64) {
    if self.window_taps.is_empty() {
      return;
    }
    let multisampled = window_samples(&self.gl) >= 2;
    for entry in self.window_taps.values_mut() {
      let start = std::time::Instant::now();
      // The rect as it stands against this frame's surface; a cut-out that
      // left the window records nothing this frame.
      let src = match entry.rect {
        Some(r) => match clamp_rect(r, surface.0, surface.1) {
          Some(r) => r,
          None => continue,
        },
        None => TapRect { x: 0, y: 0, width: surface.0, height: surface.1 },
      };
      if entry.readback.is_none() {
        match TapReadback::new(&self.gl, entry.width, entry.height) {
          Ok(readback) => entry.readback = Some(readback),
          Err(e) => {
            log::warn!("[alloy] window tap unavailable: {e}");
            entry.tap.close();
            continue;
          }
        }
      }
      let Some(mut readback) = entry.readback.take() else { continue };
      let issued = readback.issue(
        &self.gl,
        (surface.0 as i32, surface.1 as i32),
        src,
        multisampled,
        timestamp_ns,
        &mut |ts, mapped| entry.publish(ts, mapped),
      );
      entry.readback = Some(readback);
      match issued {
        Ok(()) => entry.issued += 1,
        Err(e) => log::warn!("[alloy] window tap readback failed: {e}"),
      }
      entry.issue_micros += start.elapsed().as_micros() as u64;
    }
  }
}

/// The taps carried across a context loss (see recover.rs): the same slots
/// and rects, no GL objects (the dead context's are gone with it), the
/// pending frame lost.
pub(crate) fn carry(taps: HashMap<u64, WindowTapEntry>) -> HashMap<u64, WindowTapEntry> {
  taps.into_iter().map(|(id, entry)| (id, WindowTapEntry { readback: None, ..entry })).collect()
}
