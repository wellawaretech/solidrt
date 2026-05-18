use alloy::log;
use tokio::sync::mpsc::UnboundedSender;

pub fn start(handle: &tokio::runtime::Handle, tx: UnboundedSender<crate::EngineCmd>) {
  handle.spawn(async move { spawn_go_udp_discovery(tx).await });
}

async fn spawn_go_ws(dev_server: String, tx: UnboundedSender<crate::EngineCmd>) {
  use futures_util::{SinkExt, StreamExt};

  log!("[sgo] Connecting to ws://{}...", dev_server);

  let uri = http::Uri::builder()
    .scheme("ws")
    .authority(dev_server.as_str())
    .path_and_query("/")
    .build()
    .expect("invalid dev server URI");

  loop {
    let (mut client, _) = loop {
      match tokio_websockets::ClientBuilder::from_uri(uri.clone())
        .connect()
        .await
      {
        Ok(conn) => break conn,
        Err(e) => {
          log!("[sgo] Connection failed: {e}, retrying in 3s...");
          tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        }
      }
    };

    log!("[sgo] Connected to ws://{dev_server}");

    let version = option_env!("SOLIDRT_VERSION").unwrap_or("0.0.0-dev");
    let info = format!(
      r#"{{"type":"info","platform":"{}","version":"{version}"}}"#,
      std::env::consts::OS,
    );
    let _ = client.send(tokio_websockets::Message::text(info)).await;

    while let Some(Ok(msg)) = client.next().await {
      if let Some(text) = msg.as_text() {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(text) {
          match json.get("type").and_then(|t| t.as_str()) {
            Some("reload") => {
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

    log!("[sgo] Connection lost, reconnecting in 3s...");
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
  }
}

const DEV_SERVER_PORT: u16 = 15194;

async fn spawn_go_udp_discovery(tx: UnboundedSender<crate::EngineCmd>) {
  use tokio::net::UdpSocket;

  log!("[sgo] Starting UDP discovery on port {DEV_SERVER_PORT}...");

  let sock = match UdpSocket::bind("0.0.0.0:0").await {
    Ok(s) => s,
    Err(e) => {
      log!("[sgo] UDP bind failed: {e}");
      return;
    }
  };
  if let Err(e) = sock.set_broadcast(true) {
    log!("[sgo] UDP set_broadcast failed: {e}");
    return;
  }

  let mut buf = [0u8; 64];
  loop {
    let dest = format!("255.255.255.255:{DEV_SERVER_PORT}");
    if let Err(e) = sock.send_to(b"SRT_DISCOVER", &dest).await {
      log!("[sgo] UDP send failed: {e}");
    }

    match tokio::time::timeout(
      std::time::Duration::from_secs(2),
      sock.recv_from(&mut buf),
    )
    .await
    {
      Ok(Ok((len, addr))) => {
        let msg = std::str::from_utf8(&buf[..len]).unwrap_or("");
        if msg == "SRT_SERVER" {
          let server_addr = format!("{}:{DEV_SERVER_PORT}", addr.ip());
          log!("[sgo] Discovered dev server at {server_addr}");
          spawn_go_ws(server_addr, tx).await;
          return;
        }
      }
      Ok(Err(e)) => log!("[sgo] UDP recv error: {e}"),
      Err(_) => log!("[sgo] No dev server found, retrying..."),
    }

    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
  }
}
