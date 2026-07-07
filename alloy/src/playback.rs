use std::sync::mpsc;

use crate::backend::{Frame, RenderSurface};
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

// Lockstep capture loop: block for next DisplayList, draw, glReadPixels, write
// PPM, send FrameRendered with virtual frame index and configured fps. JS is
// kept on a fixed virtual clock by lib.rs deriving time from these values.
pub(crate) fn run_playback_loop(
  window: sdl3::video::Window,
  mut render_surface: Box<dyn RenderSurface>,
  rx: mpsc::Receiver<Frame>,
  event_tx: mpsc::Sender<AlloyEvent>,
  mut playback: PlaybackConfig,
) {
  let (w_px, h_px) = window.size_in_pixels();
  let width = w_px as usize;
  let height = h_px as usize;

  type GlReadPixelsFn = unsafe extern "C" fn(i32, i32, i32, i32, u32, u32, *mut std::ffi::c_void);
  type GlBindFramebufferFn = unsafe extern "C" fn(u32, u32);
  let gl_read_pixels: GlReadPixelsFn = unsafe {
    std::mem::transmute(
      sdl3::sys::video::SDL_GL_GetProcAddress(c"glReadPixels".as_ptr()).expect("Failed to load glReadPixels"),
    )
  };
  let gl_bind_framebuffer: GlBindFramebufferFn = unsafe {
    std::mem::transmute(
      sdl3::sys::video::SDL_GL_GetProcAddress(c"glBindFramebuffer".as_ptr()).expect("Failed to load glBindFramebuffer"),
    )
  };

  const GL_READ_FRAMEBUFFER: u32 = 0x8CA8;
  const GL_RGBA: u32 = 0x1908;
  const GL_UNSIGNED_BYTE: u32 = 0x1401;

  let mut rgba = vec![0u8; width * height * 4];

  for frame in 0..playback.frames {
    let mut sub = match rx.recv() {
      Ok(sub) => sub,
      Err(_) => break,
    };

    // Order the readback after the UI thread's frame work on the GPU.
    render_surface.consume_fence(sub.fence.take(), true);
    render_surface.draw_display_list(&sub.dl).expect("Failed to draw display list");

    unsafe {
      gl_bind_framebuffer(GL_READ_FRAMEBUFFER, 0);
      gl_read_pixels(
        0,
        0,
        width as i32,
        height as i32,
        GL_RGBA,
        GL_UNSIGNED_BYTE,
        rgba.as_mut_ptr() as *mut std::ffi::c_void,
      );
    }

    write_png(&playback.output_prefix, frame, width, height, &rgba);

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
