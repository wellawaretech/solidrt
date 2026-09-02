// Loopback forwarder for the dev server's p2p tunnel: a dial-by-ticket
// transport for the existing ws://host:port dev protocol. A local TCP listener
// stands in for the server; each accepted connection dials the ticket and
// opens one bi-stream carrying that connection's bytes verbatim (the server
// side pumps them into its own loopback serve port - see
// packages/cli/server/tunnel.ts). The e2e encryption runs endpoint-to-endpoint;
// both loopback hops are same-machine.

use std::net::SocketAddr;

use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// The tunnel's ALPN; must match the server (packages/cli/server/tunnel.ts).
const PROTOCOL: &str = "solidrt-dev/0";

/// The running forwarder. Dropping it stops accepting and closes the endpoint;
/// in-flight pumps end when their streams close.
pub struct Tunnel {
  task: tokio::task::JoinHandle<()>,
  endpoint: forge::p2p::Endpoint,
}

impl Drop for Tunnel {
  fn drop(&mut self) {
    self.task.abort();
    let endpoint = self.endpoint.clone();
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
      handle.spawn(async move { endpoint.close().await });
    }
  }
}

/// The client's persisted p2p identity: 64 hex chars in
/// <client_dir>/identity/p2p.key. Loaded for every tunnel bind so the dev
/// server sees one stable node id for this client across restarts; absent or
/// unreadable falls back to a fresh key (persisted after the bind).
fn load_secret() -> Option<[u8; 32]> {
  let store = crate::storage::get()?;
  let text = std::fs::read_to_string(store.identity_dir().join("p2p.key")).ok()?;
  decode_hex(text.trim())
}

pub(crate) fn decode_hex(hex: &str) -> Option<[u8; 32]> {
  if hex.len() != 64 || !hex.is_ascii() {
    return None;
  }
  let mut out = [0u8; 32];
  for (i, byte) in out.iter_mut().enumerate() {
    *byte = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).ok()?;
  }
  Some(out)
}

fn persist_secret(hex: &str) {
  let Some(store) = crate::storage::get() else { return };
  let path = store.identity_dir().join("p2p.key");
  if let Err(e) = std::fs::write(&path, hex) {
    log::warn!("[sgo] Could not persist p2p key: {e}");
    return;
  }
  // A private key: owner-only where the OS supports it.
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
  }
}

/// Bind the endpoint and the loopback listener. Returns the address to dial in
/// place of the dev server, plus the forwarder guard.
pub async fn start(ticket: String) -> Result<(SocketAddr, Tunnel), String> {
  // Dial-only and local: the client never accepts, needs no relay of its own,
  // and must not publish addresses. The ticket carries the server's. An
  // ephemeral bind port is fine here - the dialer is never itself dialed.
  // The secret is the client's stable identity (see load_secret).
  let secret = load_secret();
  let endpoint = forge::p2p::Endpoint::bind(secret, None, Vec::new(), true, None).await?;
  if secret.is_none() {
    persist_secret(&endpoint.secret_key_hex());
  }
  let listener = TcpListener::bind("127.0.0.1:0").await.map_err(|e| format!("bind loopback: {e}"))?;
  let addr = listener.local_addr().map_err(|e| format!("loopback addr: {e}"))?;
  let task = tokio::spawn(accept_loop(listener, endpoint.clone(), ticket));
  Ok((addr, Tunnel { task, endpoint }))
}

async fn accept_loop(listener: TcpListener, endpoint: forge::p2p::Endpoint, ticket: String) {
  loop {
    let (tcp, _) = match listener.accept().await {
      Ok(conn) => conn,
      Err(e) => {
        log::warn!("[sgo] Tunnel accept failed: {e}");
        return;
      }
    };
    let endpoint = endpoint.clone();
    let ticket = ticket.clone();
    tokio::spawn(async move {
      // One QUIC connection per TCP connection: the server's accept loop takes
      // each incoming connection's first bi-stream. The io handle holds the
      // connection, so it lives exactly as long as the pump.
      match endpoint.connect_io(ticket, PROTOCOL.to_string()).await {
        Ok(io) => pump(tcp, io).await,
        Err(e) => log::warn!("[sgo] Tunnel dial failed: {e}"),
      }
    });
  }
}

/// Copy bytes both ways until each direction reaches end-of-stream, shutting
/// down the opposite write half so closes propagate.
async fn pump(tcp: TcpStream, io: impl AsyncRead + AsyncWrite + Unpin) {
  let (mut recv, mut send) = tokio::io::split(io);
  let (mut tcp_read, mut tcp_write) = tcp.into_split();
  let up = async {
    let _ = tokio::io::copy(&mut tcp_read, &mut send).await;
    let _ = send.shutdown().await;
  };
  let down = async {
    let _ = tokio::io::copy(&mut recv, &mut tcp_write).await;
    let _ = tcp_write.shutdown().await;
  };
  tokio::join!(up, down);
}
