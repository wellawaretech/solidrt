//! Engine-free logging sink.
//!
//! Names no scripting-engine types. A `Logger` is a cloneable handle over a log
//! callback; capability cores take one and call `warn`/`error` on it. The host
//! (flux) supplies the callback and owns the engine-specific glue (storing the
//! logger in JS context userdata, formatting JS exceptions) on top of this.

use std::sync::Arc;

/// Log level passed to the logger callback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
  Debug,
  Log,
  Warn,
  Error,
}

/// Shared log sink: a cloneable handle over a `(level, message)` callback.
#[derive(Clone)]
pub struct Logger(pub Arc<dyn Fn(LogLevel, &str) + Send + Sync>);

impl Logger {
  /// Build a logger from a log callback.
  pub fn new(sink: LogFn) -> Self {
    Logger(Arc::from(sink))
  }

  pub fn debug(&self, msg: &str) {
    (self.0)(LogLevel::Debug, msg);
  }

  pub fn log(&self, msg: &str) {
    (self.0)(LogLevel::Log, msg);
  }

  pub fn warn(&self, msg: &str) {
    (self.0)(LogLevel::Warn, msg);
  }

  pub fn error(&self, msg: &str) {
    (self.0)(LogLevel::Error, msg);
  }
}

pub fn default_logger() -> Logger {
  Logger(Arc::new(|level, msg| match level {
    LogLevel::Debug => log::debug!("{msg}"),
    LogLevel::Log => log::info!("{msg}"),
    LogLevel::Warn => log::warn!("{msg}"),
    LogLevel::Error => log::error!("{msg}"),
  }))
}

/// Logging function type: receives a log level and message string.
pub type LogFn = Box<dyn Fn(LogLevel, &str) + Send + Sync>;
