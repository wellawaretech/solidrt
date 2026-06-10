// Thin FFI layer for the `srt:dev` module: marshals JS calls onto the
// connection supervisor's command channel. The actual connect/discover logic
// lives in connection.rs; the JS-facing module shape lives in plugins::dev.

use flux::rquickjs::Ctx;
use tokio::sync::mpsc::UnboundedSender;

use super::connection::DevCmd;
use crate::plugins::dev::{self, DevControl, DevControlInner};

/// Install the dev control as context userdata, backing the `srt:dev` module
/// with connect/discover/stop that forward onto `cmd_tx`. `recents` is a
/// snapshot of recently connected addresses (most-recent-first).
pub fn install_dev_control(ctx: Ctx<'_>, cmd_tx: UnboundedSender<DevCmd>, recents: Vec<String>) {
  let connect_tx = cmd_tx.clone();
  let discover_tx = cmd_tx.clone();
  let stop_tx = cmd_tx;

  let control = DevControl::new(DevControlInner {
    connect: Box::new(move |addr| {
      let _ = connect_tx.send(DevCmd::Connect(addr));
    }),
    discover: Box::new(move || {
      let _ = discover_tx.send(DevCmd::Discover);
    }),
    stop: Box::new(move || {
      let _ = stop_tx.send(DevCmd::Stop);
    }),
    // discover is mDNS (desktop only); the flag lets the default app show only
    // the buttons that apply.
    can_discover: cfg!(not(target_os = "android")),
    recents,
  });

  dev::install(&ctx, control);
}
