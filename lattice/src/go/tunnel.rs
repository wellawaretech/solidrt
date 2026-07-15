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

/// Bind the endpoint and the loopback listener. Returns the address to dial in
/// place of the dev server, plus the forwarder guard.
pub async fn start(ticket: String) -> Result<(SocketAddr, Tunnel), String> {
  // Dial-only and local: the client never accepts, needs no relay of its own,
  // and must not publish addresses. The ticket carries the server's. An
  // ephemeral bind port is fine here - the dialer is never itself dialed.
  let endpoint = forge::p2p::Endpoint::bind(None, None, Vec::new(), true, None).await?;
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
      // each incoming connection's first bi-stream.
      match endpoint.connect(ticket, PROTOCOL.to_string()).await {
        // The connection handle must outlive the pump: dropping it closes the
        // stream.
        Ok((_conn, send, recv)) => pump(tcp, send, recv).await,
        Err(e) => log::warn!("[sgo] Tunnel dial failed: {e}"),
      }
    });
  }
}

/// Copy bytes both ways until each direction reaches end-of-stream, shutting
/// down the opposite write half so closes propagate.
async fn pump(tcp: TcpStream, mut send: impl AsyncWrite + Unpin, mut recv: impl AsyncRead + Unpin) {
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
