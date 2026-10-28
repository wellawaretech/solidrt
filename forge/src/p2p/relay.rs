//! A self-hosted iroh relay served from this process: the one relay an
//! `Endpoint::bind` may name as its `relay_url`. Two peers that have no
//! direct UDP path to each other (or whose direct path fails) exchange
//! their packets through it, each over a TCP connection to the relay, and
//! iroh keeps probing for a direct path meanwhile. Plain HTTP (the relay
//! protocol upgrades the connection to a websocket; iroh exchanges relay
//! URLs with the http scheme and dials ws), which is what a relay on a
//! private network wants: no certificate, no domain. Everything the relay
//! carries is end-to-end encrypted QUIC, so the relay reads nothing. It
//! admits only endpoints that present its token, a fresh random one per
//! relay, carried as the user part of the URL it hands out, so a port that
//! happens to be reachable from further away is not an open relay. Never a
//! public relay: the app hosts this one.

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;

use iroh_base::SecretKey;
use iroh_relay::server::{Access, AccessControl, ClientRequest, RelayConfig, Server, ServerConfig};
use tokio::sync::Mutex;
use url::Url;

/// A running relay: its port and URL, and `close`. Cheaply cloned (the
/// server sits behind an `Arc`); the relay stops at `close` or when the last
/// clone is dropped.
#[derive(Clone)]
pub struct Relay {
  server: Arc<Mutex<Option<Server>>>,
  port: u16,
  url: String,
}

impl Relay {
  /// Serve a relay on every IPv4 interface at `port` (0 for a free one).
  /// `url` is what peers pass as their relay URL: the relay's token as the
  /// user part, the IPv4 of the default route's interface (the address
  /// other hosts on the network reach this one by), or the loopback address
  /// on a machine without one, and the bound port, in the form a ticket
  /// carries.
  pub async fn serve(port: u16) -> Result<Relay, String> {
    let token = super::encode_hex(&SecretKey::generate().to_bytes());
    let mut relay = RelayConfig::new(SocketAddr::from((Ipv4Addr::UNSPECIFIED, port)));
    relay.access = Arc::new(TokenAccess { token: token.clone() });
    let mut config = ServerConfig::default();
    config.relay = Some(relay);
    let server = Server::spawn(config).await.map_err(|e| format!("relay: {e}"))?;
    let bound = server.http_addr().ok_or_else(|| "relay: no HTTP address bound".to_string())?;
    let host = default_ipv4().unwrap_or(Ipv4Addr::LOCALHOST);
    let mut url = Url::parse(&format!("http://{host}:{}/", bound.port())).map_err(|e| format!("relay: {e}"))?;
    url.set_username(&token).map_err(|_| "relay: the URL takes no token".to_string())?;
    Ok(Relay { server: Arc::new(Mutex::new(Some(server))), port: bound.port(), url: url.to_string() })
  }

  /// The port the relay listens on.
  pub fn port(&self) -> u16 {
    self.port
  }

  /// The relay URL peers pass; see `serve`.
  pub fn url(&self) -> &str {
    &self.url
  }

  /// Stop the relay: its listener closes and the connections it carries
  /// end. Idempotent.
  pub async fn close(&self) {
    let server = self.server.lock().await.take();
    if let Some(server) = server {
      if let Err(e) = server.shutdown().await {
        log::warn!("[forge::p2p] relay shutdown: {e}");
      }
    }
  }
}

/// The relay's door: a connection is admitted when its bearer token (what
/// an endpoint configured with the relay's URL sends on the websocket
/// upgrade) is the relay's.
#[derive(Debug)]
struct TokenAccess {
  token: String,
}

impl AccessControl for TokenAccess {
  async fn on_connect(&self, request: &ClientRequest) -> Access {
    if request.auth_token().as_deref() == Some(self.token.as_str()) {
      Access::Allow
    } else {
      Access::Deny { reason: Some("this relay admits its own token only".to_string()) }
    }
  }
}

/// The IPv4 address of the default route's interface, if there is one.
fn default_ipv4() -> Option<Ipv4Addr> {
  netdev::get_default_interface().ok().and_then(|iface| iface.ipv4.first().map(|net| net.addr()))
}
