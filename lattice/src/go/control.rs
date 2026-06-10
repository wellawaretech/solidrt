// Thin FFI layer for the `srt.dev` surface: marshals JS calls onto the
// connection supervisor's command channel. The actual connect/discover logic
// lives in connection.rs.

use flux::rquickjs::{function::MutFn, Array, Ctx, Function, Object};
use tokio::sync::mpsc::UnboundedSender;

use super::connection::DevCmd;

/// Install `srt.dev` with connect/discover/stop methods. `recents` is a
/// snapshot of recently connected addresses (most-recent-first). Augments the
/// existing `srt` global (created by the events plugin), so this must run
/// after `plugins::events::init`.
pub fn install_dev_control(ctx: Ctx<'_>, cmd_tx: UnboundedSender<DevCmd>, recents: Vec<String>) {
  let srt: Object = ctx.globals().get("srt").expect("srt global must exist before dev control");
  let dev = Object::new(ctx.clone()).expect("create dev object");

  let tx = cmd_tx.clone();
  let connect = Function::new(ctx.clone(), MutFn::from(move |addr: String| {
    let _ = tx.send(DevCmd::Connect(addr));
  }))
  .expect("create dev.connect");

  let tx = cmd_tx.clone();
  let discover = Function::new(ctx.clone(), MutFn::from(move || {
    let _ = tx.send(DevCmd::Discover);
  }))
  .expect("create dev.discover");

  let stop = Function::new(ctx.clone(), MutFn::from(move || {
    let _ = cmd_tx.send(DevCmd::Stop);
  }))
  .expect("create dev.stop");

  dev.set("connect", connect).expect("set dev.connect");
  dev.set("discover", discover).expect("set dev.discover");
  dev.set("stop", stop).expect("set dev.stop");

  // discover is mDNS (desktop only); the flag lets the default app show only
  // the buttons that apply.
  dev.set("canDiscover", cfg!(not(target_os = "android"))).expect("set dev.canDiscover");

  let recents_arr = Array::new(ctx.clone()).expect("create recents array");
  for (i, addr) in recents.into_iter().enumerate() {
    recents_arr.set(i, addr).expect("set recent");
  }
  dev.set("recents", recents_arr).expect("set dev.recents");

  srt.set("dev", dev).expect("set srt.dev");
}