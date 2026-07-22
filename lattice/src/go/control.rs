// Thin FFI layer for the `srt:dev` and `srt:apps` modules: marshals JS calls
// onto the connection supervisor's command channel and the version store. The
// actual logic lives in connection.rs and store.rs; the JS-facing module
// shapes live in plugins::dev and plugins::apps.

use flux::rquickjs::Ctx;
use tokio::sync::mpsc::UnboundedSender;

use super::connection::DevCmd;
use crate::plugins::apps::{self, AppEntry, AppsControl, AppsControlInner};
use crate::plugins::dev::{self, DevControl, DevControlInner};

/// Install the dev control as context userdata, backing the `srt:dev` module
/// with connect/discover/stop that forward onto `cmd_tx`. `recents` is a
/// snapshot of recently connected addresses (most-recent-first).
pub fn install_dev_control(
  ctx: Ctx<'_>,
  cmd_tx: UnboundedSender<DevCmd>,
  recents: Vec<String>,
  launch_address: Option<String>,
) {
  let connect_tx = cmd_tx.clone();
  let discover_tx = cmd_tx.clone();
  let stop_tx = cmd_tx;

  let control = DevControl::new(DevControlInner {
    connect: Box::new(move |addr| {
      // A ticket (`id|relay|ips`) connects through the p2p tunnel; a plain
      // `host:port` dials the dev server directly. Covers every caller,
      // including the QR scan path, which connects with the decoded payload.
      let cmd = if addr.contains('|') { DevCmd::ConnectTicket(addr) } else { DevCmd::Connect(addr) };
      let _ = connect_tx.send(cmd);
    }),
    discover: Box::new(move || {
      let _ = discover_tx.send(DevCmd::Discover);
    }),
    stop: Box::new(move || {
      let _ = stop_tx.send(DevCmd::Stop);
    }),
    // discover is mDNS (desktop only); the flag lets the launcher show only
    // the buttons that apply.
    can_discover: cfg!(not(target_os = "android")),
    recents,
    launch_address,
  });

  dev::install(&ctx, control);
}

/// Install the apps control as context userdata, backing the `srt:apps` module
/// with the version store's list/launch/remove. Launch boots the stored
/// version through the same reload path as a dev push: the engine loop
/// re-anchors the data sandbox and assets mount from the app id. Fonts are the
/// known gap: registration happens once at startup, so a launched app's custom
/// fonts apply from the next client start only (the dev-push install path has
/// the same limitation).
pub fn install_apps_control(ctx: Ctx<'_>, engine_tx: UnboundedSender<crate::EngineCmd>) {
  let control = AppsControl::new(AppsControlInner {
    list: Box::new(|| {
      super::store::list_installed()
        .into_iter()
        .map(|app| AppEntry { id: app.id, name: app.name, version: app.version })
        .collect()
    }),
    launch: Box::new(move |id| {
      let boot = super::store::load(&id).ok_or_else(|| format!("app {id} is not installed"))?;
      engine_tx
        .send(crate::EngineCmd::Reload { code: boot.code, app_id: Some(boot.app_id) })
        .map_err(|_| "engine is shutting down".to_string())
    }),
    remove: Box::new(|id| super::store::remove_app(&id)),
  });

  apps::install(&ctx, control);
}
