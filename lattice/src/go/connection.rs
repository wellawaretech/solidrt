use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::OnceCell;

pub type DevServerCell = Arc<OnceCell<String>>;

const DEV_SERVER_PORT: u16 = 15194;

pub fn start(
  handle: &tokio::runtime::Handle,
  tx: UnboundedSender<crate::EngineCmd>,
  dev_server: DevServerCell,
  proxy_files_enabled: Arc<AtomicBool>,
  proxy_http_enabled: Arc<AtomicBool>,
) {
  handle.spawn(async move {
    loop {
      let mut connected = false;

      // Android prefers the adb-reverse loopback: `srt run` / dev-android sets up
      // `adb reverse tcp:DEV_PORT` so the device reaches the host dev server at
      // 127.0.0.1. This covers the emulator (behind NAT) and USB-tethered devices.
      #[cfg(target_os = "android")]
      {
        let addr = format!("127.0.0.1:{DEV_SERVER_PORT}");
        connected = try_serve(&addr, &tx, &dev_server, &proxy_files_enabled, &proxy_http_enabled).await;
      }

      // Standard flow: when no loopback is set up (e.g. a device on the LAN), fall
      // back to LAN discovery. This is where QR-scan and recent connections will
      // slot in later, and where raw UDP broadcast will become mDNS.
      if !connected {
        if let Some(addr) = discover_udp().await {
          connected = try_serve(&addr, &tx, &dev_server, &proxy_files_enabled, &proxy_http_enabled).await;
        }
      }

      let _ = connected;
      // Back off before the next attempt, whether the connection dropped or
      // nothing was found, to avoid hammering.
      tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    }
  });
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

/// One round of LAN discovery: broadcast a probe and wait briefly for a dev
/// server to answer. Returns its address, or None if none replied in time.
async fn discover_udp() -> Option<String> {
  use tokio::net::UdpSocket;

  let sock = match UdpSocket::bind("0.0.0.0:0").await {
    Ok(s) => s,
    Err(e) => {
      log::error!("[sgo] UDP bind failed: {e}");
      return None;
    }
  };
  if let Err(e) = sock.set_broadcast(true) {
    log::error!("[sgo] UDP set_broadcast failed: {e}");
    return None;
  }

  let dest = format!("255.255.255.255:{DEV_SERVER_PORT}");
  if let Err(e) = sock.send_to(b"SRT_DISCOVER", &dest).await {
    log::warn!("[sgo] UDP send failed: {e}");
    return None;
  }

  let mut buf = [0u8; 64];
  match tokio::time::timeout(std::time::Duration::from_secs(2), sock.recv_from(&mut buf)).await {
    Ok(Ok((len, addr))) => {
      let msg = std::str::from_utf8(&buf[..len]).unwrap_or("");
      if msg == "SRT_SERVER" {
        let server_addr = format!("{}:{DEV_SERVER_PORT}", addr.ip());
        log::info!("[sgo] Discovered dev server at {server_addr}");
        return Some(server_addr);
      }
      None
    }
    Ok(Err(e)) => {
      log::warn!("[sgo] UDP recv error: {e}");
      None
    }
    Err(_) => {
      log::debug!("[sgo] No dev server found");
      None
    }
  }
}