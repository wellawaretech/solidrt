use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::OnceCell;

pub type DevServerCell = Arc<OnceCell<String>>;

pub fn start(
  handle: &tokio::runtime::Handle,
  tx: UnboundedSender<crate::EngineCmd>,
  dev_server: DevServerCell,
  proxy_files_enabled: Arc<AtomicBool>,
  proxy_http_enabled: Arc<AtomicBool>,
) {
  handle.spawn(async move { spawn_go_udp_discovery(tx, dev_server, proxy_files_enabled, proxy_http_enabled).await });
}

async fn spawn_go_ws(
  dev_server_addr: String,
  tx: UnboundedSender<crate::EngineCmd>,
  dev_server: DevServerCell,
  proxy_files_enabled: Arc<AtomicBool>,
  proxy_http_enabled: Arc<AtomicBool>,
) {
  use futures_util::{SinkExt, StreamExt};

  log::info!("[sgo] Connecting to ws://{}...", dev_server_addr);

  let uri = http::Uri::builder()
    .scheme("ws")
    .authority(dev_server_addr.as_str())
    .path_and_query("/")
    .build()
    .expect("invalid dev server URI");

  loop {
    let (mut client, _) = loop {
      match tokio_websockets::ClientBuilder::from_uri(uri.clone()).connect().await {
        Ok(conn) => break conn,
        Err(e) => {
          log::warn!("[sgo] Connection failed: {e}, retrying in 3s...");
          tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        }
      }
    };

    log::info!("[sgo] Connected to ws://{dev_server_addr}");

    // Publish the dev server address so the next engine build can install
    // the file/dir proxy. set() returns Err if already set; ignore.
    let _ = dev_server.set(dev_server_addr.clone());

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

    log::warn!("[sgo] Connection lost, reconnecting in 3s...");
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
  }
}

const DEV_SERVER_PORT: u16 = 15194;

async fn spawn_go_udp_discovery(
  tx: UnboundedSender<crate::EngineCmd>,
  dev_server: DevServerCell,
  proxy_files_enabled: Arc<AtomicBool>,
  proxy_http_enabled: Arc<AtomicBool>,
) {
  use tokio::net::UdpSocket;

  log::info!("[sgo] Starting UDP discovery on port {DEV_SERVER_PORT}...");

  let sock = match UdpSocket::bind("0.0.0.0:0").await {
    Ok(s) => s,
    Err(e) => {
      log::error!("[sgo] UDP bind failed: {e}");
      return;
    }
  };
  if let Err(e) = sock.set_broadcast(true) {
    log::error!("[sgo] UDP set_broadcast failed: {e}");
    return;
  }

  let mut buf = [0u8; 64];
  loop {
    let dest = format!("255.255.255.255:{DEV_SERVER_PORT}");
    if let Err(e) = sock.send_to(b"SRT_DISCOVER", &dest).await {
      log::warn!("[sgo] UDP send failed: {e}");
    }

    match tokio::time::timeout(std::time::Duration::from_secs(2), sock.recv_from(&mut buf)).await {
      Ok(Ok((len, addr))) => {
        let msg = std::str::from_utf8(&buf[..len]).unwrap_or("");
        if msg == "SRT_SERVER" {
          let server_addr = format!("{}:{DEV_SERVER_PORT}", addr.ip());
          log::info!("[sgo] Discovered dev server at {server_addr}");
          spawn_go_ws(server_addr, tx, dev_server, proxy_files_enabled, proxy_http_enabled).await;
          return;
        }
      }
      Ok(Err(e)) => log::warn!("[sgo] UDP recv error: {e}"),
      Err(_) => log::debug!("[sgo] No dev server found, retrying..."),
    }

    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
  }
}
