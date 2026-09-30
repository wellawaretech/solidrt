use std::sync::mpsc;

use crate::event::{playback_init_events, AlloyCommand};

pub struct SteppedConfig {
  // The refresh rate the app is told, which is the rate the embedder steps
  // its virtual clock at.
  pub fps: u32,
}

// The loop of stepped mode: headless like playback, but it emits no frame
// signal. The embedder steps every frame itself (it sets the virtual clock
// with `clock::set_virtual_ns` and runs its frame work), so the demand gate
// stays in force and a frame nobody demanded is not drawn. What is left for
// this thread is the one command an engine start depends on: every engine
// the embedder builds asks for the init events, and gets the ones a headless
// surface has (see playback_init_events). Returns when the embedder is done
// and has dropped its command sender.
pub(crate) fn run_stepped_loop(
  window: sdl3::video::Window,
  cmd_rx: mpsc::Receiver<AlloyCommand>,
  event_tx: crate::EventSender,
  config: SteppedConfig,
) {
  while let Ok(cmd) = cmd_rx.recv() {
    if let AlloyCommand::EmitInitEvents = cmd {
      for event in playback_init_events(&window, config.fps) {
        event_tx.send(event).ok();
      }
    }
  }
}
