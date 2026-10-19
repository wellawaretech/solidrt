use crate::stepped::SteppedConfig;

// Operating mode of the app. Run is the normal interactive loop driven by the
// display. Stepped is the headless mode: a headless window (a framebuffer
// of the raster thread's at the size the run starts with, or the one the
// embedder sets per engine; see stepped.rs), no frame signal of its own -
// the embedder steps every frame (a test host, a render host) and reads the
// pixels it wants (a node capture, the window) - and a frame nobody
// demanded is not drawn. Prefer matching over assuming the cases.
pub enum Mode {
  Run,
  Stepped(SteppedConfig),
}

impl Mode {
  /// No display: a headless window, nothing presented, the process clock
  /// virtual (see clock.rs).
  pub fn is_headless(&self) -> bool {
    matches!(self, Mode::Stepped(_))
  }

  /// What the raster thread does with a drawn frame in this mode.
  pub(crate) fn frame_sink(&self) -> crate::raster::FrameSink {
    match self {
      Mode::Run => crate::raster::FrameSink::Window,
      Mode::Stepped(_) => crate::raster::FrameSink::Discard,
    }
  }
}
