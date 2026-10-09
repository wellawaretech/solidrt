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
  /// True while a pushed version installs (verify, write, precompile its
  /// wasm). Drawn as the overlay badge, with a frame requested on both edges,
  /// so a multi-second install shows as such instead of as a hang.
  pub installing: Arc<AtomicBool>,
  /// The process's wasm engine, the one every app engine is built with, so an
  /// install precompiles a version's modules into exactly the cache slots
  /// those engines read. None when the host has no usable engine.
  pub wasm: Option<forge::wasm::WasmEngine>,
  /// Dev-tool pause/step/scale state, applied by the frame verb (see
  /// runtime::ClockControl); written from `clock` queries, reset on
  /// reload/stop so no app starts under a stale pause.
  pub clock: crate::runtime::ClockControl,
  /// Synthetic-input sender for `input` queries: downs/ups pushed here enter
  /// the UI thread's batch loop through the same channel as real SDL input
  /// (hit testing, input-state bookkeeping, focus). Moves follow the
  /// producer-side resampler rule instead (see `resampler`).
  pub input_tx: crate::runtime::EventSender,
  /// The alloy loop's command channel, for the `input` query's synthetic
  /// gamepads (AlloyCommand::Gamepad): pads live with SDL's on that
  /// thread, not in the UI thread's event channel.
  pub alloy_cmd_tx: std::sync::mpsc::Sender<alloy::AlloyCommand>,
  /// Resampler feed for injected pointer events, mirroring the alloy pump:
  /// moves are consumed into it (never sent as events), a down seeds the
  /// history and an up flushes it before the event travels (see alloy's
  /// resample.rs).
  pub resampler: alloy::resample::SharedResampler,
  /// The alloy run loop's user-input mute (App::user_input_mute), set by
  /// the dev tools while an agent measures or tests: the server's `mute`
  /// message (and `welcome`, for a client joining while muted) flips it; a
  /// lost connection clears it, so a dead server never leaves the user
  /// locked out. It survives reload: a mute spans the agent's rebuilds.
  pub user_input_muted: Arc<AtomicBool>,
}

/// Apply the server's mute state to the user-input mute, logging the
/// transition so the human at the client can tell why input stopped, and
/// requesting a frame so the overlay badge shows it on an idle app.
fn set_mute(flags: &DevFlags, active: bool) {
  if flags.user_input_muted.swap(active, Ordering::Relaxed) != active {
    flags.frame_requested.store(true, Ordering::Relaxed);
    if active {
      log::info!("[sgo] User input muted by the dev tools until unmuted");
    } else {
      log::info!("[sgo] User input unmuted");
    }
  }
}

/// Clears what a live session set, the moment the session ends: the
/// connected flag and the mute must drop with the connection however it
/// ends - lost (try_serve's loop breaks) or cancelled by a dev command (the
/// caller's select drops the try_serve future mid-await, so no code after
/// its loop runs). The frame request draws the change on an idle app (the
/// overlay badge).
struct SessionGuard<'a>(&'a DevFlags);

impl Drop for SessionGuard<'_> {
  fn drop(&mut self) {
    self.0.connected.store(false, Ordering::Relaxed);
    self.0.frame_requested.store(true, Ordering::Relaxed);
    set_mute(self.0, false);
  }
}

/// Send-safe handles the connection answers dev-server queries from, without a
/// round trip through the UI thread: the stats snapshot the draw loop
/// publishes, the live engine's exec handle (refreshed on each engine build),
/// a sender on the outbound channel for replies produced on the JS thread,
/// and the location the app reports (plugins::dev::LocationSlot).
#[derive(Clone)]
pub struct QueryHandles {
  pub stats: Arc<Mutex<crate::stats::StatsSnapshot>>,
  pub history: Arc<Mutex<crate::frame_history::FrameHistory>>,
  pub exec: Arc<Mutex<Option<flux::ExecHandle>>>,
  pub outbound_tx: UnboundedSender<String>,
  pub location: crate::plugins::dev::LocationSlot,
  /// The capabilities the runtime's own modules add beyond flux's (see
  /// `Modules`), reported beside them.
  pub modules: Vec<&'static str>,
}

/// Query kinds this runtime answers, advertised in the connect-time `info`
/// message so dev tools can plan against a client's actual surface before
/// calling (mixed-version fleets are normal). Keep in sync with the query
/// match in `try_serve`.
const QUERY_KINDS: &[&str] = &[
  "clock",
  "input",
  "settle",
  "stats",
  "tree",
  "snapshot",
  "gpu",
  "texture",
  "buffer",
  "debug_list",
  "debug_call",
  "link",
  "location",
];

#[cfg(not(target_os = "android"))]
const SERVICE_TYPE: &str = "_solidrt._tcp.local.";

/// Commands the JS `sol.dev` surface sends into the supervisor. The
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
/// rather than treated as an interrupt: the player re-dials its launch
/// address on every mount (including the mount a dev push causes), and
/// tearing down the live connection to redial it would make the server
/// re-deliver its latched push to the fresh connection, reloading the
/// player into another dial - a reload/reconnect loop that never settles.
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
async fn install_push(
  addr: &str,
  manifest: &str,
  code: &str,
  wasm: Option<&forge::wasm::WasmEngine>,
) -> Result<String, String> {
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
  super::store::install(manifest, code, &fetched, wasm)
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
  // Drawn as the overlay badge (see lattice's overlay::Badge): request a
  // frame so an idle app shows the edge. The guard undoes both when this
  // session ends, by any path.
  flags.frame_requested.store(true, Ordering::Relaxed);
  let _session = SessionGuard(flags);

  // Publish the dialed dev server address so the next engine build installs the
  // file/dir proxy against the server we are actually talking to. Overwrites any
  // previous address so reconnecting to a different server repoints the proxy.
  *dev_server.lock().expect("dev_server lock poisoned") = Some(addr.to_string());

  // What this client already knows about itself and never changes, told
  // once so a dev tool can tell clients apart and see what machine each one
  // is: its storage tree (<data-root>/client<N>, or the player/packed
  // folder; null without writable storage), pid and executable, the host
  // and OS, the SDL video driver, the display refresh rate as of the
  // connect and the GPU strings. The GPU strings come from the raster
  // thread's context; a connect that wins that race sends null, which a
  // reconnect corrects.
  let capabilities: Vec<&str> = flux::capabilities().into_iter().chain(queries.modules.iter().copied()).collect();
  let info = serde_json::json!({
    "type": "info",
    "platform": flux::platform(),
    "version": crate::VERSION,
    "profile": crate::PROFILE,
    "capabilities": capabilities,
    "queries": QUERY_KINDS,
    "clientDir": crate::storage::get().map(|store| store.client_dir.to_string_lossy().into_owned()),
    "pid": std::process::id(),
    "execPath": forge::process::exec_path(),
    "host": forge::process::host_name(),
    "os": forge::process::os_description(),
    "kernel": forge::process::kernel_version(),
    "videoDriver": alloy::video_driver(),
    "refreshRate": alloy::refresh_rate(),
    "gpu": alloy::gpu_info().map(|gpu| serde_json::json!({
      "vendor": gpu.vendor,
      "renderer": gpu.renderer,
      "version": gpu.version,
      // The device ceilings, spelled like flux:gpu's `limits` export - what
      // this client's creates validate against, so a dev tool can see why a
      // size or a rig fits on one connected client and not another.
      "limits": {
        "maxTextureSize": gpu.limits.max_texture_size,
        "maxCubeMapSize": gpu.limits.max_cube_map_size,
        "maxTextureUnits": gpu.limits.max_texture_units,
        "maxVertexAttribs": gpu.limits.max_vertex_attribs,
        "maxAnisotropy": gpu.limits.max_anisotropy,
        "maxVertexUniformVectors": gpu.limits.max_vertex_uniform_vectors,
      },
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
              // the app appears in the player's list and launches offline.
              // The reload itself applies the in-memory code either way - a
              // failed install degrades to an ephemeral push.
              let mut app_id = None;
              if let Some(manifest) = json.get("manifest").and_then(|m| m.as_str()) {
                flags.installing.store(true, Ordering::Relaxed);
                flags.frame_requested.store(true, Ordering::Relaxed);
                match install_push(addr, manifest, code, flags.wasm.as_ref()).await {
                  Ok(id) => app_id = Some(id),
                  Err(e) => log::warn!("[sgo] Version install failed: {e}"),
                }
                flags.installing.store(false, Ordering::Relaxed);
                flags.frame_requested.store(true, Ordering::Relaxed);
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
            // The player must never come up paused.
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
                let steps = json.get("step").and_then(|s| s.as_u64()).unwrap_or(0);
                if steps > 0 {
                  flags.clock.add_steps(steps);
                }
                flags.frame_requested.store(true, Ordering::Relaxed);
                // Stepped frames run one per frame signal, so the reply
                // waits for the queue to drain and a read right after it
                // sees the stepped state; bounded, so a client that stops
                // running frames (a wedged app) still answers, with the
                // steps it never ran as `pendingSteps`. On its own task so
                // the wait never blocks this loop.
                let clock = flags.clock.clone();
                let reply_tx = queries.outbound_tx.clone();
                tokio::spawn(async move {
                  if steps > 0 {
                    let deadline = tokio::time::Instant::now()
                      + Duration::from_millis(CLOCK_STEP_WAIT_BASE_MS + CLOCK_STEP_WAIT_MS * steps);
                    while clock.pending_steps() > 0 && tokio::time::Instant::now() < deadline {
                      tokio::time::sleep(Duration::from_millis(CLOCK_STEP_POLL_MS)).await;
                    }
                  }
                  let reply = serde_json::json!({
                    "type": "result",
                    "id": id,
                    "data": { "scale": clock.scale(), "pendingSteps": clock.pending_steps() },
                  })
                  .to_string();
                  let _ = reply_tx.send(reply);
                });
              }
              Some("input") => {
                // Synthetic input: parsed events enter the same channel real
                // SDL input feeds (see DevFlags::input_tx), deliberately with
                // no frame-request latch - like real input, the app's own
                // handlers request whatever frames their reactions need. Timed
                // sequences run on their own task so a hold or delay never
                // blocks this loop; the reply follows the last event so a
                // caller knows the gesture has fully entered the pipeline.
                match crate::input_plan::plan(json.get("events"), CONTROL_FRAME_MS) {
                  Ok(seq) => {
                    let delivered = seq.len();
                    let input_tx = flags.input_tx.clone();
                    let alloy_cmd_tx = flags.alloy_cmd_tx.clone();
                    let resampler = flags.resampler.clone();
                    let reply_tx = queries.outbound_tx.clone();
                    tokio::spawn(async move {
                      for crate::input_plan::Step { wait, inject } in seq {
                        // A step that needs a frame ahead of it gets a
                        // frame interval: there is no frame to wait on here.
                        let wait_ms = (wait.ms as f64).max(wait.frames as f64 * CONTROL_FRAME_MS);
                        if wait_ms > 0.0 {
                          tokio::time::sleep(Duration::from_secs_f64(wait_ms / 1000.0)).await;
                        }
                        match inject {
                          crate::input_plan::Injected::Event(event) => {
                            // Producer-side resampler feed, mirroring the alloy
                            // pump (see DevFlags::resampler): moves are consumed
                            // here and dispatch from the frame verb's samples.
                            // A synthetic event happens when it is sent.
                            let sent = resampler.feed(event, std::time::Instant::now(), |event, at| input_tx.send_at(event, at));
                            if sent.is_err() {
                              // The runtime is shutting down; nobody left to reply to.
                              return;
                            }
                          }
                          crate::input_plan::Injected::Gamepad(cmd) => {
                            if alloy_cmd_tx.send(alloy::AlloyCommand::Gamepad(cmd)).is_err() {
                              return;
                            }
                          }
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
              Some("settle") => {
                // Wait for the app to come to rest (settle.rs) and say
                // whether it did: the condition an agent waits on in place
                // of a sleep. The wait runs as a task of the engine, on the
                // display's own frames, bounded by `maxMs` of wall time;
                // what is left when the cap passes comes back in the reply.
                let max_ms = json.get("maxMs").and_then(|m| m.as_f64()).unwrap_or(crate::settle::DEFAULT_MAX_MS);
                let exec = queries.exec.lock().expect("exec handle lock poisoned").clone();
                match exec {
                  Some(eh) if max_ms.is_finite() && max_ms >= 0.0 => {
                    let reply_tx = queries.outbound_tx.clone();
                    eh.exec(move |ctx| {
                      let task_ctx = ctx.clone();
                      ctx.spawn(async move {
                        let started = std::time::Instant::now();
                        let cap = crate::settle::Cap::Wall(Duration::from_secs_f64(max_ms / 1000.0));
                        let left = crate::settle::settle(&task_ctx, cap, None).await.err();
                        let in_flight: serde_json::Map<String, serde_json::Value> = left
                          .iter()
                          .flat_map(|left| left.in_flight.iter())
                          .map(|(kind, count)| (kind.to_string(), serde_json::json!(count)))
                          .collect();
                        let reply = serde_json::json!({
                          "type": "result",
                          "id": id,
                          "data": {
                            "settled": left.is_none(),
                            "waitedMs": started.elapsed().as_secs_f64() * 1000.0,
                            "inFlight": in_flight,
                            "demand": left.as_ref().map(|left| left.demand.clone()).unwrap_or_default(),
                            "timerDue": left.as_ref().is_some_and(|left| left.timer_due),
                            "idleDue": left.as_ref().is_some_and(|left| left.idle_due),
                          },
                        })
                        .to_string();
                        let _ = reply_tx.send(reply);
                      });
                    });
                  }
                  Some(_) => {
                    let e = "settle: maxMs must be a non-negative number of milliseconds";
                    let _ = client.send(tokio_websockets::Message::text(error_reply(id, e))).await;
                  }
                  None => {
                    let _ = client.send(tokio_websockets::Message::text(error_reply(id, "no running engine"))).await;
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
                let ask = match json.get("windowFrames").and_then(|w| w.as_u64()) {
                  Some(n) => crate::frame_history::Window::Frames(n as usize),
                  None => crate::frame_history::Window::Ms(
                    json.get("windowMs").and_then(|w| w.as_f64()).unwrap_or(STATS_WINDOW_DEFAULT_MS),
                  ),
                }
                .clamped();
                let now_ms = crate::frame_history::now_ms();
                let window = {
                  let history = queries.history.lock().expect("frame history lock poisoned");
                  let window = history.summarize(ask, now_ms);
                  // An empty window over a ring that holds frames is the
                  // reading a tablet gave right after an animation
                  // (okf/backlog/stats-window-frames-zero.md): say what the
                  // ring held against the ask, so the next such reading
                  // names the gap instead of the symptom.
                  if window.is_none() {
                    if let Some((count, oldest_ms, newest_ms)) = history.reach(now_ms) {
                      log::debug!(
                        "[stats] window {ask:?} at {now_ms:.0} ms found no frames; the ring holds {count} (oldest {oldest_ms:.0} ms ago, newest {newest_ms:.0} ms ago)"
                      );
                    }
                  }
                  window
                };
                let exec = queries.exec.lock().expect("exec handle lock poisoned").clone();
                match exec {
                  Some(eh) => {
                    let reply_tx = queries.outbound_tx.clone();
                    eh.exec(move |ctx| {
                      let counts = flux::gui::tree::node_counts(&ctx);
                      // Read live, not from the frame-latched snapshot: a
                      // backlogged raster thread produces no frames, so the
                      // latch goes stale exactly when these matter.
                      let raster = flux::gui::alloy_context(&ctx).map(|atx| atx.raster_counters());
                      let reply = StatsReply { snap, time_ms: now_ms, ask, window: window.as_ref(), counts, raster };
                      let _ = reply_tx.send(stats_reply(id, reply));
                    });
                  }
                  None => {
                    let reply =
                      StatsReply { snap, time_ms: now_ms, ask, window: window.as_ref(), counts: None, raster: None };
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
                // `at`: the nodes a pointer at that window point reaches,
                // instead of a snapshot or a search.
                let coordinate = |name: &str| json.get("at").and_then(|at| at.get(name)).and_then(|v| v.as_f64());
                let at = coordinate("x").zip(coordinate("y")).map(|(x, y)| (x as f32, y as f32));
                let exec = queries.exec.lock().expect("exec handle lock poisoned").clone();
                match exec {
                  Some(eh) => {
                    let reply_tx = queries.outbound_tx.clone();
                    eh.exec(move |ctx| {
                      let _ = reply_tx.send(tree_reply(&ctx, id, root, depth, search.as_deref(), at, props));
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
                let step = json.get("step").and_then(|s| s.as_bool()).unwrap_or(false);
                let exec = queries.exec.lock().expect("exec handle lock poisoned").clone();
                match exec {
                  Some(eh) => {
                    let reply_tx = queries.outbound_tx.clone();
                    let frame_requested = flags.frame_requested.clone();
                    let clock = flags.clock.clone();
                    eh.exec(move |ctx| request_snapshot(&ctx, node_id, id, rect, scale, raw, step, clock, reply_tx, frame_requested));
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
                    let draw = json.get("draw").and_then(|d| d.as_u64());
                    eh.exec(move |ctx| {
                      let _ = reply_tx.send(gpu_reply(&ctx, id, label.as_deref(), draw));
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
              Some("link") => {
                // Deliver a link to the app exactly as the OS path does: the
                // raw string on the `link` bus event, nothing interpreted.
                // The reply says whether anything listened, so a caller
                // learns that an app without a handler dropped it.
                let link = json.get("link").and_then(|l| l.as_str()).unwrap_or("").to_string();
                let exec = queries.exec.lock().expect("exec handle lock poisoned").clone();
                match exec {
                  Some(eh) => {
                    let reply_tx = queries.outbound_tx.clone();
                    eh.exec(move |ctx| {
                      let _ = reply_tx.send(link_reply(&ctx, id, link));
                    });
                  }
                  None => {
                    let _ = client.send(tokio_websockets::Message::text(error_reply(id, "no running engine"))).await;
                  }
                }
              }
              Some("location") => {
                // The location the app reported through reportLocation
                // (sol:dev), a plain value shared with the engine loop, so no
                // JS-thread round trip; null when the app reported none.
                let reply = serde_json::json!({"type": "result", "id": id, "data": {"location": queries.location.get()}})
                  .to_string();
                let _ = client.send(tokio_websockets::Message::text(reply)).await;
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

/// The frame interval an input plan is run at from the control API (see
/// input_plan.rs): one refresh of a 60 Hz display, the rate apps are built
/// for. A faster display only gets its moves resampled.
const CONTROL_FRAME_MS: f64 = 1000.0 / 60.0;

/// Default window the stats summary covers when the query names none.
const STATS_WINDOW_DEFAULT_MS: f64 = 5000.0;

// The clock query's wait for its stepped frames to run: one per frame at
// the client's frame rate, so this long per step on top of the base, and
// the reply then reports whatever is still pending. Re-read at the poll
// interval.
const CLOCK_STEP_WAIT_MS: u64 = 50;
const CLOCK_STEP_WAIT_BASE_MS: u64 = 1000;
const CLOCK_STEP_POLL_MS: u64 = 4;

/// Everything a stats reply is built from. `snap` is the draw loop's latched
/// figures; `clock` the client's own clock at query time (`timeMs` on its
/// monotonic origin, the latest present index) so two samples can be
/// differenced; `ask` the (clamped) window the query asked for and
/// `window` the frame-history summary over it (None when no frame changed
/// the picture inside it; the reply then says so with `frames: 0`);
/// `counts` (mounted, total) from the live tree when the query could run on
/// the JS thread - the reply then carries mountedNodes and orphanNodes
/// (total - mounted: nodes unreachable from the root, i.e. leaked or
/// intentionally kept detached); `raster` the live raster counters. Without an
/// engine the last two are simply absent.
struct StatsReply<'a> {
  snap: crate::stats::StatsSnapshot,
  time_ms: f64,
  ask: crate::frame_history::Window,
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
  put("wordShapes", s.word_shapes.into());
  put("wordHits", s.word_hits.into());
  put("dirtiedNodes", s.dirtied.into());
  put("cacheGets", s.cache_gets.into());
  put("cacheHits", s.cache_hits.into());
  put("nodesPainted", s.paint.nodes_painted.into());
  put("backdropsPrepainted", s.paint.backdrops_prepainted.into());
  put("damagePx", (s.paint.damage_px.round() as i64).into());
  put("paintOps", paint_ops_json(&s.counters));
  put("window", window_json(r.window, r.time_ms, r.ask));
  if let Some((mounted, total)) = r.counts {
    put("mountedNodes", mounted.into());
    put("orphanNodes", total.saturating_sub(mounted).into());
  }
  if let Some(rc) = r.raster {
    put("rasterQueue", rc.queue.into());
    put("idleTicks", rc.idle_ticks.into());
    put("partialPresents", rc.partial_presents.into());
    put("videoLatched", rc.video_latched.into());
    put("videoSkipped", rc.video_skipped.into());
    put("videoLateLatches", rc.video_late.into());
    put("fenceTimeouts", rc.fence_timeouts.into());
    put("missedPresents", rc.missed_presents.into());
    put("cadenceHold", rc.cadence_hold.into());
    put("frameWorkMeanMs", round2(rc.work_mean_ms).into());
    put("frameWorkMaxMs", round2(rc.work_max_ms).into());
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
/// age is measured from; `ask` the window asked for, echoed even when no
/// frame fell inside it (the summary then has no window of its own).
fn window_json(
  window: Option<&crate::frame_history::WindowSummary>,
  now_ms: f64,
  ask: crate::frame_history::Window,
) -> serde_json::Value {
  let Some(w) = window else {
    return match ask {
      crate::frame_history::Window::Ms(ms) => serde_json::json!({ "windowMs": ms, "frames": 0 }),
      crate::frame_history::Window::Frames(n) => serde_json::json!({ "windowFrames": n, "frames": 0 }),
    };
  };
  let worst = &w.worst;
  let mut data = serde_json::Map::new();
  let mut put = |k: &str, v: serde_json::Value| {
    data.insert(k.into(), v);
  };
  put("windowMs", round2_64(w.window_ms).into());
  if let Some(n) = w.window_frames {
    put("windowFrames", n.into());
  }
  put("frames", w.frames.into());
  put("p50Ms", round2(w.p50_ms).into());
  put("p95Ms", round2(w.p95_ms).into());
  put("maxMs", round2(w.max_ms).into());
  put("slowFrames", w.slow_frames.into());
  put("captureFrames", w.capture_frames.into());
  put("periodMs", round2(worst.period_ms).into());
  put("backdropsPrepainted", w.backdrops_prepainted.into());
  put("textLayers", w.text_layers.into());
  put("nodesPaintedMax", w.nodes_painted_max.into());
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
      "wordShapes": worst.counters.word_shapes,
      "wordHits": worst.counters.word_hits,
      "dirtiedNodes": worst.counters.dirtied,
      "cacheGets": worst.counters.cache_gets,
      "cacheHits": worst.counters.cache_hits,
      "nodesPainted": worst.nodes_painted,
      "backdropsPrepainted": worst.backdrops_prepainted,
      "paintOps": paint_ops_json(&worst.counters),
    }),
  );
  if let Some(r) = &w.raster_rates {
    put("missedPresents", r.missed_presents.into());
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
  if !w.target_rates.is_empty() {
    let targets: Vec<serde_json::Value> = w
      .target_rates
      .iter()
      .map(|t| {
        let mut obj = serde_json::json!({
          "id": t.id,
          "passesPerFrame": round2(t.passes_per_frame),
          "gpuPassIssueMsPerFrame": round2(t.issue_ms_per_frame),
          "verticesPerFrame": round2(t.vertices_per_frame),
        });
        flux::gui::inspect::insert_label(&mut obj, &t.label);
        if let Some(exec) = t.exec_ms_per_frame {
          obj
            .as_object_mut()
            .expect("target json is an object")
            .insert("gpuPassExecMsPerFrame".into(), round2(exec).into());
        }
        obj
      })
      .collect();
    put("targets", targets.into());
  }
  data.into()
}

/// The display-list ops one rebuild's paint walk recorded (see
/// alloy::rendertree::counters), the per-frame counts that name a paint
/// cost on a tiled GPU without a reload per hypothesis.
fn paint_ops_json(c: &alloy::rendertree::counters::LayoutCounters) -> serde_json::Value {
  serde_json::json!({
    "draws": c.draws,
    "textLayers": c.text_layers,
    "clips": c.clips,
    "roundedClips": c.rounded_clips,
    "saveLayers": c.save_layers,
    "blends": c.blends,
    "gradients": c.gradients,
  })
}

// Search results are for locating nodes, not dumping the app: enough for a
// "which node is X" question, small enough to never rebuild the 3MB-tree
// problem the query option exists to avoid.
const TREE_MATCH_LIMIT: usize = 100;

/// Snapshot the render tree and encode it. With
/// `search`, reply with the matching nodes (id paths included) instead of a
/// subtree; with `at`, with the nodes a pointer at that point reaches, root
/// first (`hit`, empty when nothing is hit). Runs on the JS thread (see the
/// query handling above).
fn tree_reply(
  ctx: &flux::rquickjs::Ctx<'_>,
  id: u64,
  root: Option<u64>,
  depth: Option<usize>,
  search: Option<&str>,
  at: Option<(f32, f32)>,
  props: bool,
) -> String {
  let reply = flux::gui::tree::with_tree(ctx, |tree| {
    let props_tree = props.then_some(tree);
    if let Some((x, y)) = at {
      let hit: Vec<_> = tree
        .hit_path(alloy::rendertree::Point::new(x, y))
        .into_iter()
        .filter_map(|node| tree.snapshot_from(Some(node), Some(0)))
        .map(|node| flux::gui::inspect::node_record(&node, props_tree))
        .collect();
      return serde_json::json!({"type": "result", "id": id, "data": {"hit": hit}}).to_string();
    }
    if let Some(needle) = search {
      return match tree.snapshot_matches(root, needle, TREE_MATCH_LIMIT) {
        Some(matches) => {
          let entries: Vec<_> = matches
            .iter()
            .map(|m| {
              let mut obj = flux::gui::inspect::node_record(&m.node, props_tree);
              let map = obj.as_object_mut().expect("a node record is an object");
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
      Some(node) => {
        serde_json::json!({"type": "result", "id": id, "data": flux::gui::inspect::node_record(&node, props_tree)})
          .to_string()
      }
      None => match root {
        Some(r) => error_reply(id, &format!("no node with id {r}")),
        None => error_reply(id, "no render tree (the app has not rendered)"),
      },
    }
  });
  reply.unwrap_or_else(|| error_reply(id, "no render tree"))
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
  step: bool,
  clock: crate::runtime::ClockControl,
  reply_tx: UnboundedSender<String>,
  frame_requested: Arc<AtomicBool>,
) {
  let Some(alloy) = flux::gui::alloy_context(ctx) else {
    let _ = reply_tx.send(error_reply(id, "no alloy context"));
    return;
  };
  // A stepped capture is the picture of one frame as its own code drew
  // it: the capture is queued first, then one step, both on the JS thread
  // between two frame signals, so the stepped frame's paint services the
  // capture - before anything that frame left to a microtask lands. A
  // plain capture is serviced by a paint it requests itself, which comes
  // after the request's own microtask checkpoint and so never shows a
  // glitch confined to one frame. Only a paused clock steps: with it
  // running the next frame flows anyway, and a queued step would fire at
  // the next pause instead.
  if step && clock.scale() != 0.0 {
    let _ = reply_tx.send(error_reply(id, "snapshot step: the clock is running; pause it first (clock scale 0)"));
    return;
  }
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
  if step {
    clock.add_steps(1);
  }
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
    // The capture is premultiplied like every target; PNG stores straight.
    let png = match forge::image::encode_png(&rgba, width, height, true) {
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

/// The GPU inventory (flux's `gpu_record`, which a test reads too) as a
/// query reply. Runs on the JS thread (see the query handling above).
fn gpu_reply(ctx: &flux::rquickjs::Ctx<'_>, id: u64, label: Option<&str>, draw: Option<u64>) -> String {
  match flux::gui::inspect::gpu_record(ctx, label, draw) {
    Ok(data) => serde_json::json!({"type": "result", "id": id, "data": data}).to_string(),
    Err(e) => error_reply(id, &e),
  }
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
  let Some(atx) = flux::gui::alloy_context(ctx) else {
    return error_reply(id, "no alloy context");
  };
  match atx.read_texture_by_id(texture_id) {
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
  let Some(atx) = flux::gui::alloy_context(ctx) else {
    return error_reply(id, "no alloy context");
  };
  let elem_size = match fmt {
    "f32" => 4,
    "u16" => 2,
    "u8" => 1,
    _ => return error_reply(id, &format!("unsupported as '{fmt}' (expected f32|u16|u8)")),
  };
  let total = match atx.gpu_buffer_len(buffer_id) {
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
  match atx.read_gpu_buffer(buffer_id, byte_offset, len) {
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

/// Emit a link on the app's event bus, as the runner does for an OS-routed
/// one. Runs on the JS thread.
fn link_reply(ctx: &flux::rquickjs::Ctx<'_>, id: u64, link: String) -> String {
  let delivered = flux::has_listeners(ctx, "link");
  let obj = flux::rquickjs::Object::new(ctx.clone()).expect("create object");
  obj.set("link", link).expect("set link");
  flux::emit_event(ctx, "link", obj);
  serde_json::json!({"type": "result", "id": id, "data": {"delivered": delivered}}).to_string()
}

/// List the app's registered debug commands. Runs on the JS thread.
fn debug_list_reply(ctx: &flux::rquickjs::Ctx<'_>, id: u64) -> String {
  let commands = match ctx.userdata::<crate::plugins::dev::DebugRegistry>() {
    Some(registry) => registry.names(),
    // The registry is installed on first `sol:dev` import; an app that never
    // imported it simply has no commands.
    None => Vec::new(),
  };
  serde_json::json!({"type": "result", "id": id, "data": {"commands": commands}}).to_string()
}

/// Call a registered debug command (dev.rs `call_debug`, which a test calls
/// too) and encode the outcome as a query reply. Runs on the JS thread.
fn debug_call_reply(ctx: &flux::rquickjs::Ctx<'_>, id: u64, name: &str, args: Option<serde_json::Value>) -> String {
  match crate::plugins::dev::call_debug(ctx, name, args) {
    Ok(value) => serde_json::json!({"type": "result", "id": id, "data": {"value": value}}).to_string(),
    Err(e) => error_reply(id, &e),
  }
}
