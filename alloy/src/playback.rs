use std::sync::mpsc;

use crate::backend::FrameOutput;
use crate::event::AlloyEvent;
use crate::script::ScriptPlayer;

pub struct PlaybackConfig {
  pub fps: u32,
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
pub(crate) fn run_playback_loop(
  window: sdl3::video::Window,
  rx: mpsc::Receiver<FrameOutput>,
  event_tx: mpsc::Sender<AlloyEvent>,
  mut playback: PlaybackConfig,
) {
  let (w_px, h_px) = window.size_in_pixels();
  let width = w_px as usize;
  let height = h_px as usize;

  for frame in 0..playback.frames {
    let rgba = match rx.recv() {
      Ok(FrameOutput::Captured(rgba)) => rgba,
      // Presented is interactive-only; a closed channel means the raster
      // thread is gone.
      Ok(FrameOutput::Presented) | Err(_) => break,
    };
    if rgba.len() == width * height * 4 {
      write_png(&playback.output_prefix, frame, width, height, &rgba);
    } else {
      // A skipped draw (wrap_fbo failure) sends an empty readback to keep the
      // lockstep alive; drop the frame rather than encode garbage.
      log::error!("[alloy] frame {frame} readback is {} bytes, expected {}", rgba.len(), width * height * 4);
    }

    // Scripted input due for the NEXT frame must reach the UI thread before
    // the FrameRendered below, which is what triggers that frame's build:
    // same channel, so send order is receive order.
    for scripted in playback.script.due((frame + 1) as f64 / playback.fps as f64) {
      event_tx.send(scripted).ok();
    }

    let time = frame as f64 / playback.fps as f64;
    event_tx.send(AlloyEvent::FrameRendered { frame, fps: playback.fps, time }).ok();

    let frame_number = frame + 1;
    if frame_number % playback.fps as u64 == 0 {
      log::info!("[alloy] recorded {} frames", frame_number);
    }
  }

  log::info!("[alloy] recording complete ({} frames)", playback.frames);
  std::process::exit(0);
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
