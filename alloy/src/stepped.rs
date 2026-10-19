use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc};

use crate::event::{headless_init_events, AlloyCommand};

pub struct SteppedConfig {
  // The refresh rate the app is told, which is the rate the embedder steps
  // its virtual clock at.
  pub fps: u32,
}

/// The headless window as the app sees it: a logical size and a display
/// scale, which is `width * scale` by `height * scale` pixels of window
/// framebuffer (the raster thread's; see `RasterState::window_framebuffer`).
/// The scale is 1 unless the embedder sets another: the mode exists for
/// machine-independent frames, so the host display's scale never leaks into
/// layout, and a layout box maps to output pixels 1:1 by default.
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct HeadlessWindow {
  pub(crate) width: u32,
  pub(crate) height: u32,
  pub(crate) scale: f32,
}

impl HeadlessWindow {
  /// The window framebuffer's size in pixels. The embedder sends whole
  /// products (lattice validates them), so the rounding only absorbs float
  /// error.
  pub(crate) fn pixels(&self) -> (u32, u32) {
    ((self.width as f32 * self.scale).round() as u32, (self.height as f32 * self.scale).round() as u32)
  }
}

// The loop of stepped mode, the headless mode: it emits no frame signal.
// The embedder steps every frame itself (it sets the virtual clock with
// `clock::set_virtual_ns` and runs its frame work), so the demand gate stays
// in force and a frame nobody demanded is not drawn; the pixels it wants it
// asks for (`Context::read_window`, a node capture). What is left for this
// thread is the window as a fact: every engine the embedder builds asks for
// the init events and gets the ones a headless window has (see
// headless_init_events), at the size the run was started with; between
// those, the embedder may set another size and scale (a test's window),
// published to the raster thread the way a window resize is (the surface
// size atomic, read at the next frame or readback) and told to the app as a
// resize. The SDL window itself is never drawn to headless; it is held so
// the surface keeping the context current outlives every draw. Returns when
// the embedder is done and has dropped its command sender.
pub(crate) fn run_stepped_loop(
  window: sdl3::video::Window,
  cmd_rx: mpsc::Receiver<AlloyCommand>,
  event_tx: crate::EventSender,
  config: SteppedConfig,
  surface_size: Arc<AtomicU64>,
) {
  let (width, height) = window.size_in_pixels();
  let base = HeadlessWindow { width, height, scale: 1.0 };
  let mut current = base;
  let publish = |window: &HeadlessWindow| {
    let (w, h) = window.pixels();
    surface_size.store(crate::backend::pack_size(w, h), Ordering::Release);
  };
  while let Ok(cmd) = cmd_rx.recv() {
    match cmd {
      // Every engine starts at the run's window: the size a previous engine
      // set does not carry over.
      AlloyCommand::EmitInitEvents => {
        if current != base {
          current = base;
          publish(&current);
        }
        for event in headless_init_events(&current, config.fps) {
          event_tx.send(event).ok();
        }
      }
      AlloyCommand::SetHeadlessWindow { width, height, scale } => {
        current = HeadlessWindow { width, height, scale };
        publish(&current);
        event_tx.send(crate::event::headless_resize_event(&current)).ok();
      }
      // No surface to show one on.
      AlloyCommand::ShowDialog { respond, .. } => respond(Err(crate::dialog::DialogError::Unavailable)),
      _ => {}
    }
  }
}
