// Owns all dev-server client state and wiring so the runtime's `ui_thread`
// keeps a single seam to the dev client (DevSession) instead of a scatter of
// `#[cfg(feature = "go")]` blocks. None of the engine/render loop needs to know
// how the dev connection, recents, or proxy are managed; it only starts a
// session, augments each fresh engine builder, and replays state after build.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use flux::rquickjs;
use flux::{ExecHandle, FluxEngineBuilder};
use tokio::sync::mpsc::UnboundedSender;
use tokio::task::LocalSet;

use super::connection::{self, ConnState, DevCmd, DevServerCell};
use super::control::install_devserver_control;
use super::proxy::{install_proxy_state, ProxyFsModule};

/// Bundles the dev-server connection state held natively across engine rebuilds.
pub struct DevSession {
  // Latched by the connection supervisor from each server `reload` message; read
  // when building the next engine to decide whether to install the proxy.
  proxy_files_enabled: Arc<AtomicBool>,
  proxy_http_enabled: Arc<AtomicBool>,
  // Dialed address of the connected dev server (the proxy base).
  dev_server: DevServerCell,
  // Latest connection state, re-emitted to each newly built engine (the sticky
  // cache itself is per-engine).
  dev_state: Rc<RefCell<ConnState>>,
  // Recently connected addresses, most-recent-first; survive engine rebuilds
  // within a run and are snapshotted into each engine.
  dev_recents: Rc<RefCell<Vec<String>>>,
  // Control channel into the connection supervisor, exposed to JS via the
  // srt.devServer plugin.
  dev_cmd_tx: UnboundedSender<DevCmd>,
}

impl DevSession {
  /// Start the connection supervisor and the state-forwarding task. Returns
  /// None in record mode (`record_fps` set), which has no dev connection.
  /// `current_exec` is the live engine's exec handle, used to push connection
  /// state into whichever engine is current.
  pub fn start(
    handle: &tokio::runtime::Handle,
    engine_cmd_tx: UnboundedSender<crate::EngineCmd>,
    record_fps: Option<u32>,
    local: &LocalSet,
    current_exec: Rc<RefCell<Option<ExecHandle>>>,
  ) -> Option<DevSession> {
    if record_fps.is_some() {
      return None;
    }

    let proxy_files_enabled = Arc::new(AtomicBool::new(false));
    let proxy_http_enabled = Arc::new(AtomicBool::new(false));
    let dev_server: DevServerCell = Arc::new(std::sync::Mutex::new(None));
    let dev_state = Rc::new(RefCell::new(ConnState::Idle));
    let dev_recents: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));

    let (state_tx, mut state_rx) = tokio::sync::mpsc::unbounded_channel::<ConnState>();
    let dev_cmd_tx = connection::start(
      handle,
      engine_cmd_tx,
      state_tx,
      dev_server.clone(),
      proxy_files_enabled.clone(),
      proxy_http_enabled.clone(),
    );

    // Forward connection-state changes to JS as the sticky `devServer` event,
    // targeting whichever engine is currently live, and keep the held copy in
    // sync so a later engine rebuild can replay it.
    let dev_state_task = dev_state.clone();
    let dev_recents_task = dev_recents.clone();
    local.spawn_local(async move {
      while let Some(st) = state_rx.recv().await {
        if let ConnState::Connected(addr) = &st {
          add_recent(&dev_recents_task, addr);
        }
        *dev_state_task.borrow_mut() = st.clone();
        if let Some(eh) = current_exec.borrow().as_ref() {
          emit_dev_state(eh, st, dev_recents_task.borrow().clone());
        }
      }
    });

    Some(DevSession { proxy_files_enabled, proxy_http_enabled, dev_server, dev_state, dev_recents, dev_cmd_tx })
  }

  /// Install the dev-server control surface and, when the server has requested
  /// it, the file/http proxy onto a freshly created engine builder.
  pub fn augment_builder(&self, mut builder: FluxEngineBuilder) -> FluxEngineBuilder {
    let proxy_files = self.proxy_files_enabled.load(Ordering::Relaxed);
    let proxy_http = self.proxy_http_enabled.load(Ordering::Relaxed);
    if proxy_files || proxy_http {
      if let Some(url) = self.dev_server.lock().expect("dev_server lock poisoned").clone() {
        if proxy_files {
          builder = builder.module_override("flux:fs", ProxyFsModule);
        }
        builder = builder.plugin(move |ctx| install_proxy_state(ctx, url, proxy_http));
      }
    }
    let dev_cmd_tx = self.dev_cmd_tx.clone();
    let recents = self.dev_recents.borrow().clone();
    builder.plugin(move |ctx| install_devserver_control(ctx, dev_cmd_tx, recents))
  }

  /// Replay the latest connection state into a freshly built engine so a reload
  /// (e.g. a server stop returning to the default app) keeps the right indicator.
  pub fn replay_state(&self, exec: &ExecHandle) {
    emit_dev_state(exec, self.dev_state.borrow().clone(), self.dev_recents.borrow().clone());
  }
}

// Record a successfully connected address as the most-recent entry. Loopback /
// tunnel addresses are skipped since they aren't reconnectable on their own.
// In-memory only for now (lost on process exit); disk persistence is TODO.
fn add_recent(recents: &Rc<RefCell<Vec<String>>>, addr: &str) {
  if addr.starts_with("127.") || addr.starts_with("localhost") || addr.starts_with("[::1]") {
    return;
  }
  let mut r = recents.borrow_mut();
  r.retain(|a| a != addr);
  r.insert(0, addr.to_string());
  r.truncate(8);
}

// Emit the dev-server connection state to JS as the sticky `devServer` event.
// Sticky so it replays to the default app's subscriber on each engine rebuild,
// which keeps the "connected" indicator across a server stop (the stop reloads
// the default app but leaves the websocket up).
fn emit_dev_state(eh: &ExecHandle, st: ConnState, recents: Vec<String>) {
  eh.exec(move |ctx| {
    let (state, addr) = st.parts();
    let obj = rquickjs::Object::new(ctx.clone()).expect("create devServer object");
    obj.set("state", state).expect("set state");
    match addr {
      Some(a) => obj.set("address", a).expect("set address"),
      None => obj.set("address", rquickjs::Null).expect("set address null"),
    }
    let arr = rquickjs::Array::new(ctx.clone()).expect("create recents array");
    for (i, a) in recents.into_iter().enumerate() {
      arr.set(i, a).expect("set recent");
    }
    obj.set("recents", arr).expect("set recents");
    crate::plugins::events::emit_sticky(&ctx, "devServer", obj);
  });
}
