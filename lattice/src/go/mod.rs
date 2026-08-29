mod config;
mod connection;
pub(crate) mod control;
pub(crate) mod icon;
mod proxy;
mod session;
pub(crate) mod store;
pub(crate) mod tunnel;

pub use connection::QueryHandles;
pub use session::{DevExitHandle, DevSession};
#[cfg(test)]
pub(crate) use connection::parse_input_events;

/// Engine logger for the go client: writes to the local log as before and,
/// while a dev server is connected, also forwards each line over the outbound
/// channel (as a `log` message) so server-side tooling can read app output.
/// The `connected` gate keeps an offline app from queueing lines unboundedly.
pub fn dev_logger(
  outbound_tx: tokio::sync::mpsc::UnboundedSender<String>,
  connected: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> impl Fn(flux::LogLevel, &str) + Send + Sync + 'static {
  move |level, msg| {
    match level {
      flux::LogLevel::Debug => log::debug!("{msg}"),
      flux::LogLevel::Log => log::info!("{msg}"),
      flux::LogLevel::Warn => log::warn!("{msg}"),
      flux::LogLevel::Error => log::error!("{msg}"),
    }
    if connected.load(std::sync::atomic::Ordering::Relaxed) {
      let level = match level {
        flux::LogLevel::Debug => "debug",
        flux::LogLevel::Log => "log",
        flux::LogLevel::Warn => "warn",
        flux::LogLevel::Error => "error",
      };
      let _ = outbound_tx.send(serde_json::json!({"type": "log", "level": level, "text": msg}).to_string());
    }
  }
}
