//! A self-hosted iroh relay served from this process: the one relay an
//! `Endpoint::bind` may name as its `relay_url`. Two peers that have no
//! direct UDP path to each other (or whose direct path fails) exchange
//! their packets through it, each over a TCP connection to the relay, and
//! iroh keeps probing for a direct path meanwhile. Plain HTTP by default
//! (the relay protocol upgrades the connection to a websocket; iroh
//! exchanges relay URLs with the http scheme and dials ws), which is what a
//! relay on a private network wants: no certificate, no domain. With `tls`
//! it serves HTTPS (iroh dials wss) on its port, for a relay on a public
//! network. Everything the relay carries is end-to-end encrypted QUIC, so
//! the relay reads nothing. It admits only endpoints that present one of
//! its tokens (a fresh random one per relay unless the app sets them),
//! carried as the user part of the URL it hands out, so a port that happens
//! to be reachable from further away is not an open relay; an allowlist of
//! endpoint ids narrows that further. Never a public relay: the app hosts
//! this one. An HTTPS relay renews its certificate in place
//! (`set_certificate`) and keeps its URL.

use std::collections::{HashMap, HashSet};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use iroh_base::{EndpointId, SecretKey};
use iroh_relay::server::{
  Access, AccessControl, CertConfig, ClientRequest, ConnectionId, RelayConfig, RelayService, Server, ServerConfig,
  TlsConfig,
};
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;
use url::Url;

/// The fewest characters a relay token has. A token is a bearer secret the
/// relay checks on every connection and the only thing between a reachable
/// port and an open relay; 16 characters of the 66 URL-safe ones carry
/// about 96 bits when drawn at random, past any online guessing, and still
/// fit a QR code comfortably. The default token (64 hex) is well above it.
pub const MIN_TOKEN_LEN: usize = 16;

/// The default bind address: every IPv4 interface.
pub const DEFAULT_HOST: &str = "0.0.0.0";

/// How a relay serves: what `Relay::serve` takes. `Default` is a plain HTTP
/// relay on every IPv4 interface, a free port, one fresh token, any endpoint.
#[derive(Default)]
pub struct RelayOptions {
  /// The relay's port (HTTPS with `tls`, else plain HTTP); 0 for a free one.
  pub port: u16,
  /// The bind address, an IP address; `None` for `DEFAULT_HOST`.
  pub host: Option<String>,
  /// The tokens admitted; the first goes into the relay's URL. `None` for
  /// one fresh random token.
  pub tokens: Option<Vec<String>>,
  /// The endpoint ids admitted (after the token); `None` for any.
  pub allow: Option<Vec<String>>,
  /// The public base URL (behind a proxy, a NAT or a DNS name): the relay's
  /// URL is it with the first token as its user part (and the pin). `None`
  /// for the bound address.
  pub url: Option<String>,
  /// HTTPS: the certificate and when to pin it.
  pub tls: Option<RelayTls>,
}

/// The certificate of an HTTPS relay.
pub struct RelayTls {
  /// The certificate chain (PEM), the leaf first.
  pub cert: Vec<u8>,
  /// The private key (PEM) of `cert`.
  pub key: Vec<u8>,
  /// Whether the relay's URL carries `?pin=` (the leaf's key hash). `None`
  /// pins a self-signed leaf and not a CA-signed one.
  pub pinned: Option<bool>,
}

/// Options `RelayOptions::check` has validated, ready to serve: no
/// validation error is left, only binding can still fail.
pub struct RelaySetup {
  port: u16,
  host: IpAddr,
  tokens: Vec<String>,
  allow: Option<HashSet<EndpointId>>,
  url: Option<Url>,
  tls: Option<(crate::tls::ServerTls, Option<String>)>,
}

impl RelayOptions {
  /// Validate the options and build the TLS config, so that every invalid
  /// option is an error before anything binds. Errors name the option.
  pub fn check(self) -> Result<RelaySetup, String> {
    let host = self.host.as_deref().unwrap_or(DEFAULT_HOST);
    let host: IpAddr = host.parse().map_err(|_| format!("host: not an IP address: {host:?}"))?;
    let tokens = match self.tokens {
      Some(tokens) => {
        if tokens.is_empty() {
          return Err("token: at least one".to_string());
        }
        for token in &tokens {
          check_token(token)?;
        }
        tokens
      }
      None => vec![super::encode_hex(&SecretKey::generate().to_bytes())],
    };
    let allow = self.allow.map(|ids| parse_ids(&ids, "allow")).transpose()?;
    let url = self.url.as_deref().map(parse_base_url).transpose()?;
    // The relay client (iroh-relay 1.4.1) names the server for TLS by the
    // URL's host text, brackets included for an IPv6 address, which rustls
    // refuses: an HTTPS relay URL with an IPv6 host is unreachable. Without
    // `url` the URL carries the bound host (an unspecified one becomes the
    // default route's IPv4).
    let ipv6_url = match &url {
      Some(url) => (url.scheme() == "https" && matches!(url.host(), Some(url::Host::Ipv6(_)))).then_some("url"),
      None => (self.tls.is_some() && host.is_ipv6() && !host.is_unspecified()).then_some("host"),
    };
    if let Some(what) = ipv6_url {
      return Err(format!("{what}: {}: use an IPv4 address, or a url with a DNS name", super::IPV6_HTTPS_UNREACHABLE));
    }
    let tls = match self.tls {
      Some(tls) => {
        let options = crate::tls::ServerOptions {
          cert: tls.cert,
          key: tls.key,
          alpn: Vec::new(),
          client_auth: crate::tls::ClientAuth::None,
        };
        let server = crate::tls::server_config(&options).map_err(|e| format!("tls: {e}"))?;
        let leaf = crate::tls::leaf_from_pem(&options.cert, "tls: cert")?;
        let pinned = match tls.pinned {
          Some(pinned) => pinned,
          None => crate::tls::is_self_signed(&leaf).ok_or_else(|| "tls: cert: does not parse".to_string())?,
        };
        let pin = match pinned {
          true => Some(leaf_pin(&leaf).ok_or_else(|| "tls: cert: does not parse".to_string())?),
          false => None,
        };
        Some((server, pin))
      }
      None => None,
    };
    // Clients dial an http:// URL with plain ws, which a TLS port refuses,
    // pinned or not.
    if tls.is_some() && url.as_ref().is_some_and(|url| url.scheme() != "https") {
      return Err("url: a tls relay needs an https:// url (clients dial an http:// one without TLS)".to_string());
    }
    Ok(RelaySetup { port: self.port, host, tokens, allow, url, tls })
  }
}

/// A running relay: its port and URL, its access policy, its certificate
/// with TLS, and `close`. Cheaply cloned (the server sits behind an `Arc`);
/// the relay stops at `close` or when the last clone is dropped.
#[derive(Clone)]
pub struct Relay {
  server: Arc<Mutex<Option<Server>>>,
  service: Option<RelayService>,
  policy: Arc<Policy>,
  port: u16,
  url: String,
  tls: Option<ServedCert>,
}

/// The certificate an HTTPS relay presents, and the pin its URL carries
/// (`None` when unpinned), which every certificate it renews to keeps.
#[derive(Clone)]
struct ServedCert {
  cert: crate::tls::ServerCert,
  pin: Option<String>,
}

impl Relay {
  /// Serve a relay with `opts` (`RelayOptions::check`, then `start`).
  pub async fn serve(opts: RelayOptions) -> Result<Relay, String> {
    Relay::start(opts.check()?).await
  }

  /// Serve a relay on the setup's host and port. Without TLS that is the
  /// plain HTTP listener; with TLS it is the HTTPS one, and the plain
  /// listener iroh-relay always runs (for the captive-portal probe, which
  /// clients send to port 80, never here) binds a free loopback port, where
  /// nothing reaches it. `url` is what peers pass as their relay URL: the
  /// setup's `url` when it has one, else the bound host (for an unspecified
  /// host, the IPv4 of the default route's interface, the address other
  /// hosts on the network reach this one by; without one the first IPv4
  /// neither loopback nor link-local, and the loopback address only on a machine with
  /// none, see `relay_host_ipv4`) and port, in the form a ticket carries; the
  /// first token as the user part and, when pinned, `?pin=<hex>`.
  pub async fn start(setup: RelaySetup) -> Result<Relay, String> {
    let bind = SocketAddr::new(setup.host, setup.port);
    let policy = Arc::new(Policy::new(setup.tokens.clone(), setup.allow));
    // iroh-relay (1.4.1) wraps a manual config in its acceptor as it is, so
    // the config's resolver, the `ServerCert` kept here, is asked at every
    // handshake: `set_certificate` reaches the running server.
    let (mut relay, served) = match setup.tls {
      Some((server, pin)) => {
        let mut relay = RelayConfig::new(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)));
        let server_config = Arc::unwrap_or_clone(server.config);
        relay.tls = Some(TlsConfig::new(bind, CertConfig::Manual { server_config }));
        (relay, Some(ServedCert { cert: server.cert, pin }))
      }
      None => (RelayConfig::new(bind), None),
    };
    let tls = relay.tls.is_some();
    relay.access = policy.clone();
    let mut config = ServerConfig::default();
    config.relay = Some(relay);
    let server = Server::spawn(config).await.map_err(|e| format!("relay: {e}"))?;
    let bound = match tls {
      true => server.https_addr().ok_or_else(|| "relay: no HTTPS address bound".to_string())?,
      false => server.http_addr().ok_or_else(|| "relay: no HTTP address bound".to_string())?,
    };
    let mut url = match setup.url {
      Some(url) => url,
      None => {
        let host = match bound.ip() {
          ip if ip.is_unspecified() => IpAddr::V4(url_host_ipv4().await),
          ip => ip,
        };
        let scheme = if tls { "https" } else { "http" };
        Url::parse(&format!("{scheme}://{}/", SocketAddr::new(host, bound.port())))
          .map_err(|e| format!("relay: {e}"))?
      }
    };
    url.set_username(&setup.tokens[0]).map_err(|_| "relay: the URL takes no token".to_string())?;
    if let Some(pin) = served.as_ref().and_then(|served| served.pin.as_ref()) {
      url.set_query(Some(&format!("{}={pin}", super::PIN_PARAM)));
    }
    let service = server.relay_service().cloned();
    Ok(Relay {
      server: Arc::new(Mutex::new(Some(server))),
      service,
      policy,
      port: bound.port(),
      url: url.to_string(),
      tls: served,
    })
  }

  /// The port the relay listens on (the HTTPS one with TLS).
  pub fn port(&self) -> u16 {
    self.port
  }

  /// The relay URL peers pass; see `start`.
  pub fn url(&self) -> &str {
    &self.url
  }

  /// Replace the allowlist (`None` admits any endpoint with a token) and
  /// disconnect the connected endpoints it no longer admits. A malformed id
  /// is an error, prefixed `setAllow:`. A disconnected
  /// endpoint's reconnect is then denied. An endpoint admitted under the old
  /// list a moment before this call, but not registered by the relay yet,
  /// stays connected until its next reconnect.
  pub fn set_allow(&self, ids: Option<Vec<String>>) -> Result<(), String> {
    let allow = ids.map(|ids| parse_ids(&ids, "setAllow")).transpose()?;
    let revoked = self.policy.set_allow(allow);
    if let Some(service) = &self.service {
      for id in revoked {
        service.clients().disconnect(id, None);
      }
    }
    Ok(())
  }

  /// The endpoint ids connected to the relay now, sorted.
  pub fn clients(&self) -> Vec<String> {
    self.policy.clients()
  }

  /// Present `cert` and `key` (PEM) from the next TLS handshake on; the
  /// connections made before keep theirs. `url` never changes, so a pinned
  /// relay takes only a certificate on the key its pin names (a new key
  /// would change the pin every ticket and endpoint holds), and an unpinned
  /// one any pair, which clients check by its chain. An error for a plain
  /// HTTP relay, another key on a pinned one, and a bad pair; the current
  /// pair stays in place.
  pub fn set_certificate(&self, cert: &[u8], key: &[u8]) -> Result<(), String> {
    let Some(served) = &self.tls else {
      return Err("a plain HTTP relay has no certificate (serve it with tls)".to_string());
    };
    if let Some(pin) = &served.pin {
      let leaf = crate::tls::leaf_from_pem(cert, "cert")?;
      if leaf_pin(&leaf).as_ref() != Some(pin) {
        return Err(
          "cert: another key than the pin in this relay's url names; a pinned relay keeps its key (serve a new \
           relay, with a new url, for a new key)"
            .to_string(),
        );
      }
    }
    served.cert.set(cert, key)
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

/// The pin of a leaf certificate in its URL form (lowercase hex); `None`
/// for a certificate that does not parse.
fn leaf_pin(leaf: &tokio_rustls::rustls::pki_types::CertificateDer<'_>) -> Option<String> {
  crate::tls::spki_sha256(leaf).map(|hash| super::encode_hex(&hash))
}

/// A token is URL-safe (it travels as the URL's user part unencoded) and at
/// least `MIN_TOKEN_LEN` long. The error does not echo the token: it is the
/// relay's secret.
fn check_token(token: &str) -> Result<(), String> {
  let url_safe = |c: char| c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_' | '~');
  if !token.chars().all(url_safe) {
    return Err("token: only letters, digits and - . _ ~ are allowed".to_string());
  }
  if token.len() < MIN_TOKEN_LEN {
    return Err(format!("token: at least {MIN_TOKEN_LEN} characters, got {}", token.len()));
  }
  Ok(())
}

/// Endpoint ids as `Endpoint::id` gives them; errors name `what`.
fn parse_ids(ids: &[String], what: &str) -> Result<HashSet<EndpointId>, String> {
  ids.iter().map(|id| id.parse().map_err(|e| format!("{what}: invalid endpoint id {id:?}: {e}"))).collect()
}

/// The public base URL: http or https with a host, and no user part, query
/// or fragment of its own (the relay puts its token and pin there), and no
/// `|` (it separates a ticket's fields). No error echoes the URL: a user
/// part in it may be a secret.
fn parse_base_url(s: &str) -> Result<Url, String> {
  if s.contains(super::TICKET_SEPARATOR) {
    return Err(format!("url: takes no {:?} (it separates a ticket's fields)", super::TICKET_SEPARATOR));
  }
  let url = Url::parse(s).map_err(|e| format!("url: not a URL: {e}"))?;
  if !matches!(url.scheme(), "http" | "https") || !url.has_host() {
    return Err(format!("url: not an http or https URL (scheme {:?})", url.scheme()));
  }
  if !url.username().is_empty() || url.password().is_some() || url.query().is_some() || url.fragment().is_some() {
    return Err("url: takes no user part, query or fragment (the relay adds its token and pin)".to_string());
  }
  Ok(url)
}

/// The relay's door: a connection is admitted when its bearer token (what
/// an endpoint configured with the relay's URL sends on the websocket
/// upgrade) is one of the relay's and, with an allowlist, its endpoint id
/// (proved by the relay handshake) is on it. It also keeps the connected
/// endpoints, from the admissions and their disconnects. The tokens are
/// kept as their SHA-256 digests and compared as such, every one, so the
/// time a check takes says nothing about how much of a token matched.
#[derive(Debug)]
struct Policy {
  tokens: Vec<[u8; TOKEN_DIGEST_LEN]>,
  state: std::sync::Mutex<PolicyState>,
}

#[derive(Debug)]
struct PolicyState {
  allow: Option<HashSet<EndpointId>>,
  /// The admitted connections by endpoint (one endpoint may hold several).
  connected: HashMap<EndpointId, HashSet<ConnectionId>>,
}

impl Policy {
  fn new(tokens: Vec<String>, allow: Option<HashSet<EndpointId>>) -> Policy {
    let tokens = tokens.iter().map(|token| token_digest(token)).collect();
    Policy { tokens, state: std::sync::Mutex::new(PolicyState { allow, connected: HashMap::new() }) }
  }

  /// Whether `token` is one of the relay's: its digest against every
  /// token's, with no early exit.
  fn admits_token(&self, token: Option<&str>) -> bool {
    let Some(token) = token else {
      return false;
    };
    let digest = token_digest(token);
    self.tokens.iter().fold(false, |found, known| found | (*known == digest))
  }

  /// Replace the allowlist; the connected endpoints it no longer admits.
  fn set_allow(&self, allow: Option<HashSet<EndpointId>>) -> Vec<EndpointId> {
    let mut state = self.state.lock().expect("relay policy lock");
    let revoked = match &allow {
      Some(allow) => state.connected.keys().filter(|id| !allow.contains(*id)).copied().collect(),
      None => Vec::new(),
    };
    state.allow = allow;
    revoked
  }

  fn clients(&self) -> Vec<String> {
    let state = self.state.lock().expect("relay policy lock");
    let mut ids: Vec<String> = state.connected.keys().map(|id| id.to_string()).collect();
    ids.sort();
    ids
  }
}

impl AccessControl for Policy {
  async fn on_connect(&self, request: &ClientRequest) -> Access {
    let token = request.auth_token();
    if !self.admits_token(token.as_deref()) {
      return Access::Deny { reason: Some("this relay admits its own tokens only".to_string()) };
    }
    let id = request.endpoint_id();
    let mut state = self.state.lock().expect("relay policy lock");
    if state.allow.as_ref().is_some_and(|allow| !allow.contains(&id)) {
      return Access::Deny { reason: Some("this relay does not admit this endpoint".to_string()) };
    }
    state.connected.entry(id).or_default().insert(request.connection_id());
    Access::Allow
  }

  fn on_disconnect(&self, endpoint_id: EndpointId, connection_id: ConnectionId) {
    let mut state = self.state.lock().expect("relay policy lock");
    if let Some(connections) = state.connected.get_mut(&endpoint_id) {
      connections.remove(&connection_id);
      if connections.is_empty() {
        state.connected.remove(&endpoint_id);
      }
    }
  }
}

/// The length of a token's SHA-256 digest.
const TOKEN_DIGEST_LEN: usize = 32;

/// A token's SHA-256 digest, what `Policy` compares.
fn token_digest(token: &str) -> [u8; TOKEN_DIGEST_LEN] {
  Sha256::digest(token.as_bytes()).into()
}

/// The IPv4 address a relay bound to an unspecified host names in its URL
/// (`relay_host_ipv4`), read from the interface state as the endpoint reads
/// it for a ticket (`default_route_ips`), so the two agree on the default
/// route. The candidates beside it are the IPs of the interfaces that are
/// up, sorted so the pick does not follow the map's order.
async fn url_host_ipv4() -> Ipv4Addr {
  let state = netwatch::interfaces::State::new().await;
  let mut up: Vec<IpAddr> = state
    .interfaces
    .values()
    .filter(|interface| interface.is_up())
    .flat_map(|i| i.addrs().map(|net| net.addr()))
    .collect();
  up.sort();
  relay_host_ipv4(super::default_route_ips(&state).as_deref(), &up)
}

/// The IPv4 a relay's URL names for an unspecified bind host: the first
/// IPv4 of the default-route interface (`default_route`, its IPs); with no
/// default route, or none with an IPv4, the first IPv4 among `up` (every
/// interface's IPs) that is neither loopback nor link-local (169.254.x, a
/// self-assigned address no peer is configured to reach), the address
/// another host on a network without a gateway still reaches; the loopback
/// address only when there is none. Pure, so a test drives it with made-up interfaces.
pub(crate) fn relay_host_ipv4(default_route: Option<&[IpAddr]>, up: &[IpAddr]) -> Ipv4Addr {
  let route_ipv4 = default_route.and_then(|ips| {
    ips.iter().find_map(|ip| match ip {
      IpAddr::V4(ip) if !ip.is_loopback() => Some(*ip),
      _ => None,
    })
  });
  let fallback = || {
    up.iter().find_map(|ip| match ip {
      IpAddr::V4(ip) if !ip.is_loopback() && !ip.is_link_local() => Some(*ip),
      _ => None,
    })
  };
  route_ipv4.or_else(fallback).unwrap_or(Ipv4Addr::LOCALHOST)
}
