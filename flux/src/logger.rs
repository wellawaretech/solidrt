use std::sync::Arc;
use rquickjs::JsLifetime;

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
pub(crate) struct Logger(#[qjs(skip_trace)] pub(crate) Arc<dyn Fn(LogLevel, &str) + Send + Sync>);

impl Logger {
    #[allow(dead_code)]
    pub(crate) fn debug(&self, msg: &str) {
        (self.0)(LogLevel::Debug, msg);
    }

    pub(crate) fn log(&self, msg: &str) {
        (self.0)(LogLevel::Log, msg);
    }

    pub(crate) fn warn(&self, msg: &str) {
        (self.0)(LogLevel::Warn, msg);
    }

    pub(crate) fn error(&self, msg: &str) {
        (self.0)(LogLevel::Error, msg);
    }
}

pub(crate) fn default_logger() -> Logger {
    Logger(Arc::new(|level, msg| match level {
        LogLevel::Debug | LogLevel::Log => println!("{msg}"),
        LogLevel::Warn | LogLevel::Error => eprintln!("{msg}"),
    }))
}

/// Logging function type: receives a log level and message string.
pub type LogFn = Box<dyn Fn(LogLevel, &str) + Send + Sync>;
