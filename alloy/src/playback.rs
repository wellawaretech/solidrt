use std::sync::mpsc;

use crate::backend::FrameOutput;
use crate::event::AlloyEvent;
use crate::raster::RasterSender;
use crate::script::ScriptPlayer;

pub struct PlaybackConfig {
  pub fps: u32,
  // Frames written. The mount frame drawn ahead of them is not one of them
  // (see run_playback_loop).
  pub frames: u64,
  pub output_prefix: String,
  // Scripted input replayed deterministically against the virtual frame
  // clock; empty when no script was given (a pure time-based capture).
  pub script: ScriptPlayer,
}

// Lockstep capture loop: block until the raster thread (which owns the
// process's single GL context) has drawn the next frame into the hidden
// window's backbuffer and read it back, write PNG, send FrameRendered with
// virtual frame index and configured fps. JS is kept on a fixed virtual clock
// by lib.rs deriving time from these values. The raster thread never drops
// frames in capture mode, so the one-Captured-per-submit contract holds.
//
// Draw 0 is the mount frame: the app's bootstrap paints it before any frame
// callback has run, so it shows what the components mounted, not what they
// animate. It is drawn (its FrameRendered is what starts the clock) but not
// written: PNG k is draw k + 1, the state after the app's (k + 1)th frame
// callback at time (k + 1) / fps, so every written frame is one a frame
// callback has shaped. A static app renders the same PNGs either way.
//
// Nothing may be drawing when this returns: dropping `window` unloads the
// EGL library, and the embedder exits the process right after, and either
// pulls the driver out from under a draw still encoding (a fault in the
// driver or in the thread's GL teardown, an abort from Impeller's encoder
// check). So the last written draw gets no FrameRendered, which would have
// the UI thread build one more frame (playback renders unconditionally),
// and the raster thread is fenced before the return.
pub(crate) fn run_playback_loop(
  window: sdl3::video::Window,
  rx: mpsc::Receiver<FrameOutput>,
  event_tx: mpsc::Sender<AlloyEvent>,
  raster: &RasterSender,
  mut playback: PlaybackConfig,
) -> Result<(), String> {
  let (w_px, h_px) = window.size_in_pixels();
  let width = w_px as usize;
  let height = h_px as usize;

  let mut written: u64 = 0;
  for draw in 0..=playback.frames {
    let rgba = match rx.recv() {
      Ok(FrameOutput::Captured(rgba)) => rgba,
      // Presented is interactive-only; a closed channel means the raster
      // thread is gone.
      Ok(FrameOutput::Presented) | Err(_) => break,
    };
    if draw > 0 {
      let frame = draw - 1;
      if rgba.len() == width * height * 4 {
        write_png(&playback.output_prefix, frame, width, height, &rgba);
        written += 1;
        if written % playback.fps as u64 == 0 {
          log::info!("[alloy] recorded {written} frames");
        }
      } else {
        // A skipped draw (wrap_fbo failure) sends an empty readback to keep
        // the lockstep alive; drop the frame rather than encode garbage.
        log::error!("[alloy] frame {frame} readback is {} bytes, expected {}", rgba.len(), width * height * 4);
      }
    }
    if draw == playback.frames {
      break;
    }

    // Scripted input due for the NEXT draw must reach the UI thread before
    // the FrameRendered below, which is what triggers that draw's build:
    // same channel, so send order is receive order.
    for scripted in playback.script.due((draw + 1) as f64 / playback.fps as f64) {
      event_tx.send(scripted).ok();
    }

    let time = draw as f64 / playback.fps as f64;
    event_tx.send(AlloyEvent::FrameRendered { frame: draw, fps: playback.fps, time }).ok();
  }
  raster.drain();

  log::info!("[alloy] recording complete ({written} of {} frames)", playback.frames);
  // An incomplete capture means the app failed to produce some frame (threw
  // at startup, raster thread died, readbacks failed). Err surfaces through
  // App::run to the embedder, whose exit code headless callers use as a
  // verification gate - so only a full capture reads as success, and the
  // exit itself stays out of library code.
  if written < playback.frames {
    return Err(format!("only {written} of {} frames were written", playback.frames));
  }
  Ok(())
}

// Lossless PNG. GL gives RGBA bottom-up, image expects top-down, so we flip
// rows before encoding.
fn write_png(prefix: &str, frame: u64, width: usize, height: usize, rgba: &[u8]) {
  let path = format!("{}-{:06}.png", prefix, frame);
  let row_bytes = width * 4;
  let mut flipped = Vec::with_capacity(rgba.len());
  for y in (0..height).rev() {
    let start = y * row_bytes;
    flipped.extend_from_slice(&rgba[start..start + row_bytes]);
  }
  image::save_buffer(&path, &flipped, width as u32, height as u32, image::ColorType::Rgba8)
    .expect("Failed to save PNG");
}
