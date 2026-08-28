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
  /// Dev-tool pause/step/scale state, applied by the frame verb (see
  /// runtime::ClockControl); written from `clock` queries, reset on
  /// reload/stop so no app starts under a stale pause.
  pub clock: crate::runtime::ClockControl,
  /// Synthetic-input sender for `input` queries: downs/ups pushed here enter
  /// the UI thread's batch loop through the same channel as real SDL input
  /// (hit testing, input-state bookkeeping, focus). Moves follow the
  /// producer-side resampler rule instead (see `resampler`).
  pub input_tx: UnboundedSender<alloy::AlloyEvent>,
  /// Resampler feed for injected pointer events, mirroring the alloy pump:
  /// moves are consumed into it (never sent as events), downs seed and ups
  /// drop the history before their events travel (see alloy's resample.rs).
  pub resampler: alloy::resample::SharedResampler,
  /// The alloy run loop's user-input mute (App::user_input_mute), set by
  /// the dev tools while an agent measures or tests: the server's `mute`
  /// message (and `welcome`, for a client joining while muted) flips it; a
  /// lost connection clears it, so a dead server never leaves the user
  /// locked out. It survives reload: a mute spans the agent's rebuilds.
  pub user_input_muted: Arc<AtomicBool>,
}

/// Apply the server's mute state to the user-input mute, logging the
/// transition so the human at the client can tell why input stopped.
fn set_mute(flags: &DevFlags, active: bool) {
  if flags.user_input_muted.swap(active, Ordering::Relaxed) != active {
    if active {
      log::info!("[sgo] User input muted by the dev tools until unmuted");
    } else {
      log::info!("[sgo] User input unmuted");
    }
  }
}

/// Send-safe handles the connection answers dev-server queries from, without a
/// round trip through the UI thread: the stats snapshot the draw loop
/// publishes, the live engine's exec handle (refreshed on each engine build),
/// and a sender on the outbound channel for replies produced on the JS thread.
#[derive(Clone)]
pub struct QueryHandles {
  pub stats: Arc<Mutex<crate::stats::StatsSnapshot>>,
  pub history: Arc<Mutex<crate::frame_history::FrameHistory>>,
  pub exec: Arc<Mutex<Option<flux::ExecHandle>>>,
  pub outbound_tx: UnboundedSender<String>,
}

/// Query kinds this runtime answers, advertised in the connect-time `info`
/// message so dev tools can plan against a client's actual surface before
/// calling (mixed-version fleets are normal). Keep in sync with the query
/// match in `try_serve`.
const QUERY_KINDS: &[&str] =
  &["clock", "input", "stats", "tree", "snapshot", "gpu", "texture", "buffer", "debug_list", "debug_call"];

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

  // What this client already knows about itself and never changes, told
  // once so a dev tool can tell clients apart and see what machine each one
  // is: its storage tree (<data-root>/client<N>, or the launcher/packed
  // folder; null without writable storage), pid and executable, the host
  // and OS, the SDL video driver and the GPU strings. The GPU strings come
  // from the raster thread's context; a connect that wins that race sends
  // null, which a reconnect corrects.
  let info = serde_json::json!({
    "type": "info",
    "platform": flux::platform(),
    "version": crate::VERSION,
    "profile": crate::PROFILE,
    "capabilities": flux::capabilities(),
    "queries": QUERY_KINDS,
    "clientDir": crate::storage::get().map(|store| store.client_dir.to_string_lossy().into_owned()),
    "pid": std::process::id(),
    "execPath": forge::process::exec_path(),
    "host": forge::process::host_name(),
    "os": forge::process::os_description(),
    "kernel": forge::process::kernel_version(),
    "videoDriver": alloy::video_driver(),
    "gpu": alloy::gpu_info().map(|gpu| serde_json::json!({
      "vendor": gpu.vendor,
      "renderer": gpu.renderer,
      "version": gpu.version,
    })),
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
            // The server's mute state, for a client joining while muted
            // (see DevFlags::user_input_muted).
            if let Some(active) = json.get("mute").and_then(|m| m.as_bool()) {
              set_mute(&flags, active);
            }
          }
          Some("mute") => {
            // The user-input mute going on or off: the server's /mute
            // control endpoint, broadcast to every client.
            if let Some(active) = json.get("active").and_then(|a| a.as_bool()) {
              set_mute(&flags, active);
            }
          }
          Some("reload") => {
            // A fresh push must not start under a stale dev-tool pause: an
            // agent (or human) that paused and forgot would see every later
            // app boot frozen.
            flags.clock.reset();
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
            // The launcher must never come up paused.
            flags.clock.reset();
            let _ = tx.send(crate::EngineCmd::Stop);
          }
          Some("query") => {
            let id = json.get("id").and_then(|i| i.as_u64()).unwrap_or(0);
            match json.get("kind").and_then(|k| k.as_str()) {
              Some("clock") => {
                // Dev-tool clock control (pause/step/scale): the state is
                // atomics shared with the frame verb, so apply and ack right
                // here without a JS-thread round trip. The latch makes the
                // change visible promptly even on an idle app (a stepped or
                // resumed frame presents; a pause shows its current state).
                if let Some(scale) = json.get("scale").and_then(|s| s.as_f64()) {
                  flags.clock.set_scale(scale);
                }
                if let Some(step) = json.get("step").and_then(|s| s.as_u64()) {
                  flags.clock.add_steps(step);
                }
                flags.frame_requested.store(true, Ordering::Relaxed);
                let reply = serde_json::json!({
                  "type": "result",
                  "id": id,
                  "data": { "scale": flags.clock.scale(), "pendingSteps": flags.clock.pending_steps() },
                })
                .to_string();
                let _ = client.send(tokio_websockets::Message::text(reply)).await;
              }
              Some("input") => {
                // Synthetic input: parsed events enter the same channel real
                // SDL input feeds (see DevFlags::input_tx), deliberately with
                // no frame-request latch - like real input, the app's own
                // handlers request whatever frames their reactions need. Timed
                // sequences run on their own task so a hold or delay never
                // blocks this loop; the reply follows the last event so a
                // caller knows the gesture has fully entered the pipeline.
                match parse_input_events(json.get("events")) {
                  Ok(seq) => {
                    let delivered = seq.len();
                    let input_tx = flags.input_tx.clone();
                    let resampler = flags.resampler.clone();
                    let reply_tx = queries.outbound_tx.clone();
                    tokio::spawn(async move {
                      for (delay_ms, event) in seq {
                        if delay_ms > 0 {
                          tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                        }
                        // Producer-side resampler feed, mirroring the alloy
                        // pump (see DevFlags::resampler): moves are consumed
                        // here and dispatch from the frame verb's samples.
                        if resampler.feed(&event) {
                          continue;
                        }
                        if input_tx.send(event).is_err() {
                          // The runtime is shutting down; nobody left to reply to.
                          return;
                        }
                      }
                      let reply =
                        serde_json::json!({"type": "result", "id": id, "data": {"delivered": delivered}}).to_string();
                      let _ = reply_tx.send(reply);
                    });
                  }
                  Err(e) => {
                    let _ = client.send(tokio_websockets::Message::text(error_reply(id, &e))).await;
                  }
                }
              }
              Some("stats") => {
                // The snapshot answers from the draw loop's latch; the mounted
                // count is derived from the live tree on the JS thread when an
                // engine runs, so mounted-vs-orphan is exact at query time (an
                // orphan gap growing at a stable tree shape is an unmount leak).
                let snap = *queries.stats.lock().expect("stats snapshot lock poisoned");
                // The window summary reads the frame history at query time
                // (not on the JS thread: a wedged app must still answer).
                let window_ms = json
                  .get("windowMs")
                  .and_then(|w| w.as_f64())
                  .unwrap_or(STATS_WINDOW_DEFAULT_MS);
                let now_ms = crate::frame_history::now_ms();
                let window = queries.history.lock().expect("frame history lock poisoned").summarize(window_ms, now_ms);
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
                      let raster = ctx.userdata::<flux::gui::AlloyContext>().map(|atx| atx.raster_counters());
                      let reply = StatsReply { snap, time_ms: now_ms, window: window.as_ref(), counts, raster };
                      let _ = reply_tx.send(stats_reply(id, reply));
                    });
                  }
                  None => {
                    let reply = StatsReply { snap, time_ms: now_ms, window: window.as_ref(), counts: None, raster: None };
                    let _ = client.send(tokio_websockets::Message::text(stats_reply(id, reply))).await;
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
                let props = json.get("props").and_then(|p| p.as_bool()).unwrap_or(false);
                let exec = queries.exec.lock().expect("exec handle lock poisoned").clone();
                match exec {
                  Some(eh) => {
                    let reply_tx = queries.outbound_tx.clone();
                    eh.exec(move |ctx| {
                      let _ = reply_tx.send(tree_reply(&ctx, id, root, depth, search.as_deref(), props));
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
                let rect = query_rect(&json);
                let scale = query_scale(&json);
                let raw = query_raw(&json);
                let exec = queries.exec.lock().expect("exec handle lock poisoned").clone();
                match exec {
                  Some(eh) => {
                    let reply_tx = queries.outbound_tx.clone();
                    let frame_requested = flags.frame_requested.clone();
                    eh.exec(move |ctx| request_snapshot(&ctx, node_id, id, rect, scale, raw, reply_tx, frame_requested));
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
                    let label = json.get("label").and_then(|l| l.as_str()).map(str::to_string);
                    eh.exec(move |ctx| {
                      let _ = reply_tx.send(gpu_reply(&ctx, id, label.as_deref()));
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
                let rect = query_rect(&json);
                let scale = query_scale(&json);
                let raw = query_raw(&json);
                let exec = queries.exec.lock().expect("exec handle lock poisoned").clone();
                match exec {
                  Some(eh) => {
                    let reply_tx = queries.outbound_tx.clone();
                    eh.exec(move |ctx| {
                      let _ = reply_tx.send(texture_reply(&ctx, id, texture_id, rect, scale, raw));
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
                // Self-identifying: mixed-version fleets are normal, and the
                // usual cause of an unknown kind is a client runtime older
                // than the dev tool calling it.
                let kind = other.unwrap_or("<none>");
                let msg = format!(
                  "Unknown query kind \"{kind}\": this client (runtime {}) does not support it - the client likely predates the tool",
                  crate::VERSION
                );
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
  set_mute(&flags, false);
  log::warn!("[sgo] Connection to ws://{addr} lost");
  true
}

/// Round to two decimals for the JSON payloads: raw f32s serialize with float
/// noise (0.1 -> 0.10000000149...) that only bloats the wire format.
// One sampler binding in the /gpu inventory: the bare source id, or
// `{ id, filter?, wrap? }` when the binding carries a sampling override.
fn binding_json(b: &alloy::TextureBinding) -> (String, serde_json::Value) {
  let value = if b.sampler.is_empty() {
    serde_json::json!(b.id)
  } else {
    let mut o = serde_json::Map::new();
    o.insert("id".into(), serde_json::json!(b.id));
    if let Some(f) = b.sampler.filter {
      o.insert("filter".into(), serde_json::json!(f.name()));
    }
    if let Some(w) = b.sampler.wrap {
      o.insert("wrap".into(), serde_json::json!(w.name()));
    }
    serde_json::Value::Object(o)
  };
  (b.name.clone(), value)
}

fn round2(v: f32) -> f64 {
  (v as f64 * 100.0).round() / 100.0
}

fn error_reply(id: u64, message: &str) -> String {
  serde_json::json!({"type": "result", "id": id, "error": message}).to_string()
}

/// Reserved pointer id for injected pointer events: far outside anything SDL
/// hands out, so a synthetic pointer never aliases a live one in the router
/// or the runner's input state.
const SYNTHETIC_POINTER_ID: u64 = 1 << 60;
/// Per-event cap on `delayMs`/`holdMs` and whole-sequence duration cap for
/// `input` queries, bounding how long a sequence task can run. The dev server
/// sizes its query timeout from the same request, so these two never race.
const INPUT_DELAY_MAX_MS: u64 = 5000;
const INPUT_TOTAL_MAX_MS: u64 = 30_000;

/// Parse an `input` query's `events` array into a flat send plan of
/// (delay-before-send ms, event). A `tap` expands to down + up with its
/// `holdMs` as the up's delay. Everything is validated upfront: any invalid
/// event rejects the whole sequence before a single event is sent.
pub(crate) fn parse_input_events(events: Option<&serde_json::Value>) -> Result<Vec<(u64, alloy::AlloyEvent)>, String> {
  use alloy::{AlloyEvent, Modifiers, PointerType};
  let arr = events.and_then(|e| e.as_array()).ok_or("events must be an array")?;
  if arr.is_empty() {
    return Err("events must not be empty".into());
  }
  let mut out = Vec::new();
  let mut total: u64 = 0;
  for (i, ev) in arr.iter().enumerate() {
    let field_ms = |name: &str| -> Result<u64, String> {
      match ev.get(name) {
        None => Ok(0),
        Some(v) => match v.as_u64() {
          Some(ms) if ms <= INPUT_DELAY_MAX_MS => Ok(ms),
          _ => Err(format!("events[{i}]: {name} must be an integer 0..={INPUT_DELAY_MAX_MS}")),
        },
      }
    };
    let str_field = |name: &str| ev.get(name).and_then(|v| v.as_str());
    let num_field = |name: &str| -> Result<f32, String> {
      ev.get(name)
        .and_then(|v| v.as_f64())
        .filter(|v| v.is_finite())
        .map(|v| v as f32)
        .ok_or_else(|| format!("events[{i}]: {name} must be a finite number"))
    };
    let flag = |name: &str| ev.get(name).and_then(|v| v.as_bool()).unwrap_or(false);
    let modifiers = Modifiers { shift: flag("shift"), ctrl: flag("ctrl"), alt: flag("alt"), meta: flag("meta") };
    let delay = field_ms("delayMs")?;
    let hold = field_ms("holdMs")?;
    let ty = str_field("type").ok_or_else(|| format!("events[{i}]: missing type"))?;
    let action = str_field("action");
    if ev.get("holdMs").is_some() && action != Some("tap") {
      return Err(format!("events[{i}]: holdMs only applies to action \"tap\""));
    }
    match ty {
      "key" => {
        let key = str_field("key")
          .filter(|k| !k.is_empty())
          .ok_or_else(|| format!("events[{i}]: key events need a non-empty key name"))?;
        let make = |down: bool| AlloyEvent::Key {
          down,
          key: key.to_string(),
          code: alloy::w3c_code_for_key(key),
          modifiers,
          repeat: false,
        };
        match action {
          Some("down") => out.push((delay, make(true))),
          Some("up") => out.push((delay, make(false))),
          Some("tap") => {
            out.push((delay, make(true)));
            out.push((hold, make(false)));
          }
          _ => return Err(format!("events[{i}]: key action must be down, up or tap")),
        }
      }
      "pointer" => {
        let x = num_field("x")?;
        let y = num_field("y")?;
        let pointer_type = match str_field("pointerType") {
          None | Some("mouse") => PointerType::Mouse,
          Some("touch") => PointerType::Touch,
          Some(_) => return Err(format!("events[{i}]: pointerType must be mouse or touch")),
        };
        let button = match ev.get("button") {
          None => 0u8,
          Some(v) => match v.as_u64() {
            Some(b) if b <= 4 => b as u8,
            _ => return Err(format!("events[{i}]: button must be an integer 0..=4")),
          },
        };
        let down =
          || AlloyEvent::PointerDown { pointer_id: SYNTHETIC_POINTER_ID, pointer_type, button, x, y, modifiers };
        let up = || AlloyEvent::PointerUp { pointer_id: SYNTHETIC_POINTER_ID, pointer_type, button, x, y, modifiers };
        // No hardware delta for synthetic moves: movement derives from the
        // position diff, the honest synthetic delta.
        let mv = || AlloyEvent::PointerMove { pointer_id: SYNTHETIC_POINTER_ID, pointer_type, x, y, rel: None, modifiers };
        match action {
          Some("move") => out.push((delay, mv())),
          Some("down") => out.push((delay, down())),
          Some("up") => out.push((delay, up())),
          Some("tap") => {
            out.push((delay, down()));
            out.push((hold, up()));
          }
          _ => return Err(format!("events[{i}]: pointer action must be down, up, move or tap")),
        }
      }
      "wheel" => {
        let x = num_field("x")?;
        let y = num_field("y")?;
        let delta_x = num_field("deltaX")?;
        let delta_y = num_field("deltaY")?;
        out.push((
          delay,
          AlloyEvent::Wheel {
            pointer_id: SYNTHETIC_POINTER_ID,
            pointer_type: PointerType::Mouse,
            x,
            y,
            delta_x,
            delta_y,
            modifiers,
          },
        ));
      }
      "text" => {
        let text = str_field("text")
          .filter(|t| !t.is_empty())
          .ok_or_else(|| format!("events[{i}]: text events need a non-empty text"))?;
        out.push((delay, AlloyEvent::TextInput { text: text.to_string() }));
      }
      _ => return Err(format!("events[{i}]: type must be key, pointer, wheel or text")),
    }
    total += delay + hold;
    if total > INPUT_TOTAL_MAX_MS {
      return Err(format!("Sequence too long: delays and holds total over {INPUT_TOTAL_MAX_MS} ms"));
    }
  }
  Ok(out)
}

/// Default window the stats summary covers when the query names none.
const STATS_WINDOW_DEFAULT_MS: f64 = 5000.0;

/// Everything a stats reply is built from. `snap` is the draw loop's latched
/// figures; `clock` the client's own clock at query time (`timeMs` on its
/// monotonic origin, the latest present index) so two samples can be
/// differenced; `window` the frame-history summary (None when no frame was
/// rebuilt inside the window; the reply then says so with `frames: 0`);
/// `counts` (mounted, total) from the live tree when the query could run on
/// the JS thread - the reply then carries mountedNodes and orphanNodes
/// (total - mounted: nodes unreachable from the root, i.e. leaked or
/// intentionally kept detached); `raster` the live raster counters. Without an
/// engine the last two are simply absent.
struct StatsReply<'a> {
  snap: crate::stats::StatsSnapshot,
  time_ms: f64,
  window: Option<&'a crate::frame_history::WindowSummary>,
  counts: Option<(usize, usize)>,
  raster: Option<alloy::RasterCounters>,
}

fn round2_64(v: f64) -> f64 {
  (v * 100.0).round() / 100.0
}

fn stats_reply(id: u64, r: StatsReply<'_>) -> String {
  let s = r.snap;
  let mut data = serde_json::Map::new();
  let mut put = |k: &str, v: serde_json::Value| {
    data.insert(k.into(), v);
  };
  put("timeMs", round2_64(r.time_ms).into());
  put("frame", s.frame.into());
  put("fps", s.fps.into());
  put("cpuPct", round2(s.cpu_pct).into());
  put("memBytes", s.mem_bytes.into());
  put("jsMs", round2(s.js_ms).into());
  put("frameMs", round2(s.frame_ms).into());
  put("setPropsPerFrame", round2(s.set_count).into());
  put("layoutMs", round2(s.layout_ms).into());
  put("postLayoutMs", round2(s.post_ms).into());
  put("paintMs", round2(s.paint_ms).into());
  put("hoverMs", round2(s.hover_ms).into());
  put("reusedPerSec", s.reused.into());
  put("skippedPerSec", s.skipped.into());
  put("textures", s.textures.into());
  put("nodes", s.node_count.into());
  put("measureCalls", s.measure_calls.into());
  put("paraShapes", s.para_shapes.into());
  put("wordHits", s.word_hits.into());
  put("dirtiedNodes", s.dirtied.into());
  put("cacheGets", s.cache_gets.into());
  put("cacheHits", s.cache_hits.into());
  put("nodesPainted", s.paint.nodes_painted.into());
  put("window", window_json(r.window, r.time_ms));
  if let Some((mounted, total)) = r.counts {
    put("mountedNodes", mounted.into());
    put("orphanNodes", total.saturating_sub(mounted).into());
  }
  if let Some(rc) = r.raster {
    put("rasterQueue", rc.queue.into());
    put("idleTicks", rc.idle_ticks.into());
    put("fenceTimeouts", rc.fence_timeouts.into());
    put("gpuPasses", rc.passes.into());
    // Integer ms: sub-ms increments accumulate in the microsecond counters
    // before this division, so the cumulative rounding loss stays under 1ms.
    put("gpuPassIssueMs", (rc.pass_issue_micros / 1000).into());
    // Absent (not 0) when the context has no timer queries.
    if let Some(exec) = rc.pass_exec_micros {
      put("gpuPassExecMs", (exec / 1000).into());
    }
    if let Some(exec) = rc.frame_exec_micros {
      put("gpuFrameExecMs", (exec / 1000).into());
    }
    put("rasterCmdMs", (rc.cmd_micros / 1000).into());
  }
  serde_json::json!({"type": "result", "id": id, "data": data}).to_string()
}

/// The window block: percentiles of the JS-thread critical path over the
/// rebuilt frames in the window, the count over the refresh period, and the
/// worst frame with its phase breakdown and layout activity - the frame the
/// smoothed figures average away. Raster rates ride along when the window
/// spans two or more frames. `now_ms` is the query instant the worst frame's
/// age is measured from.
fn window_json(window: Option<&crate::frame_history::WindowSummary>, now_ms: f64) -> serde_json::Value {
  let Some(w) = window else {
    return serde_json::json!({ "windowMs": STATS_WINDOW_DEFAULT_MS, "frames": 0 });
  };
  let worst = &w.worst;
  let mut data = serde_json::Map::new();
  let mut put = |k: &str, v: serde_json::Value| {
    data.insert(k.into(), v);
  };
  put("windowMs", w.window_ms.into());
  put("frames", w.frames.into());
  put("p50Ms", round2(w.p50_ms).into());
  put("p95Ms", round2(w.p95_ms).into());
  put("maxMs", round2(w.max_ms).into());
  put("slowFrames", w.slow_frames.into());
  put("periodMs", round2(worst.period_ms).into());
  put(
    "worst",
    serde_json::json!({
      "ageMs": round2_64(now_ms - worst.at_ms),
      "frame": worst.frame,
      "totalMs": round2(worst.total_ms),
      "jsMs": round2(worst.js_ms),
      "layoutMs": round2(worst.layout_ms),
      "postLayoutMs": round2(worst.post_ms),
      "paintMs": round2(worst.paint_ms),
      "hoverMs": round2(worst.hover_ms),
      "measureCalls": worst.counters.measure_calls,
      "paraShapes": worst.counters.para_shapes,
      "wordHits": worst.counters.word_hits,
      "dirtiedNodes": worst.counters.dirtied,
      "cacheGets": worst.counters.cache_gets,
      "cacheHits": worst.counters.cache_hits,
      "nodesPainted": worst.nodes_painted,
    }),
  );
  if let Some(r) = &w.raster_rates {
    put("fenceTimeoutsPerSec", round2(r.fence_timeouts_per_sec).into());
    put("gpuPassesPerFrame", round2(r.passes_per_frame).into());
    put("gpuPassIssueMsPerFrame", round2(r.pass_issue_ms_per_frame).into());
    if let Some(exec) = r.pass_exec_ms_per_frame {
      put("gpuPassExecMsPerFrame", round2(exec).into());
    }
    if let Some(exec) = r.frame_exec_ms_per_frame {
      put("gpuFrameExecMsPerFrame", round2(exec).into());
    }
    put("rasterCmdMsPerSec", round2(r.cmd_ms_per_sec).into());
  }
  data.into()
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
  props: bool,
) -> String {
  let Some(tree) = ctx.userdata::<flux::gui::tree::SharedRenderTree>() else {
    return error_reply(id, "no render tree");
  };
  let tree = tree.0.borrow();
  let props_tree = props.then_some(&*tree);
  if let Some(needle) = search {
    return match tree.snapshot_matches(root, needle, TREE_MATCH_LIMIT) {
      Some(matches) => {
        let entries: Vec<_> = matches
          .iter()
          .map(|m| {
            let mut obj = node_json(&m.node, props_tree);
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
    Some(node) => serde_json::json!({"type": "result", "id": id, "data": node_json(&node, props_tree)}).to_string(),
    None => match root {
      Some(r) => error_reply(id, &format!("no node with id {r}")),
      None => error_reply(id, "no render tree (the app has not rendered)"),
    },
  }
}

/// The optional crop rect of a snapshot/texture query message.
fn query_rect(json: &serde_json::Value) -> Option<(u32, u32, u32, u32)> {
  let r = json.get("rect")?;
  Some((
    r.get("x")?.as_u64()? as u32,
    r.get("y")?.as_u64()? as u32,
    r.get("width")?.as_u64()? as u32,
    r.get("height")?.as_u64()? as u32,
  ))
}

/// The optional integer magnification of a snapshot/texture query message.
fn query_scale(json: &serde_json::Value) -> u32 {
  json.get("scale").and_then(|s| s.as_u64()).unwrap_or(1) as u32
}

/// Whether a snapshot/texture query wants the pixels as they are
/// (`format: "raw"`: base64 RGBA8, rows top-down) instead of a PNG.
fn query_raw(json: &serde_json::Value) -> bool {
  json.get("format").and_then(|f| f.as_str()) == Some("raw")
}

/// Cap on a scaled capture's side length: magnification multiplies the PNG
/// encode input by scale^2, and that encode runs inline on the JS thread.
const CAPTURE_OUT_MAX: u32 = 8192;

/// Apply an optional crop rect and integer nearest-neighbour magnification to
/// an RGBA8 buffer: crop first, then duplicate each pixel into a scale x scale
/// block. The full readback is already in hand, so doing both CPU-side keeps
/// the GL path identical to the plain case.
fn crop_scale_rgba(
  pixels: Vec<u8>,
  width: u32,
  height: u32,
  rect: Option<(u32, u32, u32, u32)>,
  scale: u32,
) -> Result<(Vec<u8>, u32, u32), String> {
  if scale < 1 || scale > 8 {
    return Err(format!("scale {scale} outside 1-8"));
  }
  let (pixels, width, height) = match rect {
    None => (pixels, width, height),
    Some((x, y, w, h)) => {
      if w == 0 || h == 0 || x.saturating_add(w) > width || y.saturating_add(h) > height {
        return Err(format!("rect {w}x{h} at {x},{y} outside the {width}x{height} source"));
      }
      let mut cropped = Vec::with_capacity((w as usize) * (h as usize) * 4);
      for row in y..y + h {
        let start = ((row as usize) * (width as usize) + x as usize) * 4;
        cropped.extend_from_slice(&pixels[start..start + (w as usize) * 4]);
      }
      (cropped, w, h)
    }
  };
  if scale == 1 {
    return Ok((pixels, width, height));
  }
  let (out_w, out_h) = (width * scale, height * scale);
  if out_w > CAPTURE_OUT_MAX || out_h > CAPTURE_OUT_MAX {
    return Err(format!(
      "scaled output {out_w}x{out_h} exceeds {CAPTURE_OUT_MAX} px per side; crop tighter or lower the scale"
    ));
  }
  let mut scaled = Vec::with_capacity((out_w as usize) * (out_h as usize) * 4);
  let mut line = Vec::with_capacity((out_w as usize) * 4);
  for row in 0..height as usize {
    line.clear();
    let row_start = row * (width as usize) * 4;
    for px in 0..width as usize {
      let p = &pixels[row_start + px * 4..row_start + px * 4 + 4];
      for _ in 0..scale {
        line.extend_from_slice(p);
      }
    }
    for _ in 0..scale {
      scaled.extend_from_slice(&line);
    }
  }
  Ok((scaled, out_w, out_h))
}

/// Queue a snapshot capture of `node_id` on the alloy context (reached from JS
/// userdata, like `tree_reply` reaches the render tree). The completion callback
/// runs on this same JS thread during the paint pass that services the capture:
/// it crops/scales and PNG-encodes the captured pixels and routes the reply out.
/// Runs on the JS thread via the engine exec handle.
fn request_snapshot(
  ctx: &flux::rquickjs::Ctx<'_>,
  node_id: u64,
  id: u64,
  rect: Option<(u32, u32, u32, u32)>,
  scale: u32,
  raw: bool,
  reply_tx: UnboundedSender<String>,
  frame_requested: Arc<AtomicBool>,
) {
  let Some(atx) = ctx.userdata::<flux::gui::AlloyContext>() else {
    let _ = reply_tx.send(error_reply(id, "no alloy context"));
    return;
  };
  let alloy = atx.0.clone();
  alloy.request_capture(
    node_id,
    Box::new(move |result| {
      let reply = match result {
        Ok(info) => match crop_scale_rgba(info.pixels, info.width, info.height, rect, scale) {
          Ok((pixels, width, height)) => snapshot_reply(id, width, height, pixels, raw),
          Err(e) => error_reply(id, &e),
        },
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

/// Encode a captured RGBA8 buffer as a base64 PNG reply, or as the base64
/// bytes themselves when `raw` (for pixel assertions without a decoder). On-
/// demand and rare (a dev-server query), so encoding inline on the JS thread
/// is fine.
fn snapshot_reply(id: u64, width: u32, height: u32, rgba: Vec<u8>, raw: bool) -> String {
  let data = if raw {
    let b64 = base64::engine::general_purpose::STANDARD.encode(&rgba);
    serde_json::json!({ "rgbaBase64": b64, "width": width, "height": height })
  } else {
    let png = match forge::image::encode_png(&rgba, width, height) {
      Ok(png) => png,
      Err(e) => return error_reply(id, &format!("png encode failed: {e}")),
    };
    let b64 = base64::engine::general_purpose::STANDARD.encode(&png);
    serde_json::json!({ "pngBase64": b64, "width": width, "height": height })
  };
  serde_json::json!({
    "type": "result",
    "id": id,
    "data": data,
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
/// `label`, when given, keeps only the resources created with exactly that
/// debug label (every list; the window shader has none and is dropped).
fn gpu_reply(ctx: &flux::rquickjs::Ctx<'_>, id: u64, label: Option<&str>) -> String {
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
        // rate); issueMs is raster-thread occupancy, execMs is GPU-side
        // duration from timer queries (0 on a context without them).
        "passes": p.passes,
        "issueMs": p.pass_issue_micros / 1000,
        "execMs": p.pass_exec_micros / 1000,
        "textures": p.textures.iter().map(binding_json).collect::<serde_json::Map<_, _>>(),
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
      match p.instance_buffer_ids.as_slice() {
        [] => {}
        [id] => {
          map.insert("instanceBuffer".into(), (*id).into());
        }
        ids => {
          map.insert("instanceBuffers".into(), ids.to_vec().into());
        }
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
      if p.samples > 1 {
        map.insert("samples".into(), p.samples.into());
      }
      // A sub-target: where it renders (its parent) and its rectangle
      // there, top-left origin like the texture leaf's srcX/srcY.
      if let Some(r) = &p.region {
        map.insert("into".into(), r.parent.into());
        map.insert("x".into(), r.x.into());
        map.insert("y".into(), r.y.into());
        map.insert("width".into(), r.width.into());
        map.insert("height".into(), r.height.into());
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
        // The buffer slot is reported only off its default, like the draw
        // state fields: absent means slot 0.
        let attrs: Vec<serde_json::Value> = p
          .instance_attributes
          .iter()
          .map(|(name, format, slot)| {
            if *slot == 0 {
              serde_json::json!({"name": name, "format": format})
            } else {
              serde_json::json!({"name": name, "format": format, "slot": slot})
            }
          })
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
              "textures": d.textures.iter().map(binding_json).collect::<serde_json::Map<_, _>>(),
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
            match d.instance_buffer_ids.as_slice() {
              [] => {}
              [id] => {
                map.insert("instanceBuffer".into(), (*id).into());
              }
              ids => {
                map.insert("instanceBuffers".into(), ids.to_vec().into());
              }
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
        // The buffer slot is reported only off its default, like the draw
        // state fields: absent means slot 0.
        let attrs: Vec<serde_json::Value> = p
          .instance_attributes
          .iter()
          .map(|(name, format, slot)| {
            if *slot == 0 {
              serde_json::json!({"name": name, "format": format})
            } else {
              serde_json::json!({"name": name, "format": format, "slot": slot})
            }
          })
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

  let keep = |list: Vec<serde_json::Value>| -> Vec<serde_json::Value> {
    match label {
      None => list,
      Some(label) => list.into_iter().filter(|obj| obj.get("label").and_then(|l| l.as_str()) == Some(label)).collect(),
    }
  };
  let mut data = serde_json::json!({
    "textures": keep(textures), "buffers": keep(buffers), "pipelines": keep(pipelines),
    "renderPipelines": keep(render_pipelines), "programs": keep(programs),
  });
  if let (Some(ws), None) = (&res.window_shader, label) {
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

/// Read back a registered texture's pixels (optionally cropped to `rect` and
/// magnified by `scale`) and encode them as a PNG reply. Runs on the JS thread.
fn texture_reply(
  ctx: &flux::rquickjs::Ctx<'_>,
  id: u64,
  texture_id: u64,
  rect: Option<(u32, u32, u32, u32)>,
  scale: u32,
  raw: bool,
) -> String {
  let Some(atx) = ctx.userdata::<flux::gui::AlloyContext>() else {
    return error_reply(id, "no alloy context");
  };
  match atx.0.read_texture_by_id(texture_id) {
    Err(e) => error_reply(id, &e),
    Ok((width, height, pixels)) => match crop_scale_rgba(pixels, width, height, rect, scale) {
      Ok((pixels, width, height)) => snapshot_reply(id, width, height, pixels, raw),
      Err(e) => error_reply(id, &e),
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
      // An async command returns a Promise, which would stringify as {} and
      // read as a silent success. The reply is encoded right here - nothing
      // awaits it - so reject loudly instead.
      if value.as_promise().is_some() {
        return error_reply(
          id,
          &format!("debug command '{name}' returned a Promise; commands must return synchronously (not be async)"),
        );
      }
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

/// `props`, when set, is the live tree: each node additionally carries its
/// current off-default property values (JSX names, see flux read_jsx) and,
/// when a transform anywhere on its ancestor chain moved it off its
/// axis-aligned box, the painted `quad` (four corners, window coordinates,
/// [x0,y0, x1,y1, x2,y2, x3,y3] from pre-transform top-left clockwise). The
/// box x/y/width/height stay the quad's axis-aligned bounds either way.
fn node_json(node: &alloy::rendertree::NodeSnapshot, props: Option<&alloy::rendertree::RenderTree>) -> serde_json::Value {
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
  if let Some(tree) = props {
    if let Some(element) = tree.try_node(node.id) {
      let values = flux::gui::read_jsx(element);
      if !values.is_empty() {
        let mut props_map = serde_json::Map::with_capacity(values.len());
        for (name, value) in values {
          props_map.insert(name.into(), read_value_json(value));
        }
        map.insert("props".into(), props_map.into());
      }
    }
    if let Some(quad) = tree.painted_quad(node.id) {
      if !quad_is_aabb(&quad) {
        let flat: Vec<serde_json::Value> =
          quad.iter().flat_map(|p| [round2(p.x).into(), round2(p.y).into()]).collect();
        map.insert("quad".into(), flat.into());
      }
    }
  }
  if !node.children.is_empty() {
    map.insert("children".into(), node.children.iter().map(|c| node_json(c, props)).collect::<Vec<_>>().into());
  }
  // A depth cap cut this node's children off: surface how many exist so a
  // reader knows to descend with root=<id>.
  if node.children.len() < node.child_count {
    map.insert("childCount".into(), node.child_count.into());
  }
  obj
}

/// True when the painted quad is still the axis-aligned box the snapshot
/// already reports (top-left, top-right, bottom-right, bottom-left of its own
/// AABB) - the untransformed common case, where emitting it would be noise.
fn quad_is_aabb(quad: &[alloy::rendertree::Point; 4]) -> bool {
  const EPS: f32 = 0.01;
  let eq = |a: f32, b: f32| (a - b).abs() < EPS;
  let (min_x, max_x) = (quad.iter().map(|p| p.x).fold(f32::MAX, f32::min), quad.iter().map(|p| p.x).fold(f32::MIN, f32::max));
  let (min_y, max_y) = (quad.iter().map(|p| p.y).fold(f32::MAX, f32::min), quad.iter().map(|p| p.y).fold(f32::MIN, f32::max));
  eq(quad[0].x, min_x)
    && eq(quad[0].y, min_y)
    && eq(quad[1].x, max_x)
    && eq(quad[1].y, min_y)
    && eq(quad[2].x, max_x)
    && eq(quad[2].y, max_y)
    && eq(quad[3].x, min_x)
    && eq(quad[3].y, max_y)
}

fn read_value_json(value: flux::gui::ReadValue) -> serde_json::Value {
  match value {
    flux::gui::ReadValue::Num(n) => round2(n as f32).into(),
    flux::gui::ReadValue::Int(n) => n.into(),
    flux::gui::ReadValue::Bool(b) => b.into(),
    flux::gui::ReadValue::Str(s) => s.into(),
    flux::gui::ReadValue::Nums(list) => list.into_iter().map(|n| round2(n as f32)).collect::<Vec<_>>().into(),
  }
}
