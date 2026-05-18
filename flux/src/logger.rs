use rquickjs::{Ctx, JsLifetime};
use std::sync::Arc;

/// Log level passed to the logger callback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
  Debug,
  Log,
  Warn,
  Error,
}

/// Shared log sink, stored as userdata in the JS context.
#[derive(Clone, JsLifetime)]
pub struct Logger(#[qjs(skip_trace)] pub(crate) Arc<dyn Fn(LogLevel, &str) + Send + Sync>);

impl Logger {
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

pub trait CtxLogger {
  fn logger(&self) -> Logger;
}

impl CtxLogger for Ctx<'_> {
  fn logger(&self) -> Logger {
    self.userdata::<Logger>().unwrap().clone()
  }
}

/// Logging function type: receives a log level and message string.
pub type LogFn = Box<dyn Fn(LogLevel, &str) + Send + Sync>;
