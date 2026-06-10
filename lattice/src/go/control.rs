// Thin FFI layer for the `srt.devServer` surface: marshals JS calls onto the
// connection supervisor's command channel. The actual connect/discover logic
// lives in connection.rs.

use flux::rquickjs::{function::MutFn, Array, Ctx, Function, Object};
use tokio::sync::mpsc::UnboundedSender;

use super::connection::DevCmd;

/// Install `srt.devServer` with connect/discover/stop methods plus capability
/// and platform hints. `recents` is a snapshot of recently connected addresses
/// (most-recent-first). Augments the existing `srt` global (created by the
/// events plugin), so this must run after `plugins::events::init`.
pub fn install_devserver_control(ctx: Ctx<'_>, cmd_tx: UnboundedSender<DevCmd>, recents: Vec<String>) {
  let srt: Object = ctx.globals().get("srt").expect("srt global must exist before devServer control");
  let dev = Object::new(ctx.clone()).expect("create devServer object");

  let tx = cmd_tx.clone();
  let connect = Function::new(ctx.clone(), MutFn::from(move |addr: String| {
    let _ = tx.send(DevCmd::Connect(addr));
  }))
  .expect("create devServer.connect");

  let tx = cmd_tx.clone();
  let discover = Function::new(ctx.clone(), MutFn::from(move || {
    let _ = tx.send(DevCmd::Discover);
  }))
  .expect("create devServer.discover");

  let stop = Function::new(ctx.clone(), MutFn::from(move || {
    let _ = cmd_tx.send(DevCmd::Stop);
  }))
  .expect("create devServer.stop");

  dev.set("connect", connect).expect("set devServer.connect");
  dev.set("discover", discover).expect("set devServer.discover");
  dev.set("stop", stop).expect("set devServer.stop");

  // Capability hints so the default app shows only the buttons that apply.
  // discover is mDNS (desktop only). scanQr is a pure UI hint: the app scans
  // via the camera module itself and connect()s with the decoded address;
  // phones are where camera pairing makes sense.
  let caps = Object::new(ctx.clone()).expect("create capabilities");
  caps.set("connect", true).expect("set cap.connect");
  caps.set("discover", cfg!(not(target_os = "android"))).expect("set cap.discover");
  caps.set("scanQr", cfg!(target_os = "android")).expect("set cap.scanQr");
  dev.set("capabilities", caps).expect("set devServer.capabilities");
  dev.set("platform", std::env::consts::OS).expect("set devServer.platform");

  let recents_arr = Array::new(ctx.clone()).expect("create recents array");
  for (i, addr) in recents.into_iter().enumerate() {
    recents_arr.set(i, addr).expect("set recent");
  }
  dev.set("recents", recents_arr).expect("set devServer.recents");

  srt.set("devServer", dev).expect("set srt.devServer");
}