// Owns all dev-server client state and wiring so the runtime's `ui_thread`
// keeps a single seam to the dev client (DevSession) instead of a scatter of
// `#[cfg(feature = "go")]` blocks. None of the engine/render loop needs to know
// how the dev connection, recents, or proxy are managed; it only starts a
// session, augments each fresh engine builder, and replays state after build.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use flux::rquickjs;
use flux::{ExecHandle, FluxEngineBuilder};
use tokio::sync::mpsc::UnboundedSender;
use tokio::task::LocalSet;

use super::connection::{self, ConnState, DevCmd, DevFlags, DevServerCell};
use super::control::install_dev_control;
use super::proxy::install_proxy_state;

/// Bundles the dev-server connection state held natively across engine rebuilds.
pub struct DevSession {
  // Atomic flags shared with the connection supervisor (proxy enablement,
  // stats overlay); see DevFlags.
  flags: DevFlags,
  // Dialed address of the connected dev server (the proxy base).
  dev_server: DevServerCell,
  // Latest connection state, re-emitted to each newly built engine (the sticky
  // cache itself is per-engine).
  dev_state: Rc<RefCell<ConnState>>,
  // Recently connected addresses, most-recent-first; survive engine rebuilds
  // within a run and are snapshotted into each engine.
  dev_recents: Rc<RefCell<Vec<String>>>,
  // Control channel into the connection supervisor, exposed to JS via the
  // srt.dev plugin.
  dev_cmd_tx: UnboundedSender<DevCmd>,
  // Dev-server address delivered at launch (srt client --android), exposed to JS
  // as srt:dev launchAddress so the launcher can auto-connect. Consumed by a
  // user exit (see DevExitHandle), since the launcher re-dials it on every
  // mount.
  launch_address: Arc<Mutex<Option<String>>>,
}

/// What a user exit does to the dev session (see ExitPolicy in lib.rs): drop
/// the connection and forget the launch address, so the launcher the exit
/// returns to sits idle instead of re-dialing and taking the server's
/// latched push straight back into the app. A device on a dev leash that
/// exits its app is done with the leash too; the address stays one tap away
/// in the launcher.
#[derive(Clone)]
pub struct DevExitHandle {
  cmd_tx: UnboundedSender<DevCmd>,
  launch_address: Arc<Mutex<Option<String>>>,
}

impl DevExitHandle {
  pub fn disconnect(&self) {
    self.launch_address.lock().expect("launch_address lock poisoned").take();
    // A failed send means the supervisor is gone; nothing left to drop.
    let _ = self.cmd_tx.send(DevCmd::Stop);
  }
}

impl DevSession {
  /// Start the connection supervisor and the state-forwarding task. Returns
  /// None in playback mode (`playback_fps` set), which has no dev connection.
  /// `current_exec` is the live engine's exec handle, used to push connection
  /// state into whichever engine is current. `outbound_rx` is the runtime's
  /// text traffic to the server (capture events, log lines, query replies);
  /// `queries` holds what the connection answers server queries from.
  pub fn start(
    handle: &tokio::runtime::Handle,
    engine_cmd_tx: UnboundedSender<crate::EngineCmd>,
    playback_fps: Option<u32>,
    local: &LocalSet,
    current_exec: Rc<RefCell<Option<ExecHandle>>>,
    stats_handles: (Arc<AtomicBool>, Arc<AtomicBool>),
    capture_enabled: Arc<AtomicBool>,
    connected: Arc<AtomicBool>,
    clock: crate::runtime::ClockControl,
    input_tx: UnboundedSender<alloy::AlloyEvent>,
    resampler: alloy::resample::SharedResampler,
    user_input_muted: Arc<AtomicBool>,
    outbound_rx: tokio::sync::mpsc::UnboundedReceiver<String>,
    queries: connection::QueryHandles,
    launch_address: Option<String>,
  ) -> Option<DevSession> {
    if playback_fps.is_some() {
      return None;
    }

    let (stats_enabled, frame_requested) = stats_handles;
    let flags = DevFlags {
      proxy_http_enabled: Arc::new(AtomicBool::new(false)),
      stats_enabled,
      frame_requested,
      capture_enabled,
      connected,
      clock,
      input_tx,
      resampler,
      user_input_muted,
    };
    let dev_server: DevServerCell = Arc::new(std::sync::Mutex::new(None));
    let dev_state = Rc::new(RefCell::new(ConnState::Idle));
    let dev_recents: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(super::config::load().recents));

    let (state_tx, mut state_rx) = tokio::sync::mpsc::unbounded_channel::<ConnState>();
    let dev_cmd_tx =
      connection::start(handle, engine_cmd_tx, state_tx, dev_server.clone(), flags.clone(), outbound_rx, queries);

    // Forward connection-state changes to JS as the sticky `dev` event,
    // targeting whichever engine is currently live, and keep the held copy in
    // sync so a later engine rebuild can replay it.
    let dev_state_task = dev_state.clone();
    let dev_recents_task = dev_recents.clone();
    local.spawn_local(async move {
      while let Some(st) = state_rx.recv().await {
        if let ConnState::Connected { addr, recent } = &st {
          if add_recent(&dev_recents_task, addr, recent.as_deref()) {
            super::config::save_recents(&dev_recents_task.borrow());
          }
        }
        *dev_state_task.borrow_mut() = st.clone();
        if let Some(eh) = current_exec.borrow().as_ref() {
          emit_dev_state(eh, st, dev_recents_task.borrow().clone());
        }
      }
    });

    Some(DevSession {
      flags,
      dev_server,
      dev_state,
      dev_recents,
      dev_cmd_tx,
      launch_address: Arc::new(Mutex::new(launch_address)),
    })
  }

  /// Install the dev-server control surface and, when the server has requested
  /// it, the http proxy onto a freshly created engine builder.
  pub fn augment_builder(&self, mut builder: FluxEngineBuilder) -> FluxEngineBuilder {
    if self.flags.proxy_http_enabled.load(Ordering::Relaxed) {
      if let Some(url) = self.dev_server.lock().expect("dev_server lock poisoned").clone() {
        builder = builder.plugin(move |ctx| install_proxy_state(ctx, url));
      }
    }
    let dev_cmd_tx = self.dev_cmd_tx.clone();
    let recents = self.dev_recents.borrow().clone();
    let launch_address = self.launch_address.lock().expect("launch_address lock poisoned").clone();
    builder.plugin(move |ctx| install_dev_control(ctx, dev_cmd_tx, recents, launch_address))
  }

  pub fn exit_handle(&self) -> DevExitHandle {
    DevExitHandle { cmd_tx: self.dev_cmd_tx.clone(), launch_address: self.launch_address.clone() }
  }

  /// Replay the latest connection state into a freshly built engine so a reload
  /// (e.g. a server stop returning to the launcher) keeps the right indicator.
  pub fn replay_state(&self, exec: &ExecHandle) {
    emit_dev_state(exec, self.dev_state.borrow().clone(), self.dev_recents.borrow().clone());
  }
}

// Record a reconnectable connection as the most-recent entry. `recent`, when
// present, is the identifier to remember (a ticket, for tunnel connections);
// otherwise the connected `addr` is used, and loopback / tunnel addresses are
// skipped since they aren't reconnectable on their own. Returns true if the
// list changed (so the caller can persist it), false for a skipped address.
fn add_recent(recents: &Rc<RefCell<Vec<String>>>, addr: &str, recent: Option<&str>) -> bool {
  let key = match recent {
    Some(t) => t,
    None => {
      if addr.starts_with("127.") || addr.starts_with("localhost") || addr.starts_with("[::1]") {
        return false;
      }
      addr
    }
  };
  let mut r = recents.borrow_mut();
  r.retain(|a| a != key);
  r.insert(0, key.to_string());
  r.truncate(8);
  true
}

// Emit the dev-server connection state to JS as the sticky `dev` event.
// Sticky so it replays to the launcher's subscriber on each engine rebuild,
// which keeps the "connected" indicator across a server stop (the stop reloads
// the launcher but leaves the websocket up).
fn emit_dev_state(eh: &ExecHandle, st: ConnState, recents: Vec<String>) {
  eh.exec(move |ctx| {
    let (state, addr, tunneled) = st.parts();
    let obj = rquickjs::Object::new(ctx.clone()).expect("create dev event object");
    obj.set("state", state).expect("set state");
    match addr {
      Some(a) => obj.set("address", a).expect("set address"),
      None => obj.set("address", rquickjs::Null).expect("set address null"),
    }
    obj.set("tunneled", tunneled).expect("set tunneled");
    let arr = rquickjs::Array::new(ctx.clone()).expect("create recents array");
    for (i, a) in recents.into_iter().enumerate() {
      arr.set(i, a).expect("set recent");
    }
    obj.set("recents", arr).expect("set recents");
    flux::emit_sticky(&ctx, "dev", obj);
  });
}
