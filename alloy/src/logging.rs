struct SdlLogger;

impl log::Log for SdlLogger {
  fn enabled(&self, metadata: &log::Metadata) -> bool {
    let t = metadata.target();
    if t.starts_with("alloy") || t.starts_with("flux") || t.starts_with("forge") || t.starts_with("lattice") {
      // Our own crates log at every level.
      true
    } else {
      // Third-party crates (notably the iroh p2p stack: magicsock, quinn, relay,
      // net-report, portmapper) are very chatty at info; keep only their warnings.
      // Raise with SOLIDRT_LOG=debug when diagnosing a specific subsystem.
      metadata.level() <= log::Level::Warn
    }
  }
  fn log(&self, record: &log::Record) {
    if self.enabled(record.metadata()) {
      sdl3::log::log(&format!("{}", record.args()));
    }
  }
  fn flush(&self) {}
}

static SDL_LOGGER: SdlLogger = SdlLogger;

pub fn install_logger() {
  log::set_logger(&SDL_LOGGER).ok();
  let level = std::env::var("SOLIDRT_LOG").ok().and_then(|s| s.parse().ok()).unwrap_or(log::LevelFilter::Info);
  log::set_max_level(level);
}
