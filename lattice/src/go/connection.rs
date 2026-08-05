use base64::Engine as _;
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
  /// `addr` is the address to display (the server's self-reported LAN address
  /// once known). `recent` is the reconnectable identifier to remember, when it
  /// differs from `addr`: a ticket connection displays the LAN address but must
  /// remember the ticket (the loopback socket it dials is not reconnectable on
  /// its own). `None` for direct connections, which remember `addr`.
  Connected {
    addr: String,
    recent: Option<String>,
  },
}

impl ConnState {
  /// (state string, optional address, tunneled) for the JS event payload.
  /// `tunneled` is true when this connection was made by ticket (p2p, e2e
  /// encrypted) rather than a direct `host:port` dial.
  pub fn parts(&self) -> (&'static str, Option<&str>, bool) {
    match self {
      ConnState::Idle => ("idle", None, false),
      ConnState::Searching => ("searching", None, false),
      ConnState::Connecting(addr) => ("connecting", Some(addr), false),
      ConnState::Connected { addr, recent } => ("connected", Some(addr), recent.is_some()),
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
          run_direct(addr, &mut cmd_rx, &engine_tx, &state_tx, &dev_server, &flags, &mut outbound_rx, &queries, None)
            .await;
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
///
/// A connect naming what this loop already dials is redundant and ignored
/// rather than treated as an interrupt: the launcher re-dials its launch
/// address on every mount (including the mount a dev push causes), and
/// tearing down the live connection to redial it would make the server
/// re-deliver its latched push to the fresh connection, reloading the
/// launcher into another dial - a reload/reconnect loop that never settles.
async fn run_direct(
  addr: String,
  cmd_rx: &mut UnboundedReceiver<DevCmd>,
  engine_tx: &UnboundedSender<crate::EngineCmd>,
  state_tx: &UnboundedSender<ConnState>,
  dev_server: &DevServerCell,
  flags: &DevFlags,
  outbound_rx: &mut UnboundedReceiver<String>,
  queries: &QueryHandles,
  recent_key: Option<&str>,
) -> Option<DevCmd> {
  let redundant = |cmd: &DevCmd| match cmd {
    DevCmd::Connect(a) => recent_key.is_none() && *a == addr,
    // Tunnels dial a loopback forwarder; the ticket is their identity.
    DevCmd::ConnectTicket(t) => recent_key == Some(t.as_str()),
    _ => false,
  };
  loop {
    let _ = state_tx.send(ConnState::Connecting(addr.clone()));
    {
      let serve = try_serve(&addr, engine_tx, state_tx, dev_server, flags, outbound_rx, queries, recent_key);
      tokio::pin!(serve);
      loop {
        tokio::select! {
          cmd = cmd_rx.recv() => match cmd {
            Some(ref c) if redundant(c) => {
              log::debug!("[sgo] Ignoring connect to {addr}: already the active target");
            }
            cmd => return cmd,
          },
          // Failed to connect or the connection dropped; fall out to the
          // retry pause.
          _ = &mut serve => break,
        }
      }
    }
    // Pause before retrying, but let a new command interrupt the wait (a
    // redundant connect just retries immediately).
    tokio::select! {
      cmd = cmd_rx.recv() => match cmd {
        Some(ref c) if redundant(c) => {}
        cmd => return cmd,
      },
      _ = tokio::time::sleep(Duration::from_secs(3)) => {}
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
  let (addr, _tunnel) = match super::tunnel::start(ticket.clone()).await {
    Ok(started) => started,
    Err(e) => {
      log::error!("[sgo] Tunnel start failed: {e}");
      let _ = state_tx.send(ConnState::Idle);
      return cmd_rx.recv().await;
    }
  };
  // Remember the ticket, not the loopback address it dials: the ticket is the
  // reconnectable identifier (stable across dev-server restarts).
  run_direct(addr.to_string(), cmd_rx, engine_tx, state_tx, dev_server, flags, outbound_rx, queries, Some(&ticket))
    .await
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
        c = try_serve(&server, engine_tx, state_tx, dev_server, flags, outbound_rx, queries, None) => c,
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

/// Install a pushed version, first fetching the assets the store does not
/// already hold from the dev server's /assets/ route (the same origin the
/// WebSocket rides on). Runs inline in the connection task: a push is not
/// applied until its install settles, and dev asset sets are small.
async fn install_push(addr: &str, manifest: &str, code: &str) -> Result<String, String> {
  let (_, missing) = super::store::missing_assets(manifest)?;
  let mut fetched = std::collections::HashMap::new();
  if !missing.is_empty() {
    let client = reqwest::Client::builder()
      .timeout(std::time::Duration::from_secs(30))
      .build()
      .map_err(|e| format!("http client: {e}"))?;
    let base = reqwest::Url::parse(&format!("http://{addr}/")).map_err(|e| format!("server address: {e}"))?;
    for asset in &missing {
      let url = base.join(&asset.path).map_err(|e| format!("asset path {}: {e}", asset.path))?;
      let resp = client.get(url).send().await.map_err(|e| format!("fetch {}: {e}", asset.path))?;
      if !resp.status().is_success() {
        return Err(format!("fetch {}: HTTP {}", asset.path, resp.status()));
      }
      let bytes = resp.bytes().await.map_err(|e| format!("fetch {}: {e}", asset.path))?;
      fetched.insert(asset.path.clone(), bytes.to_vec());
    }
    log::info!("[sgo] Fetched {} asset(s) from the dev server", fetched.len());
  }
  super::store::install(manifest, code, &fetched)
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
  recent_key: Option<&str>,
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
  let _ = state_tx.send(ConnState::Connected { addr: addr.to_string(), recent: recent_key.map(str::to_string) });
  flags.connected.store(true, Ordering::Relaxed);

  // Publish the dialed dev server address so the next engine build installs the
  // file/dir proxy against the server we are actually talking to. Overwrites any
  // previous address so reconnecting to a different server repoints the proxy.
  *dev_server.lock().expect("dev_server lock poisoned") = Some(addr.to_string());

  let info = serde_json::json!({
    "type": "info",
    "platform": flux::platform(),
    "version": crate::VERSION,
    "profile": crate::PROFILE,
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
                let _ = state_tx
                  .send(ConnState::Connected { addr: server_addr.to_string(), recent: recent_key.map(str::to_string) });
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
            let proxy_http = json.get("proxyHttp").and_then(|p| p.as_bool()).unwrap_or(false);
            flags.proxy_http_enabled.store(proxy_http, Ordering::Relaxed);
            if let Some(code) = json.get("code").and_then(|c| c.as_str()) {
              // A push with a manifest is an install: persist the version so
              // the app appears in the launcher's list and launches offline.
              // The reload itself applies the in-memory code either way - a
              // failed install degrades to an ephemeral push.
              let mut app_id = None;
              if let Some(manifest) = json.get("manifest").and_then(|m| m.as_str()) {
                match install_push(addr, manifest, code).await {
                  Ok(id) => app_id = Some(id),
                  Err(e) => log::warn!("[sgo] Version install failed: {e}"),
                }
              }
              // The session's app arguments ride each push (flux:process
              // argv), so every client - local or remote - sees the same
              // vector for the same app.
              let args = json
                .get("args")
                .and_then(|a| a.as_array())
                .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                .unwrap_or_default();
              let _ = tx.send(crate::EngineCmd::Reload { code: code.to_string(), app_id, args });
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
                // The snapshot answers from the draw loop's latch; the mounted
                // count is derived from the live tree on the JS thread when an
                // engine runs, so mounted-vs-orphan is exact at query time (an
                // orphan gap growing at a stable tree shape is an unmount leak).
                let snap = *queries.stats.lock().expect("stats snapshot lock poisoned");
                let exec = queries.exec.lock().expect("exec handle lock poisoned").clone();
                match exec {
                  Some(eh) => {
                    let reply_tx = queries.outbound_tx.clone();
                    eh.exec(move |ctx| {
                      let counts = ctx.userdata::<flux::gui::tree::SharedRenderTree>().map(|t| {
                        let tree = t.0.borrow();
                        (tree.mounted_count(), tree.node_count())
                      });
                      // Read live, not from the frame-latched snapshot: a
                      // backlogged raster thread produces no frames, so the
                      // latch goes stale exactly when these matter.
                      let raster = ctx.userdata::<flux::gui::AlloyContext>().map(|atx| RasterCounters {
                        queue: atx.raster_queue_depth(),
                        idle_ticks: atx.idle_ticks(),
                        fence_timeouts: atx.fence_timeouts(),
                        passes: atx.passes(),
                        pass_micros: atx.pass_micros(),
                        cmd_micros: atx.cmd_micros(),
                      });
                      let _ = reply_tx.send(stats_reply(id, snap, counts, raster));
                    });
                  }
                  None => {
                    let _ = client.send(tokio_websockets::Message::text(stats_reply(id, snap, None, None))).await;
                  }
                }
              }
              Some("tree") => {
                // The render tree lives on the JS thread; snapshot it there and
                // route the reply back through the outbound channel. Optional
                // scoping: `root` (subtree), `depth` (level cap), `query`
                // (kind/text search instead of a snapshot).
                let root = json.get("root").and_then(|n| n.as_u64());
                let depth = json.get("depth").and_then(|n| n.as_u64()).map(|n| n as usize);
                let search = json.get("query").and_then(|q| q.as_str()).map(str::to_string);
                let exec = queries.exec.lock().expect("exec handle lock poisoned").clone();
                match exec {
                  Some(eh) => {
                    let reply_tx = queries.outbound_tx.clone();
                    eh.exec(move |ctx| {
                      let _ = reply_tx.send(tree_reply(&ctx, id, root, depth, search.as_deref()));
                    });
                  }
                  None => {
                    let _ = client.send(tokio_websockets::Message::text(error_reply(id, "no running engine"))).await;
                  }
                }
              }
              Some("snapshot") => {
                // Rasterize a node's subtree to a texture on the JS thread and
                // route the PNG reply back through the outbound channel. The
                // capture is async (serviced on a paint), so unlike tree/stats
                // the reply is sent from the capture callback, not here.
                let node_id = json.get("nodeId").and_then(|n| n.as_u64()).unwrap_or(0);
                let exec = queries.exec.lock().expect("exec handle lock poisoned").clone();
                match exec {
                  Some(eh) => {
                    let reply_tx = queries.outbound_tx.clone();
                    let frame_requested = flags.frame_requested.clone();
                    eh.exec(move |ctx| request_snapshot(&ctx, node_id, id, reply_tx, frame_requested));
                  }
                  None => {
                    let _ = client.send(tokio_websockets::Message::text(error_reply(id, "no running engine"))).await;
                  }
                }
              }
              Some("gpu") => {
                // Alloy's GPU bookkeeping lives on the JS thread (its GL context
                // is current there); snapshot it there like the render tree.
                let exec = queries.exec.lock().expect("exec handle lock poisoned").clone();
                match exec {
                  Some(eh) => {
                    let reply_tx = queries.outbound_tx.clone();
                    eh.exec(move |ctx| {
                      let _ = reply_tx.send(gpu_reply(&ctx, id));
                    });
                  }
                  None => {
                    let _ = client.send(tokio_websockets::Message::text(error_reply(id, "no running engine"))).await;
                  }
                }
              }
              Some("texture") => {
                // Read back a registered texture's pixels. Unlike snapshot this
                // needs no paint pass (the texture already exists), so the reply
                // is built synchronously on the JS thread.
                let texture_id = json.get("textureId").and_then(|n| n.as_u64()).unwrap_or(0);
                let rect = json.get("rect").and_then(|r| {
                  Some((
                    r.get("x")?.as_u64()? as u32,
                    r.get("y")?.as_u64()? as u32,
                    r.get("width")?.as_u64()? as u32,
                    r.get("height")?.as_u64()? as u32,
                  ))
                });
                let exec = queries.exec.lock().expect("exec handle lock poisoned").clone();
                match exec {
                  Some(eh) => {
                    let reply_tx = queries.outbound_tx.clone();
                    eh.exec(move |ctx| {
                      let _ = reply_tx.send(texture_reply(&ctx, id, texture_id, rect));
                    });
                  }
                  None => {
                    let _ = client.send(tokio_websockets::Message::text(error_reply(id, "no running engine"))).await;
                  }
                }
              }
              Some("buffer") => {
                // Read back part of a vertex buffer, decoded to numbers on the
                // JS thread (glMapBufferRange needs the GL context current).
                let buffer_id = json.get("bufferId").and_then(|n| n.as_u64()).unwrap_or(0);
                let byte_offset = json.get("byteOffset").and_then(|n| n.as_u64()).unwrap_or(0) as usize;
                let length = json.get("length").and_then(|n| n.as_u64()).map(|n| n as usize);
                let fmt = json.get("as").and_then(|f| f.as_str()).unwrap_or("f32").to_string();
                let exec = queries.exec.lock().expect("exec handle lock poisoned").clone();
                match exec {
                  Some(eh) => {
                    let reply_tx = queries.outbound_tx.clone();
                    eh.exec(move |ctx| {
                      let _ = reply_tx.send(buffer_reply(&ctx, id, buffer_id, byte_offset, length, &fmt));
                    });
                  }
                  None => {
                    let _ = client.send(tokio_websockets::Message::text(error_reply(id, "no running engine"))).await;
                  }
                }
              }
              Some("debug_list") => {
                let exec = queries.exec.lock().expect("exec handle lock poisoned").clone();
                match exec {
                  Some(eh) => {
                    let reply_tx = queries.outbound_tx.clone();
                    eh.exec(move |ctx| {
                      let _ = reply_tx.send(debug_list_reply(&ctx, id));
                    });
                  }
                  None => {
                    let _ = client.send(tokio_websockets::Message::text(error_reply(id, "no running engine"))).await;
                  }
                }
              }
              Some("debug_call") => {
                // Call an app-registered debug command on the JS thread, args
                // in and return value out as JSON.
                let name = json.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string();
                let args = json.get("args").filter(|a| !a.is_null()).cloned();
                let exec = queries.exec.lock().expect("exec handle lock poisoned").clone();
                match exec {
                  Some(eh) => {
                    let reply_tx = queries.outbound_tx.clone();
                    eh.exec(move |ctx| {
                      let _ = reply_tx.send(debug_call_reply(&ctx, id, &name, args));
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

/// GPU-side counters read live from the alloy context at query time (never
/// from the frame-latched snapshot, which goes stale exactly when the raster
/// thread wedges): a nonzero queue with idle ticks racing is the
/// idle-tick-runaway signature, climbing fence timeouts mean the GPU is over
/// budget, and passes racing ahead of presented frames means redundant
/// shader/pipeline target re-renders (see
/// okf/backlog/idle-tick-gpu-backlog-runaway.md). All cumulative except
/// `queue`; consumers diff between queries.
struct RasterCounters {
  queue: usize,
  idle_ticks: u64,
  fence_timeouts: u64,
  passes: u64,
  pass_micros: u64,
  cmd_micros: u64,
}

/// `counts` is (mounted, total) from the live tree when the query could run on
/// the JS thread; the reply then carries mountedNodes and orphanNodes (total -
/// mounted: nodes unreachable from the root, i.e. leaked or intentionally kept
/// detached). Without an engine the counts and raster fields are simply
/// absent.
fn stats_reply(
  id: u64,
  s: crate::overlay::StatsSnapshot,
  counts: Option<(usize, usize)>,
  raster: Option<RasterCounters>,
) -> String {
  let mut data = serde_json::json!({
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
      "nodes": s.node_count,
      "measureCalls": s.measure_calls,
      "paraShapes": s.para_shapes,
      "dirtiedNodes": s.dirtied,
      "cacheGets": s.cache_gets,
      "cacheHits": s.cache_hits,
  });
  let map = data.as_object_mut().expect("stats data is an object");
  if let Some((mounted, total)) = counts {
    map.insert("mountedNodes".into(), mounted.into());
    map.insert("orphanNodes".into(), total.saturating_sub(mounted).into());
  }
  if let Some(r) = raster {
    map.insert("rasterQueue".into(), r.queue.into());
    map.insert("idleTicks".into(), r.idle_ticks.into());
    map.insert("fenceTimeouts".into(), r.fence_timeouts.into());
    map.insert("gpuPasses".into(), r.passes.into());
    // Integer ms: sub-ms increments accumulate in the microsecond counters
    // before this division, so the cumulative rounding loss stays under 1ms.
    map.insert("gpuPassMs".into(), (r.pass_micros / 1000).into());
    map.insert("rasterCmdMs".into(), (r.cmd_micros / 1000).into());
  }
  serde_json::json!({"type": "result", "id": id, "data": data}).to_string()
}

// Search results are for locating nodes, not dumping the app: enough for a
// "which node is X" question, small enough to never rebuild the 3MB-tree
// problem the query option exists to avoid.
const TREE_MATCH_LIMIT: usize = 100;

/// Snapshot the render tree from the engine's userdata and encode it. With
/// `search`, reply with the matching nodes (id paths included) instead of a
/// subtree. Runs on the JS thread (see the query handling above).
fn tree_reply(
  ctx: &flux::rquickjs::Ctx<'_>,
  id: u64,
  root: Option<u64>,
  depth: Option<usize>,
  search: Option<&str>,
) -> String {
  let Some(tree) = ctx.userdata::<flux::gui::tree::SharedRenderTree>() else {
    return error_reply(id, "no render tree");
  };
  let tree = tree.0.borrow();
  if let Some(needle) = search {
    return match tree.snapshot_matches(root, needle, TREE_MATCH_LIMIT) {
      Some(matches) => {
        let entries: Vec<_> = matches
          .iter()
          .map(|m| {
            let mut obj = node_json(&m.node);
            let map = obj.as_object_mut().expect("node_json is an object");
            map.remove("children");
            map.insert("path".into(), m.path.clone().into());
            obj
          })
          .collect();
        serde_json::json!({"type": "result", "id": id, "data": {"matches": entries, "limit": TREE_MATCH_LIMIT}})
          .to_string()
      }
      None => match root {
        Some(r) => error_reply(id, &format!("no node with id {r}")),
        None => error_reply(id, "no render tree (the app has not rendered)"),
      },
    };
  }
  match tree.snapshot_from(root, depth) {
    Some(node) => serde_json::json!({"type": "result", "id": id, "data": node_json(&node)}).to_string(),
    None => match root {
      Some(r) => error_reply(id, &format!("no node with id {r}")),
      None => error_reply(id, "no render tree (the app has not rendered)"),
    },
  }
}

/// Queue a snapshot capture of `node_id` on the alloy context (reached from JS
/// userdata, like `tree_reply` reaches the render tree). The completion callback
/// runs on this same JS thread during the paint pass that services the capture:
/// it reads the texture back, PNG-encodes it, frees the texture, and routes the
/// reply out. Runs on the JS thread via the engine exec handle.
fn request_snapshot(
  ctx: &flux::rquickjs::Ctx<'_>,
  node_id: u64,
  id: u64,
  reply_tx: UnboundedSender<String>,
  frame_requested: Arc<AtomicBool>,
) {
  let Some(atx) = ctx.userdata::<flux::gui::AlloyContext>() else {
    let _ = reply_tx.send(error_reply(id, "no alloy context"));
    return;
  };
  let alloy = atx.0.clone();
  let encode_alloy = alloy.clone();
  alloy.request_capture(
    node_id,
    Box::new(move |result| {
      let reply = match result {
        Ok(info) => {
          let read = encode_alloy.read_texture_by_id(info.texture_id);
          encode_alloy.destroy_texture(info.texture_id);
          match read {
            Ok((width, height, pixels)) => snapshot_reply(id, width, height, pixels),
            Err(e) => error_reply(id, &e),
          }
        }
        Err(e) => error_reply(id, &e),
      };
      let _ = reply_tx.send(reply);
    }),
  );
  // Latch a frame only after the capture is queued (matching captureSnapshot's
  // order), so a Tick-driven draw cannot consume the latch before the request
  // is registered and leave the capture stranded. The app may be idle; the idle
  // Tick then services it within a refresh period.
  frame_requested.store(true, Ordering::Relaxed);
}

/// Encode a captured RGBA8 buffer as a base64 PNG reply. On-demand and rare (a
/// dev-server query), so encoding inline on the JS thread is fine.
fn snapshot_reply(id: u64, width: u32, height: u32, rgba: Vec<u8>) -> String {
  let png = match forge::image::encode_png(&rgba, width, height) {
    Ok(png) => png,
    Err(e) => return error_reply(id, &format!("png encode failed: {e}")),
  };
  let b64 = base64::engine::general_purpose::STANDARD.encode(&png);
  serde_json::json!({
    "type": "result",
    "id": id,
    "data": { "pngBase64": b64, "width": width, "height": height },
  })
  .to_string()
}

/// Add the create's debug label to a resource object, when one was given
/// (absent otherwise, like the other off-default keys).
fn insert_label(obj: &mut serde_json::Value, label: &Option<String>) {
  if let Some(label) = label {
    obj.as_object_mut().expect("resource json is an object").insert("label".into(), label.clone().into());
  }
}

/// Inventory alloy's GPU bookkeeping (textures, buffers, pipelines, programs)
/// and encode it. Runs on the JS thread (see the query handling above).
fn gpu_reply(ctx: &flux::rquickjs::Ctx<'_>, id: u64) -> String {
  let Some(atx) = ctx.userdata::<flux::gui::AlloyContext>() else {
    return error_reply(id, "no alloy context");
  };
  let res = atx.0.gpu_resources();

  let textures: Vec<serde_json::Value> = res
    .textures
    .iter()
    .map(|t| {
      let mut obj = serde_json::json!({"id": t.id, "width": t.width, "height": t.height, "target": t.target, "format": t.format});
      insert_label(&mut obj, &t.label);
      obj
    })
    .collect();
  let buffers: Vec<serde_json::Value> = res
    .buffers
    .iter()
    .map(|b| {
      let mut obj = serde_json::json!({"id": b.id, "byteLength": b.byte_length});
      insert_label(&mut obj, &b.label);
      obj
    })
    .collect();
  let pipelines: Vec<serde_json::Value> = res
    .pipelines
    .iter()
    .map(|p| {
      let mut obj = serde_json::json!({
        "textureId": p.texture_id,
        "kind": p.kind,
        // Manual targets render only on renderTarget, never by the flush.
        "manual": p.manual,
        "loadOp": if p.load { "load" } else { "clear" },
        // Cumulative like the get_stats aggregates (diff two queries for a
        // rate); passMs is raster-thread occupancy, not GPU-side duration.
        "passes": p.passes,
        "passMs": p.pass_micros / 1000,
        "textures": p.textures.iter().map(|(name, tex)| (name.clone(), serde_json::json!(tex))).collect::<serde_json::Map<_, _>>(),
        "params": p.params.iter().map(|(name, v)| {
          let v = match v {
            alloy::ParamValue::Scalar(n) => serde_json::json!(n),
            alloy::ParamValue::Array(a) => serde_json::json!(a),
          };
          (name.clone(), v)
        }).collect::<serde_json::Map<_, _>>(),
      });
      insert_label(&mut obj, &p.label);
      let map = obj.as_object_mut().expect("pipeline json is an object");
      if let Some(program_id) = p.program_id {
        map.insert("programId".into(), program_id.into());
      }
      if let Some(pipeline_id) = p.pipeline_id {
        map.insert("pipelineId".into(), pipeline_id.into());
      }
      if let Some(buffer_id) = p.buffer_id {
        map.insert("bufferId".into(), buffer_id.into());
      }
      // An index binding is itself off-default; with one present the range
      // keys switch to the index spellings (the numbers count indices).
      let indexed = p.index_buffer_id.is_some();
      if let Some(index_buffer_id) = p.index_buffer_id {
        map.insert("indexBuffer".into(), index_buffer_id.into());
      }
      if let Some(index_format) = p.index_format {
        map.insert("indexFormat".into(), index_format.into());
      }
      if let Some(instance_buffer_id) = p.instance_buffer_id {
        map.insert("instanceBuffer".into(), instance_buffer_id.into());
      }
      if let Some(topology) = p.topology {
        map.insert("topology".into(), topology.into());
      }
      if let Some(draw_count) = p.draw_count {
        map.insert(if indexed { "indexCount".into() } else { "drawCount".into() }, draw_count.into());
      }
      // Reported only off their defaults, like depthWrite below: absent
      // means the plain draw from the buffer's start.
      if let Some(first_vertex) = p.first_vertex.filter(|v| *v != 0) {
        map.insert(if indexed { "firstIndex".into() } else { "firstVertex".into() }, first_vertex.into());
      }
      if let Some(instance_count) = p.instance_count.filter(|v| *v != 1) {
        map.insert("instanceCount".into(), instance_count.into());
      }
      if p.depth {
        map.insert("depth".into(), true.into());
      }
      // Reported only off their defaults, like depth: absent means the
      // ordinary opaque draw.
      if p.depth_write == Some(false) {
        map.insert("depthWrite".into(), false.into());
      }
      if let Some(blend) = p.blend.filter(|b| *b != "none") {
        map.insert("blend".into(), blend.into());
      }
      if let Some(cull) = p.cull.filter(|c| *c != "none") {
        map.insert("cull".into(), cull.into());
      }
      if !p.attributes.is_empty() {
        let attrs: Vec<serde_json::Value> =
          p.attributes.iter().map(|(name, format)| serde_json::json!({"name": name, "format": format})).collect();
        map.insert("attributes".into(), attrs.into());
      }
      if !p.instance_attributes.is_empty() {
        let attrs: Vec<serde_json::Value> = p
          .instance_attributes
          .iter()
          .map(|(name, format)| serde_json::json!({"name": name, "format": format}))
          .collect();
        map.insert("instanceAttributes".into(), attrs.into());
      }
      // A draw target (kind "draws") reports its entries in list order; each
      // entry follows the flat fields' off-default conventions.
      if p.kind == "draws" {
        let draws: Vec<serde_json::Value> = p
          .draws
          .iter()
          .map(|d| {
            let mut entry = serde_json::json!({
              "id": d.id,
              "textures": d.textures.iter().map(|(name, tex)| (name.clone(), serde_json::json!(tex))).collect::<serde_json::Map<_, _>>(),
              "params": d.params.iter().map(|(name, v)| {
                let v = match v {
                  alloy::ParamValue::Scalar(n) => serde_json::json!(n),
                  alloy::ParamValue::Array(a) => serde_json::json!(a),
                };
                (name.clone(), v)
              }).collect::<serde_json::Map<_, _>>(),
            });
            let map = entry.as_object_mut().expect("draw json is an object");
            if let Some(pipeline_id) = d.pipeline_id {
              map.insert("pipelineId".into(), pipeline_id.into());
            }
            if let Some(buffer_id) = d.buffer_id {
              map.insert("bufferId".into(), buffer_id.into());
            }
            // An index binding is itself off-default; with one present the
            // range keys switch to the index spellings (indices, not
            // vertices).
            let indexed = d.index_buffer_id.is_some();
            if let Some(index_buffer_id) = d.index_buffer_id {
              map.insert("indexBuffer".into(), index_buffer_id.into());
            }
            if let Some(index_format) = d.index_format {
              map.insert("indexFormat".into(), index_format.into());
            }
            if let Some(instance_buffer_id) = d.instance_buffer_id {
              map.insert("instanceBuffer".into(), instance_buffer_id.into());
            }
            map.insert("topology".into(), d.topology.into());
            map.insert(if indexed { "indexCount".into() } else { "vertexCount".into() }, d.vertex_count.into());
            if d.first_vertex != 0 {
              map.insert(if indexed { "firstIndex".into() } else { "firstVertex".into() }, d.first_vertex.into());
            }
            if d.instance_count != 1 {
              map.insert("instanceCount".into(), d.instance_count.into());
            }
            if !d.depth_write {
              map.insert("depthWrite".into(), false.into());
            }
            if d.blend != "none" {
              map.insert("blend".into(), d.blend.into());
            }
            if d.cull != "none" {
              map.insert("cull".into(), d.cull.into());
            }
            entry
          })
          .collect();
        map.insert("draws".into(), draws.into());
      }
      obj
    })
    .collect();

  let render_pipelines: Vec<serde_json::Value> = res
    .render_pipelines
    .iter()
    .map(|p| {
      let mut obj = serde_json::json!({"id": p.id, "programId": p.program_id});
      insert_label(&mut obj, &p.label);
      let map = obj.as_object_mut().expect("render pipeline json is an object");
      // Draw state reported only off its defaults, like the per-target infos.
      if p.topology != "triangles" {
        map.insert("topology".into(), p.topology.into());
      }
      if p.blend != "none" {
        map.insert("blend".into(), p.blend.into());
      }
      if p.cull != "none" {
        map.insert("cull".into(), p.cull.into());
      }
      if p.depth {
        map.insert("depth".into(), true.into());
      }
      if !p.depth_write {
        map.insert("depthWrite".into(), false.into());
      }
      if !p.attributes.is_empty() {
        let attrs: Vec<serde_json::Value> =
          p.attributes.iter().map(|(name, format)| serde_json::json!({"name": name, "format": format})).collect();
        map.insert("attributes".into(), attrs.into());
      }
      if !p.instance_attributes.is_empty() {
        let attrs: Vec<serde_json::Value> = p
          .instance_attributes
          .iter()
          .map(|(name, format)| serde_json::json!({"name": name, "format": format}))
          .collect();
        map.insert("instanceAttributes".into(), attrs.into());
      }
      obj
    })
    .collect();

  let programs: Vec<serde_json::Value> = res
    .programs
    .iter()
    .map(|p| {
      let mut obj = serde_json::json!({"id": p.id});
      insert_label(&mut obj, &p.label);
      obj
    })
    .collect();

  let mut data = serde_json::json!({
    "textures": textures, "buffers": buffers, "pipelines": pipelines,
    "renderPipelines": render_pipelines, "programs": programs,
  });
  if let Some(ws) = &res.window_shader {
    data["windowShader"] = serde_json::json!({
      "programId": ws.program_id, "layerWidth": ws.width, "layerHeight": ws.height,
      "previous": ws.previous, "passOnlyFrames": ws.pass_only_frames,
    });
  }

  serde_json::json!({
    "type": "result",
    "id": id,
    "data": data,
  })
  .to_string()
}

/// Read back a registered texture's pixels (optionally cropped to `rect`) and
/// encode them as a PNG reply. Runs on the JS thread.
fn texture_reply(
  ctx: &flux::rquickjs::Ctx<'_>,
  id: u64,
  texture_id: u64,
  rect: Option<(u32, u32, u32, u32)>,
) -> String {
  let Some(atx) = ctx.userdata::<flux::gui::AlloyContext>() else {
    return error_reply(id, "no alloy context");
  };
  match atx.0.read_texture_by_id(texture_id) {
    Err(e) => error_reply(id, &e),
    Ok((width, height, pixels)) => match rect {
      None => snapshot_reply(id, width, height, pixels),
      Some((x, y, w, h)) => {
        if w == 0 || h == 0 || x.saturating_add(w) > width || y.saturating_add(h) > height {
          return error_reply(id, &format!("rect {w}x{h} at {x},{y} outside texture {width}x{height}"));
        }
        // The full readback is already in hand; cropping CPU-side keeps the
        // GL path identical to the uncropped case.
        let mut cropped = Vec::with_capacity((w as usize) * (h as usize) * 4);
        for row in y..y + h {
          let start = ((row as usize) * (width as usize) + x as usize) * 4;
          cropped.extend_from_slice(&pixels[start..start + (w as usize) * 4]);
        }
        snapshot_reply(id, w, h, cropped)
      }
    },
  }
}

// Per-call cap on buffer readback, so one query cannot stall the JS thread on
// a huge map + JSON encode. Callers page through with byteOffset.
const BUFFER_READ_CAP_BYTES: usize = 65536;

/// Read back part of a vertex buffer and decode it to numbers. `length` counts
/// elements of `fmt` (not bytes); omitted means the rest of the buffer, capped.
/// Runs on the JS thread.
fn buffer_reply(
  ctx: &flux::rquickjs::Ctx<'_>,
  id: u64,
  buffer_id: u64,
  byte_offset: usize,
  length: Option<usize>,
  fmt: &str,
) -> String {
  let Some(atx) = ctx.userdata::<flux::gui::AlloyContext>() else {
    return error_reply(id, "no alloy context");
  };
  let elem_size = match fmt {
    "f32" => 4,
    "u16" => 2,
    "u8" => 1,
    _ => return error_reply(id, &format!("unsupported as '{fmt}' (expected f32|u16|u8)")),
  };
  let total = match atx.0.gpu_buffer_len(buffer_id) {
    Ok(n) => n,
    Err(e) => return error_reply(id, &e),
  };
  if byte_offset >= total {
    return error_reply(id, &format!("byteOffset {byte_offset} beyond buffer size {total}"));
  }
  let avail = total - byte_offset;
  let want = length.map(|n| n.saturating_mul(elem_size)).unwrap_or(avail).min(avail);
  // Whole elements only, so a cap or short buffer never splits a value.
  let len = (want.min(BUFFER_READ_CAP_BYTES) / elem_size) * elem_size;
  match atx.0.read_gpu_buffer(buffer_id, byte_offset, len) {
    Err(e) => error_reply(id, &e),
    Ok(bytes) => {
      // Native endianness: the bytes came from typed arrays in this same
      // process. Non-finite floats serialize as null (JSON has no NaN).
      let values: Vec<serde_json::Value> = match fmt {
        "f32" => {
          bytes.chunks_exact(4).map(|c| serde_json::json!(f32::from_ne_bytes([c[0], c[1], c[2], c[3]]))).collect()
        }
        "u16" => bytes.chunks_exact(2).map(|c| u16::from_ne_bytes([c[0], c[1]]).into()).collect(),
        _ => bytes.iter().map(|b| (*b).into()).collect(),
      };
      serde_json::json!({
        "type": "result",
        "id": id,
        "data": {
          "values": values,
          "byteOffset": byte_offset,
          "byteLength": len,
          "bufferByteLength": total,
        },
      })
      .to_string()
    }
  }
}

/// List the app's registered debug commands. Runs on the JS thread.
fn debug_list_reply(ctx: &flux::rquickjs::Ctx<'_>, id: u64) -> String {
  let commands = match ctx.userdata::<crate::plugins::dev::DebugRegistry>() {
    Some(registry) => registry.names(),
    // The registry is installed on first `srt:dev` import; an app that never
    // imported it simply has no commands.
    None => Vec::new(),
  };
  serde_json::json!({"type": "result", "id": id, "data": {"commands": commands}}).to_string()
}

/// Call a registered debug command with JSON args and encode its return value
/// (undefined -> null). JS exceptions become error replies. Runs on the JS
/// thread.
fn debug_call_reply(ctx: &flux::rquickjs::Ctx<'_>, id: u64, name: &str, args: Option<serde_json::Value>) -> String {
  let Some(registry) = ctx.userdata::<crate::plugins::dev::DebugRegistry>() else {
    return error_reply(id, &format!("no debug command '{name}' (none registered)"));
  };
  let Some(persistent) = registry.get(name) else {
    let names = registry.names();
    let hint =
      if names.is_empty() { "none registered".to_string() } else { format!("registered: {}", names.join(", ")) };
    return error_reply(id, &format!("no debug command '{name}' ({hint})"));
  };
  let func = match persistent.restore(ctx) {
    Ok(f) => f,
    Err(e) => return error_reply(id, &format!("restore failed: {e}")),
  };

  let result: Result<flux::rquickjs::Value, flux::rquickjs::Error> = match args {
    Some(a) => match ctx.json_parse(a.to_string()) {
      Ok(parsed) => func.call((parsed,)),
      Err(e) => return error_reply(id, &format!("args parse failed: {e}")),
    },
    None => func.call(()),
  };

  match result {
    Err(e) => error_reply(id, &flux::rquickjs::CaughtError::from_error(ctx, e).to_string()),
    Ok(value) => {
      // A returned Promise stringifies as {} - async commands are not
      // supported (yet); commands must return synchronously.
      match ctx.json_stringify(value) {
        Ok(Some(s)) => {
          let text = s.to_string().unwrap_or_default();
          let data: serde_json::Value = serde_json::from_str(&text).unwrap_or(serde_json::Value::Null);
          serde_json::json!({"type": "result", "id": id, "data": {"value": data}}).to_string()
        }
        Ok(None) => serde_json::json!({"type": "result", "id": id, "data": {"value": null}}).to_string(),
        Err(e) => error_reply(
          id,
          &format!("return value is not JSON-serializable: {}", flux::rquickjs::CaughtError::from_error(ctx, e)),
        ),
      }
    }
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
  // A depth cap cut this node's children off: surface how many exist so a
  // reader knows to descend with root=<id>.
  if node.children.len() < node.child_count {
    map.insert("childCount".into(), node.child_count.into());
  }
  obj
}
