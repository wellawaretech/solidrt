//! The camera thread: owns a session's SDL device from the permission grant
//! to the close, and turns every frame the device delivers into an upright
//! RGBA8 frame in the session's tap. `SDL_AcquireCameraFrame` never blocks
//! and SDL offers no wait, so the thread sleeps between empty polls: one
//! long sleep to just before the next frame is expected from the device's
//! negotiated rate, then short polls until it lands. SDL documents acquire,
//! release and close as callable from any thread; the device is used by one
//! thread at a time (the UI thread until the producer starts, the producer
//! until it exits), which is what `Device` asserts.

use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use sdl3::sys::camera::SDL_Camera;
use sdl3::sys::pixels::SDL_PIXELFORMAT_RGBA32;
use sdl3::sys::surface::SDL_Surface;

use super::tap::FrameTap;
use crate::sdl_utils;

/// The short poll while a frame is expected any moment: the latency a frame
/// can pick up before the thread sees it. A display refresh is 16 ms, so
/// this stays well inside one.
const POLL_INTERVAL: Duration = Duration::from_millis(2);

/// How early before a frame's expected arrival the long sleep ends and the
/// short polls begin: the cadence jitter of a camera plus the sleep's own
/// overshoot. The wakeups per frame are this over `POLL_INTERVAL`, plus
/// one.
const EARLY_MARGIN: Duration = Duration::from_millis(5);

/// An SDL camera device handle moved to the producer thread.
struct Device(*mut SDL_Camera);

// SDL's camera calls are documented safe from any thread, and the producer
// is the only thread touching the device once it holds it.
unsafe impl Send for Device {}

/// Start the producer for `camera`, filling `tap` until it closes. The
/// thread closes the device on its way out; join the handle after closing
/// the tap to have the device released when the close returns.
pub(super) fn spawn(camera: *mut SDL_Camera, tap: FrameTap) -> Result<JoinHandle<()>, String> {
  let device = Device(camera);
  std::thread::Builder::new()
    .name("sol-camera".into())
    .spawn(move || run(device, tap))
    .map_err(|e| format!("camera thread failed to start: {e}"))
}

fn run(device: Device, tap: FrameTap) {
  let camera = device.0;
  let period = frame_period(camera);
  let mut convert = Vec::new();
  // When the next frame should land, from the last one and the period;
  // None until the first frame, and while the period is unknown.
  let mut expected: Option<Instant> = None;
  while !tap.closed() {
    let Some((surface, timestamp_ns)) = newest_frame(camera) else {
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
    match upright_frame(surface, &mut convert, &mut pixels) {
      Ok((width, height)) => tap.publish(timestamp_ns, width, height, pixels),
      Err(e) => log::warn!("[camera] frame dropped: {e}"),
    }
    sdl_utils::camera_release_frame(camera, surface);
  }
  sdl_utils::camera_close(camera);
}

/// The device's frame interval from its negotiated format; None when SDL
/// reports no rate.
fn frame_period(camera: *mut SDL_Camera) -> Option<Duration> {
  let spec = sdl_utils::camera_format(camera)?;
  if spec.framerate_numerator <= 0 || spec.framerate_denominator <= 0 {
    return None;
  }
  Some(Duration::from_secs_f64(spec.framerate_denominator as f64 / spec.framerate_numerator as f64))
}

/// The newest frame the device has queued, releasing any older ones it
/// delivered since the last poll: the slot is latest-wins, so a backlog
/// behind a slow poll never reaches it.
fn newest_frame(camera: *mut SDL_Camera) -> Option<(*mut SDL_Surface, u64)> {
  let mut newest: Option<(*mut SDL_Surface, u64)> = None;
  loop {
    let (surface, timestamp_ns) = sdl_utils::camera_acquire_frame(camera);
    if surface.is_null() {
      return newest;
    }
    if let Some((older, _)) = newest.replace((surface, timestamp_ns)) {
      sdl_utils::camera_release_frame(camera, older);
    }
  }
}

/// Turn the delivered surface into a tightly packed upright RGBA8 frame in
/// `dst` (converted to RGBA32 through `convert` when the device delivers a
/// native format, rotated per SDL's per-frame rotation: mobile sensor plus
/// display orientation). Returns the upright size.
fn upright_frame(frame: *mut SDL_Surface, convert: &mut Vec<u8>, dst: &mut Vec<u8>) -> Result<(u32, u32), String> {
  let rotation = sdl_utils::surface_rotation_degrees(frame);
  let surface = unsafe { &*frame };
  let (frame_w, frame_h) = (surface.w as usize, surface.h as usize);
  let row_bytes = frame_w * 4;
  let (src, pitch): (*const u8, usize) = if surface.format == SDL_PIXELFORMAT_RGBA32 {
    (surface.pixels as *const u8, surface.pitch as usize)
  } else {
    convert.resize(row_bytes * frame_h, 0);
    if !sdl_utils::surface_to_rgba(surface, convert) {
      return Err(format!("conversion failed: {}", sdl_utils::sdl_error()));
    }
    (convert.as_ptr(), row_bytes)
  };
  upright_into(src, pitch, frame_w, frame_h, rotation, dst);
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
