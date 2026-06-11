struct SdlLogger;

impl log::Log for SdlLogger {
  fn enabled(&self, metadata: &log::Metadata) -> bool {
    let t = metadata.target();
    if t.starts_with("alloy") || t.starts_with("flux") || t.starts_with("lattice") {
      true
    } else if t.starts_with("whisper_rs") {
      // whisper.cpp/ggml route through the whisper_rs log hooks and are chatty
      // at info: full model dumps on load plus several lines per VAD call
      // (every 100ms while a recognizer listens).
      metadata.level() <= log::Level::Warn
    } else {
      metadata.level() <= log::Level::Info
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
  let default_level = if cfg!(target_os = "android") { log::LevelFilter::Info } else { log::LevelFilter::Debug };
  let level = std::env::var("SRT_LOG").ok().and_then(|s| s.parse().ok()).unwrap_or(default_level);
  log::set_max_level(level);
}
