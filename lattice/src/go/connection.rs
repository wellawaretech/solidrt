use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::OnceCell;

pub type DevServerCell = Arc<OnceCell<String>>;

const DEV_SERVER_PORT: u16 = 15194;

#[cfg(not(target_os = "android"))]
const SERVICE_TYPE: &str = "_solidrt._tcp.local.";

pub fn start(
  handle: &tokio::runtime::Handle,
  tx: UnboundedSender<crate::EngineCmd>,
  dev_server: DevServerCell,
  proxy_files_enabled: Arc<AtomicBool>,
  proxy_http_enabled: Arc<AtomicBool>,
) {
  handle.spawn(async move {
    // Android reaches the host dev server through the adb-reverse loopback that
    // `srt run` / dev-android sets up (`adb reverse tcp:DEV_PORT`). This covers
    // the emulator (behind NAT) and USB-tethered devices.
    #[cfg(target_os = "android")]
    loopback_loop(tx, dev_server, proxy_files_enabled, proxy_http_enabled).await;

    // Other platforms discover the dev server on the LAN via mDNS / DNS-SD.
    #[cfg(not(target_os = "android"))]
    discovery_loop(tx, dev_server, proxy_files_enabled, proxy_http_enabled).await;
  });
}

#[cfg(target_os = "android")]
async fn loopback_loop(
  tx: UnboundedSender<crate::EngineCmd>,
  dev_server: DevServerCell,
  proxy_files_enabled: Arc<AtomicBool>,
  proxy_http_enabled: Arc<AtomicBool>,
) {
  let addr = format!("127.0.0.1:{DEV_SERVER_PORT}");
  loop {
    try_serve(&addr, &tx, &dev_server, &proxy_files_enabled, &proxy_http_enabled).await;
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
  }
}

/// Browse for the dev server via mDNS, then connect and serve. Once a server is
/// resolved we keep reconnecting to that address (so dev-server restarts on the
/// same host reconnect without waiting for a fresh announcement); only after
/// repeated connect failures do we go back to browsing, in case it moved.
#[cfg(not(target_os = "android"))]
async fn discovery_loop(
  tx: UnboundedSender<crate::EngineCmd>,
  dev_server: DevServerCell,
  proxy_files_enabled: Arc<AtomicBool>,
  proxy_http_enabled: Arc<AtomicBool>,
) {
  use mdns_sd::ServiceDaemon;

  let mdns = match ServiceDaemon::new() {
    Ok(d) => d,
    Err(e) => {
      log::error!("[sgo] mDNS init failed: {e}");
      return;
    }
  };
  let receiver = match mdns.browse(SERVICE_TYPE) {
    Ok(r) => r,
    Err(e) => {
      log::error!("[sgo] mDNS browse failed: {e}");
      return;
    }
  };
  log::info!("[sgo] Browsing for {SERVICE_TYPE} via mDNS...");

  const MAX_FAILURES: u32 = 5;
  let mut addr: Option<String> = None;
  let mut failures = 0u32;

  loop {
    // Block for the next resolved service whenever we have no address to try.
    if addr.is_none() {
      addr = recv_resolved(&receiver).await;
      failures = 0;
      if addr.is_none() {
        // Receiver closed; nothing more will arrive.
        return;
      }
    }

    if let Some(server) = addr.clone() {
      if try_serve(&server, &tx, &dev_server, &proxy_files_enabled, &proxy_http_enabled).await {
        // Was connected, then dropped: retry the same address.
        failures = 0;
      } else {
        failures += 1;
        if failures >= MAX_FAILURES {
          log::info!("[sgo] {server} unreachable; re-discovering");
          addr = None;
        }
      }
      tokio::time::sleep(std::time::Duration::from_secs(3)).await;
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
  dev_server: &DevServerCell,
  proxy_files_enabled: &Arc<AtomicBool>,
  proxy_http_enabled: &Arc<AtomicBool>,
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

  // Publish the dev server address so the next engine build can install the
  // file/dir proxy. set() returns Err if already set; ignore.
  let _ = dev_server.set(addr.to_string());

  let version = option_env!("SOLIDRT_VERSION").unwrap_or("0.0.0-dev");
  let info = format!(r#"{{"type":"info","platform":"{}","version":"{version}"}}"#, std::env::consts::OS,);
  let _ = client.send(tokio_websockets::Message::text(info)).await;

  while let Some(Ok(msg)) = client.next().await {
    if let Some(text) = msg.as_text() {
      if let Ok(json) = serde_json::from_str::<serde_json::Value>(text) {
        match json.get("type").and_then(|t| t.as_str()) {
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