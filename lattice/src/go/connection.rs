use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

/// The dialed address of the dev server the proxy should route to. Resettable
/// (not write-once) so reconnecting to a different server repoints the proxy;
/// `None` until the first successful connect.
pub type DevServerCell = Arc<Mutex<Option<String>>>;

/// Atomic flags shared between the dev-server connection (this module, which
/// runs on a tokio worker thread) and the UI thread. Cheap to clone: every
/// field is an `Arc`, so cloning shares the same underlying flags.
#[derive(Clone)]
pub struct DevFlags {
  /// Latched from a `reload` message; read when building the next engine to
  /// decide whether to install the file proxy.
  pub proxy_files_enabled: Arc<AtomicBool>,
  /// Latched from a `reload` message; read when building the next engine to
  /// decide whether to install the fetch proxy.
  pub proxy_http_enabled: Arc<AtomicBool>,
  /// Whether the debug stats overlay is drawn; set from the `welcome`
  /// message's `stats` field.
  pub stats_enabled: Arc<AtomicBool>,
  /// Frame-request latch (see PlatformContext::request_frame); set alongside
  /// `stats_enabled` so a toggle is drawn even when the app is idle.
  pub frame_requested: Arc<AtomicBool>,
  /// Whether the UI thread should forward key events to the dev server; set
  /// from the `welcome` message's `capture` field. The server decides what to
  /// do with forwarded events (see the outbound channel on the caller side and
  /// `dev-server.ts`'s `capture` message handling).
  pub capture_enabled: Arc<AtomicBool>,
  /// True while a dev-server connection is up. Gates senders that would
  /// otherwise queue unboundedly while offline (log forwarding).
  pub connected: Arc<AtomicBool>,
}

/// Send-safe handles the connection answers dev-server queries from, without a
/// round trip through the UI thread: the stats snapshot the draw loop
/// publishes, the live engine's exec handle (refreshed on each engine build),
/// and a sender on the outbound channel for replies produced on the JS thread.
#[derive(Clone)]
pub struct QueryHandles {
  pub stats: Arc<Mutex<crate::overlay::StatsSnapshot>>,
  pub exec: Arc<Mutex<Option<flux::ExecHandle>>>,
  pub outbound_tx: UnboundedSender<String>,
}

#[cfg(not(target_os = "android"))]
const SERVICE_TYPE: &str = "_solidrt._tcp.local.";

/// Commands the JS `srt.dev` surface sends into the supervisor. The
/// connection is opt-in: nothing happens until one of these arrives.
pub enum DevCmd {
  /// Connect to a known `host:port` and keep retrying/reconnecting. Covers the
  /// adb-reverse loopback (`127.0.0.1:DEV_PORT`), manual entry and recents.
  Connect(String),
  /// Connect through the p2p tunnel by ticket: start a loopback forwarder that
  /// dials the ticket per connection, then connect to it like `Connect`.
  ConnectTicket(String),
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
  flags: DevFlags,
  outbound_rx: UnboundedReceiver<String>,
  queries: QueryHandles,
) -> UnboundedSender<DevCmd> {
  let (cmd_tx, cmd_rx) = tokio::sync::mpsc::unbounded_channel::<DevCmd>();
  handle.spawn(supervisor(cmd_rx, engine_tx, state_tx, dev_server, flags, outbound_rx, queries));
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
  flags: DevFlags,
  mut outbound_rx: UnboundedReceiver<String>,
  queries: QueryHandles,
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
        pending =
          run_direct(addr, &mut cmd_rx, &engine_tx, &state_tx, &dev_server, &flags, &mut outbound_rx, &queries).await;
      }
      DevCmd::ConnectTicket(ticket) => {
        pending =
          run_ticket(ticket, &mut cmd_rx, &engine_tx, &state_tx, &dev_server, &flags, &mut outbound_rx, &queries).await;
      }
      DevCmd::Discover => {
        #[cfg(not(target_os = "android"))]
        {
          pending =
            run_discover(&mut cmd_rx, &engine_tx, &state_tx, &dev_server, &flags, &mut outbound_rx, &queries).await;
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
  flags: &DevFlags,
  outbound_rx: &mut UnboundedReceiver<String>,
  queries: &QueryHandles,
) -> Option<DevCmd> {
  loop {
    let _ = state_tx.send(ConnState::Connecting(addr.clone()));
    tokio::select! {
      cmd = cmd_rx.recv() => return cmd,
      _ = try_serve(&addr, engine_tx, state_tx, dev_server, flags, outbound_rx, queries) => {
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

/// Bring up the p2p tunnel forwarder for `ticket`, then connect through its
/// loopback address exactly like a direct connection: the WS handshake rides
/// the tunnel, and a failed ticket dial surfaces as a failed connect that
/// `run_direct` retries. The forwarder is torn down when a new command
/// interrupts (guard drop).
async fn run_ticket(
  ticket: String,
  cmd_rx: &mut UnboundedReceiver<DevCmd>,
  engine_tx: &UnboundedSender<crate::EngineCmd>,
  state_tx: &UnboundedSender<ConnState>,
  dev_server: &DevServerCell,
  flags: &DevFlags,
  outbound_rx: &mut UnboundedReceiver<String>,
  queries: &QueryHandles,
) -> Option<DevCmd> {
  let (addr, _tunnel) = match super::tunnel::start(ticket).await {
    Ok(started) => started,
    Err(e) => {
      log::error!("[sgo] Tunnel start failed: {e}");
      let _ = state_tx.send(ConnState::Idle);
      return cmd_rx.recv().await;
    }
  };
  run_direct(addr.to_string(), cmd_rx, engine_tx, state_tx, dev_server, flags, outbound_rx, queries).await
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
  flags: &DevFlags,
  outbound_rx: &mut UnboundedReceiver<String>,
  queries: &QueryHandles,
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
        c = try_serve(&server, engine_tx, state_tx, dev_server, flags, outbound_rx, queries) => c,
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
  flags: &DevFlags,
  outbound_rx: &mut UnboundedReceiver<String>,
  queries: &QueryHandles,
) -> bool {
  use futures_util::{SinkExt, StreamExt};

  let uri =
    http::Uri::builder().scheme("ws").authority(addr).path_and_query("/").build().expect("invalid dev server URI");

  let (mut client, _) = match tokio_websockets::ClientBuilder::from_uri(uri).connect().await {
    Ok(conn) => conn,
    Err(e) => {
      log::debug!("[sgo] Connect to ws://{addr} failed: {e}");
      return false;
    }
  };

  log::info!("[sgo] Connected to ws://{addr}");
  let _ = state_tx.send(ConnState::Connected(addr.to_string()));
  flags.connected.store(true, Ordering::Relaxed);

  // Publish the dialed dev server address so the next engine build installs the
  // file/dir proxy against the server we are actually talking to. Overwrites any
  // previous address so reconnecting to a different server repoints the proxy.
  *dev_server.lock().expect("dev_server lock poisoned") = Some(addr.to_string());

  let info = serde_json::json!({
    "type": "info",
    "platform": flux::platform(),
    "version": crate::VERSION,
    "capabilities": flux::capabilities(),
  });
  let _ = client.send(tokio_websockets::Message::text(info.to_string())).await;

  loop {
    tokio::select! {
      // Runtime-to-server traffic produced outside this task: captured key
      // events from the UI thread, forwarded console/error lines from the
      // engine logger, and query replies built on the JS thread. Forwarded
      // verbatim; the server decides what to do with each.
      Some(text) = outbound_rx.recv() => {
        let _ = client.send(tokio_websockets::Message::text(text)).await;
        continue;
      }
      msg = client.next() => {
        let Some(Ok(msg)) = msg else { break };
        let Some(text) = msg.as_text() else { continue };
        let Ok(json) = serde_json::from_str::<serde_json::Value>(text) else { continue };
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
              flags.stats_enabled.store(stats, Ordering::Relaxed);
              flags.frame_requested.store(true, Ordering::Relaxed);
            }
            // The dev server's --capture setting: whether the UI thread should
            // forward key events for this session (see capture_rx below).
            if let Some(capture) = json.get("capture").and_then(|c| c.as_bool()) {
              flags.capture_enabled.store(capture, Ordering::Relaxed);
            }
          }
          Some("reload") => {
            let proxy_files = json.get("proxyFiles").and_then(|p| p.as_bool()).unwrap_or(false);
            let proxy_http = json.get("proxyHttp").and_then(|p| p.as_bool()).unwrap_or(false);
            flags.proxy_files_enabled.store(proxy_files, Ordering::Relaxed);
            flags.proxy_http_enabled.store(proxy_http, Ordering::Relaxed);
            if let Some(code) = json.get("code").and_then(|c| c.as_str()) {
              let _ = tx.send(crate::EngineCmd::Reload(code.to_string()));
            }
          }
          Some("stats") => {
            // Live toggle of the debug overlay from the dev-server REPL.
            if let Some(stats) = json.get("stats").and_then(|s| s.as_bool()) {
              flags.stats_enabled.store(stats, Ordering::Relaxed);
              flags.frame_requested.store(true, Ordering::Relaxed);
            }
          }
          Some("stop") => {
            let _ = tx.send(crate::EngineCmd::Stop);
          }
          Some("query") => {
            let id = json.get("id").and_then(|i| i.as_u64()).unwrap_or(0);
            match json.get("kind").and_then(|k| k.as_str()) {
              Some("stats") => {
                let snap = *queries.stats.lock().expect("stats snapshot lock poisoned");
                let _ = client.send(tokio_websockets::Message::text(stats_reply(id, snap))).await;
              }
              Some("tree") => {
                // The render tree lives on the JS thread; snapshot it there and
                // route the reply back through the outbound channel.
                let exec = queries.exec.lock().expect("exec handle lock poisoned").clone();
                match exec {
                  Some(eh) => {
                    let reply_tx = queries.outbound_tx.clone();
                    eh.exec(move |ctx| {
                      let _ = reply_tx.send(tree_reply(&ctx, id));
                    });
                  }
                  None => {
                    let _ = client.send(tokio_websockets::Message::text(error_reply(id, "no running engine"))).await;
                  }
                }
              }
              other => {
                let msg = format!("unknown query kind {other:?}");
                let _ = client.send(tokio_websockets::Message::text(error_reply(id, &msg))).await;
              }
            }
          }
          _ => {}
        }
      }
    }
  }

  flags.connected.store(false, Ordering::Relaxed);
  log::warn!("[sgo] Connection to ws://{addr} lost");
  true
}

/// Round to two decimals for the JSON payloads: raw f32s serialize with float
/// noise (0.1 -> 0.10000000149...) that only bloats the wire format.
fn round2(v: f32) -> f64 {
  (v as f64 * 100.0).round() / 100.0
}

fn error_reply(id: u64, message: &str) -> String {
  serde_json::json!({"type": "result", "id": id, "error": message}).to_string()
}

fn stats_reply(id: u64, s: crate::overlay::StatsSnapshot) -> String {
  serde_json::json!({
    "type": "result",
    "id": id,
    "data": {
      "fps": s.fps,
      "cpuPct": round2(s.cpu_pct),
      "memBytes": s.mem_bytes,
      "jsMs": round2(s.js_ms),
      "frameMs": round2(s.frame_ms),
      "setPropsPerFrame": round2(s.set_count),
      "layoutMs": round2(s.layout_ms),
      "postLayoutMs": round2(s.post_ms),
      "paintMs": round2(s.paint_ms),
      "hoverMs": round2(s.hover_ms),
      "reusedPerSec": s.reused,
      "skippedPerSec": s.skipped,
      "textures": s.textures,
    },
  })
  .to_string()
}

/// Snapshot the render tree from the engine's userdata and encode it. Runs on
/// the JS thread (see the query handling above).
fn tree_reply(ctx: &flux::rquickjs::Ctx<'_>, id: u64) -> String {
  let Some(tree) = ctx.userdata::<flux::gui::tree::SharedRenderTree>() else {
    return error_reply(id, "no render tree");
  };
  let snapshot = tree.0.borrow().snapshot();
  match snapshot {
    Some(root) => serde_json::json!({"type": "result", "id": id, "data": node_json(&root)}).to_string(),
    None => error_reply(id, "no render tree (the app has not rendered)"),
  }
}

fn node_json(node: &alloy::rendertree::NodeSnapshot) -> serde_json::Value {
  let mut obj = serde_json::json!({
    "id": node.id,
    "kind": node.kind,
    "x": round2(node.x),
    "y": round2(node.y),
    "width": round2(node.width),
    "height": round2(node.height),
  });
  let map = obj.as_object_mut().expect("node_json is an object");
  if node.detached {
    map.insert("detached".into(), true.into());
  }
  if let Some(text) = &node.text {
    map.insert("text".into(), text.clone().into());
  }
  if !node.children.is_empty() {
    map.insert("children".into(), node.children.iter().map(node_json).collect::<Vec<_>>().into());
  }
  obj
}
