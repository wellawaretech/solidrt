use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

/// The dialed address of the dev server the proxy should route to. Resettable
/// (not write-once) so reconnecting to a different server repoints the proxy;
/// `None` until the first successful connect.
pub type DevServerCell = Arc<Mutex<Option<String>>>;

#[cfg(not(target_os = "android"))]
const SERVICE_TYPE: &str = "_solidrt._tcp.local.";

/// Commands the JS `srt.dev` surface sends into the supervisor. The
/// connection is opt-in: nothing happens until one of these arrives.
pub enum DevCmd {
  /// Connect to a known `host:port` and keep retrying/reconnecting. Covers the
  /// adb-reverse loopback (`127.0.0.1:DEV_PORT`), manual entry and recents.
  Connect(String),
  /// Browse the LAN for a dev server via mDNS, then connect.
  Discover,
  /// Stop searching and drop any connection, back to idle.
  Stop,
}

/// Connection state reported back to JS as the sticky `dev` event.
#[derive(Clone)]
pub enum ConnState {
  Idle,
  // Searching is mDNS (desktop only); the variant + JS state exist everywhere
  // so the event payload mapping stays uniform. (QR scanning is no longer a
  // supervisor state: the app scans via the camera module and sends a plain
  // Connect with the decoded address.)
  #[cfg_attr(target_os = "android", allow(dead_code))]
  Searching,
  Connecting(String),
  Connected(String),
}

impl ConnState {
  /// (state string, optional address) for the JS event payload.
  pub fn parts(&self) -> (&'static str, Option<&str>) {
    match self {
      ConnState::Idle => ("idle", None),
      ConnState::Searching => ("searching", None),
      ConnState::Connecting(addr) => ("connecting", Some(addr)),
      ConnState::Connected(addr) => ("connected", Some(addr)),
    }
  }
}

/// Spawn the dev-server connection supervisor. It parks until JS sends a
/// `DevCmd` (via the returned sender), so an idle app never browses or connects.
pub fn start(
  handle: &tokio::runtime::Handle,
  engine_tx: UnboundedSender<crate::EngineCmd>,
  state_tx: UnboundedSender<ConnState>,
  dev_server: DevServerCell,
  proxy_files_enabled: Arc<AtomicBool>,
  proxy_http_enabled: Arc<AtomicBool>,
  stats_handles: (Arc<AtomicBool>, Arc<AtomicBool>),
) -> UnboundedSender<DevCmd> {
  let (cmd_tx, cmd_rx) = tokio::sync::mpsc::unbounded_channel::<DevCmd>();
  handle.spawn(supervisor(cmd_rx, engine_tx, state_tx, dev_server, proxy_files_enabled, proxy_http_enabled, stats_handles));
  cmd_tx
}

/// Drives one mechanism at a time. Each `run_*` returns the command that
/// interrupted it (so we switch mechanism), or `None` when the command channel
/// closes (so we exit).
async fn supervisor(
  mut cmd_rx: UnboundedReceiver<DevCmd>,
  engine_tx: UnboundedSender<crate::EngineCmd>,
  state_tx: UnboundedSender<ConnState>,
  dev_server: DevServerCell,
  proxy_files_enabled: Arc<AtomicBool>,
  proxy_http_enabled: Arc<AtomicBool>,
  stats_handles: (Arc<AtomicBool>, Arc<AtomicBool>),
) {
  let mut pending: Option<DevCmd> = None;
  loop {
    let cmd = match pending.take() {
      Some(c) => c,
      None => match cmd_rx.recv().await {
        Some(c) => c,
        None => return,
      },
    };
    match cmd {
      DevCmd::Stop => {
        let _ = state_tx.send(ConnState::Idle);
      }
      DevCmd::Connect(addr) => {
        pending = run_direct(
          addr,
          &mut cmd_rx,
          &engine_tx,
          &state_tx,
          &dev_server,
          &proxy_files_enabled,
          &proxy_http_enabled,
          &stats_handles,
        )
        .await;
      }
      DevCmd::Discover => {
        #[cfg(not(target_os = "android"))]
        {
          pending = run_discover(
            &mut cmd_rx,
            &engine_tx,
            &state_tx,
            &dev_server,
            &proxy_files_enabled,
            &proxy_http_enabled,
            &stats_handles,
          )
          .await;
        }
        #[cfg(target_os = "android")]
        {
          log::warn!("[sgo] discover() is not supported on this platform");
          let _ = state_tx.send(ConnState::Idle);
        }
      }
    }
  }
}

/// Connect to a fixed address, retrying until reachable and reconnecting after
/// drops. Returns when a new command interrupts (or the channel closes).
async fn run_direct(
  addr: String,
  cmd_rx: &mut UnboundedReceiver<DevCmd>,
  engine_tx: &UnboundedSender<crate::EngineCmd>,
  state_tx: &UnboundedSender<ConnState>,
  dev_server: &DevServerCell,
  proxy_files_enabled: &Arc<AtomicBool>,
  proxy_http_enabled: &Arc<AtomicBool>,
  stats_handles: &(Arc<AtomicBool>, Arc<AtomicBool>),
) -> Option<DevCmd> {
  loop {
    let _ = state_tx.send(ConnState::Connecting(addr.clone()));
    tokio::select! {
      cmd = cmd_rx.recv() => return cmd,
      _ = try_serve(&addr, engine_tx, state_tx, dev_server, proxy_files_enabled, proxy_http_enabled, stats_handles) => {
        // Failed to connect or the connection dropped; pause before retrying,
        // but let a new command interrupt the wait.
        tokio::select! {
          cmd = cmd_rx.recv() => return cmd,
          _ = tokio::time::sleep(Duration::from_secs(3)) => {}
        }
      }
    }
  }
}

/// Browse for the dev server via mDNS, then connect and serve. Once a server is
/// resolved we keep reconnecting to that address (so dev-server restarts on the
/// same host reconnect without waiting for a fresh announcement); only after
/// repeated connect failures do we go back to browsing, in case it moved. The
/// mDNS daemon is dropped when this returns, so an interrupting command (e.g.
/// Stop) actually stops browsing.
#[cfg(not(target_os = "android"))]
async fn run_discover(
  cmd_rx: &mut UnboundedReceiver<DevCmd>,
  engine_tx: &UnboundedSender<crate::EngineCmd>,
  state_tx: &UnboundedSender<ConnState>,
  dev_server: &DevServerCell,
  proxy_files_enabled: &Arc<AtomicBool>,
  proxy_http_enabled: &Arc<AtomicBool>,
  stats_handles: &(Arc<AtomicBool>, Arc<AtomicBool>),
) -> Option<DevCmd> {
  use mdns_sd::ServiceDaemon;

  let _ = state_tx.send(ConnState::Searching);

  let mdns = match ServiceDaemon::new() {
    Ok(d) => d,
    Err(e) => {
      log::error!("[sgo] mDNS init failed: {e}");
      let _ = state_tx.send(ConnState::Idle);
      return cmd_rx.recv().await;
    }
  };
  let receiver = match mdns.browse(SERVICE_TYPE) {
    Ok(r) => r,
    Err(e) => {
      log::error!("[sgo] mDNS browse failed: {e}");
      let _ = state_tx.send(ConnState::Idle);
      return cmd_rx.recv().await;
    }
  };
  log::info!("[sgo] Browsing for {SERVICE_TYPE} via mDNS...");

  const MAX_FAILURES: u32 = 5;
  let mut addr: Option<String> = None;
  let mut failures = 0u32;

  loop {
    // Block for the next resolved service whenever we have no address to try.
    if addr.is_none() {
      let _ = state_tx.send(ConnState::Searching);
      tokio::select! {
        cmd = cmd_rx.recv() => return cmd,
        resolved = recv_resolved(&receiver) => {
          match resolved {
            Some(a) => { addr = Some(a); failures = 0; }
            // Receiver closed; go idle and wait for the next command.
            None => { let _ = state_tx.send(ConnState::Idle); return cmd_rx.recv().await; }
          }
        }
      }
    }

    if let Some(server) = addr.clone() {
      let _ = state_tx.send(ConnState::Connecting(server.clone()));
      let connected = tokio::select! {
        cmd = cmd_rx.recv() => return cmd,
        c = try_serve(&server, engine_tx, state_tx, dev_server, proxy_files_enabled, proxy_http_enabled, stats_handles) => c,
      };
      if connected {
        // Was connected, then dropped: retry the same address.
        failures = 0;
      } else {
        failures += 1;
        if failures >= MAX_FAILURES {
          log::info!("[sgo] {server} unreachable; re-discovering");
          addr = None;
          continue;
        }
      }
      tokio::select! {
        cmd = cmd_rx.recv() => return cmd,
        _ = tokio::time::sleep(Duration::from_secs(3)) => {}
      }
    }
  }
}

/// Wait for the next resolved service and return its `host:port`. Returns None
/// only when the mDNS receiver is closed.
#[cfg(not(target_os = "android"))]
async fn recv_resolved(receiver: &mdns_sd::Receiver<mdns_sd::ServiceEvent>) -> Option<String> {
  use mdns_sd::ServiceEvent;

  loop {
    match receiver.recv_async().await {
      Ok(ServiceEvent::ServiceResolved(info)) => {
        if let Some(addr) = service_addr(&info) {
          log::info!("[sgo] Discovered dev server at {addr}");
          return Some(addr);
        }
      }
      Ok(_) => {}
      Err(e) => {
        log::warn!("[sgo] mDNS receiver closed: {e}");
        return None;
      }
    }
  }
}

/// Pick a connectable `host:port` from a resolved service, preferring IPv4.
#[cfg(not(target_os = "android"))]
fn service_addr(info: &mdns_sd::ResolvedService) -> Option<String> {
  let port = info.get_port();
  // Prefer a routable IPv4 address.
  if let Some(v4) = info.get_addresses_v4().into_iter().find(|a| !a.is_loopback()) {
    return Some(format!("{v4}:{port}"));
  }
  // Otherwise take the first non-loopback address (bracket IPv6 for the URI).
  let ip = info.get_addresses().iter().find(|a| !a.is_loopback())?.to_ip_addr();
  match ip {
    std::net::IpAddr::V4(v4) => Some(format!("{v4}:{port}")),
    std::net::IpAddr::V6(v6) => Some(format!("[{v6}]:{port}")),
  }
}

/// Connect to a dev server at `addr` and serve until the connection drops.
/// Returns true if the connection was established (and has since been lost),
/// false if the initial connect failed (so the caller can try the next path).
async fn try_serve(
  addr: &str,
  tx: &UnboundedSender<crate::EngineCmd>,
  state_tx: &UnboundedSender<ConnState>,
  dev_server: &DevServerCell,
  proxy_files_enabled: &Arc<AtomicBool>,
  proxy_http_enabled: &Arc<AtomicBool>,
  stats_handles: &(Arc<AtomicBool>, Arc<AtomicBool>),
) -> bool {
  use futures_util::{SinkExt, StreamExt};

  let uri = http::Uri::builder()
    .scheme("ws")
    .authority(addr)
    .path_and_query("/")
    .build()
    .expect("invalid dev server URI");

  let (mut client, _) = match tokio_websockets::ClientBuilder::from_uri(uri).connect().await {
    Ok(conn) => conn,
    Err(e) => {
      log::debug!("[sgo] Connect to ws://{addr} failed: {e}");
      return false;
    }
  };

  log::info!("[sgo] Connected to ws://{addr}");
  let _ = state_tx.send(ConnState::Connected(addr.to_string()));

  // Publish the dialed dev server address so the next engine build installs the
  // file/dir proxy against the server we are actually talking to. Overwrites any
  // previous address so reconnecting to a different server repoints the proxy.
  *dev_server.lock().expect("dev_server lock poisoned") = Some(addr.to_string());

  let info =
    format!(r#"{{"type":"info","platform":"{}","version":"{}"}}"#, flux::platform(), crate::VERSION);
  let _ = client.send(tokio_websockets::Message::text(info)).await;

  while let Some(Ok(msg)) = client.next().await {
    if let Some(text) = msg.as_text() {
      if let Ok(json) = serde_json::from_str::<serde_json::Value>(text) {
        match json.get("type").and_then(|t| t.as_str()) {
          Some("welcome") => {
            // The server's self-reported LAN address. Report it as the connected
            // address (for display/recents) even though the socket may be the adb
            // loopback tunnel; the dev_server proxy base stays on the dialed addr.
            if let Some(server_addr) = json.get("address").and_then(|a| a.as_str()) {
              if !server_addr.is_empty() && server_addr != addr {
                let _ = state_tx.send(ConnState::Connected(server_addr.to_string()));
              }
            }
            // The dev server's --stats setting for this session, applied to the
            // overlay live (no launch-arg plumbing needed on either platform).
            if let Some(stats) = json.get("stats").and_then(|s| s.as_bool()) {
              let (stats_enabled, frame_requested) = stats_handles;
              stats_enabled.store(stats, Ordering::Relaxed);
              frame_requested.store(true, Ordering::Relaxed);
            }
          }
          Some("reload") => {
            let proxy_files = json.get("proxyFiles").and_then(|p| p.as_bool()).unwrap_or(false);
            let proxy_http = json.get("proxyHttp").and_then(|p| p.as_bool()).unwrap_or(false);
            proxy_files_enabled.store(proxy_files, Ordering::Relaxed);
            proxy_http_enabled.store(proxy_http, Ordering::Relaxed);
            if let Some(code) = json.get("code").and_then(|c| c.as_str()) {
              let _ = tx.send(crate::EngineCmd::Reload(code.to_string()));
            }
          }
          Some("stop") => {
            let _ = tx.send(crate::EngineCmd::Stop);
          }
          _ => {}
        }
      }
    }
  }

  log::warn!("[sgo] Connection to ws://{addr} lost");
  true
}