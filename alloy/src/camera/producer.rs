//! The camera thread: owns a session's camera (`ore::camera::Camera`) from
//! the permission grant to the close, and turns every frame the device
//! delivers into an upright RGBA8 frame in the session's tap. Acquiring a
//! frame never blocks and SDL offers no wait, so the thread sleeps between
//! empty polls: one long sleep to just before the next frame is expected
//! from the device's negotiated rate, then short polls until it lands. The
//! camera is used by one thread at a time (the UI thread until the producer
//! starts, the producer until it exits), which is why it is `Send` and not
//! `Sync`.

use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use super::tap::FrameTap;

/// The short poll while a frame is expected any moment: the latency a frame
/// can pick up before the thread sees it. A display refresh is 16 ms, so
/// this stays well inside one.
const POLL_INTERVAL: Duration = Duration::from_millis(2);

/// How early before a frame's expected arrival the long sleep ends and the
/// short polls begin: the cadence jitter of a camera plus the sleep's own
/// overshoot. The wakeups per frame are this over `POLL_INTERVAL`, plus
/// one.
const EARLY_MARGIN: Duration = Duration::from_millis(5);

/// Start the producer for `camera`, filling `tap` until it closes. The
/// thread closes the device on its way out; join the handle after closing
/// the tap to have the device released when the close returns.
pub(super) fn spawn(camera: ore::camera::Camera, tap: FrameTap) -> Result<JoinHandle<()>, String> {
  std::thread::Builder::new()
    .name("sol-camera".into())
    .spawn(move || run(camera, tap))
    .map_err(|e| format!("camera thread failed to start: {e}"))
}

fn run(camera: ore::camera::Camera, tap: FrameTap) {
  let period = camera.frame_period();
  let mut convert = Vec::new();
  // When the next frame should land, from the last one and the period;
  // None until the first frame, and while the period is unknown.
  let mut expected: Option<Instant> = None;
  while !tap.closed() {
    let Some(frame) = newest_frame(&camera) else {
      let now = Instant::now();
      match (expected, period) {
        // Well before the frame is due: one sleep to the margin.
        (Some(due), Some(_)) if now + EARLY_MARGIN < due => std::thread::sleep(due - EARLY_MARGIN - now),
        // A full period overdue (the device stalled or paused): expect it a
        // period from now rather than polling short until it comes back.
        (Some(due), Some(period)) if now > due + period => expected = Some(now + period),
        _ => std::thread::sleep(POLL_INTERVAL),
      }
      continue;
    };
    expected = period.map(|period| Instant::now() + period);
    let mut pixels = tap.take_spare();
    match upright_frame(&frame, &mut convert, &mut pixels) {
      Ok((width, height)) => tap.publish(frame.timestamp_ns(), width, height, pixels),
      Err(e) => log::warn!("[camera] frame dropped: {e}"),
    }
    // Released to the device here, after publishing.
    drop(frame);
  }
  // The camera closes as it drops on the way out.
}

/// The newest frame the device has queued, releasing any older ones it
/// delivered since the last poll: the slot is latest-wins, so a backlog
/// behind a slow poll never reaches it.
fn newest_frame(camera: &ore::camera::Camera) -> Option<ore::camera::Frame<'_>> {
  let mut newest = None;
  while let Some(frame) = camera.acquire() {
    // Replacing an older frame drops it, which releases it.
    newest = Some(frame);
  }
  newest
}

/// Turn the delivered frame into a tightly packed upright RGBA8 frame in
/// `dst` (converted to RGBA32 through `convert` when the device delivers a
/// native format, rotated per SDL's per-frame rotation: mobile sensor plus
/// display orientation). Returns the upright size.
fn upright_frame(
  frame: &ore::camera::Frame<'_>,
  convert: &mut Vec<u8>,
  dst: &mut Vec<u8>,
) -> Result<(u32, u32), String> {
  let rotation = frame.rotation_degrees();
  let (frame_w, frame_h) = (frame.width() as usize, frame.height() as usize);
  let (rows, pitch) = frame.rgba(convert)?;
  upright_into(rows.as_ptr(), pitch, frame_w, frame_h, rotation, dst);
  let upright = if rotation == 90 || rotation == 270 { (frame_h, frame_w) } else { (frame_w, frame_h) };
  Ok((upright.0 as u32, upright.1 as u32))
}

/// Pixels per side of the tiles a quarter-turn rotation works in: the
/// destination rows a tile touches stay in cache across the tile's source
/// rows, where a plain row walk writes one pixel per cache line.
const ROTATE_TILE: usize = 16;

/// Copy a `pitch`-strided RGBA8 image into `dst` rotated clockwise by
/// `rotation` degrees, tightly packed (also flattens any pitch padding).
pub(crate) fn upright_into(base: *const u8, pitch: usize, w: usize, h: usize, rotation: u32, dst: &mut Vec<u8>) {
  dst.resize(w * h * 4, 0);
  let row = |r: usize| unsafe { std::slice::from_raw_parts(base.add(r * pitch), w * 4) };
  match rotation {
    // (r, c) -> (c, h-1-r), rotated row length is h; and
    // (r, c) -> (w-1-c, r), the same, by tiles.
    90 | 270 => {
      for r0 in (0..h).step_by(ROTATE_TILE) {
        for c0 in (0..w).step_by(ROTATE_TILE) {
          for r in r0..(r0 + ROTATE_TILE).min(h) {
            let src = row(r);
            for c in c0..(c0 + ROTATE_TILE).min(w) {
              let di = if rotation == 90 { (c * h + (h - 1 - r)) * 4 } else { ((w - 1 - c) * h + r) * 4 };
              dst[di..di + 4].copy_from_slice(&src[c * 4..c * 4 + 4]);
            }
          }
        }
      }
    }
    // (r, c) -> (h-1-r, w-1-c): each row reversed into its mirror row.
    180 => {
      for r in 0..h {
        let out = &mut dst[(h - 1 - r) * w * 4..(h - r) * w * 4];
        for (o, i) in out.chunks_exact_mut(4).rev().zip(row(r).chunks_exact(4)) {
          o.copy_from_slice(i);
        }
      }
    }
    // Pitch repack only.
    _ => {
      for r in 0..h {
        dst[r * w * 4..(r + 1) * w * 4].copy_from_slice(row(r));
      }
    }
  }
}
