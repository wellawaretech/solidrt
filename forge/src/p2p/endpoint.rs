//! The peer-to-peer endpoint, built on iroh (the `p2p` feature).
//!
//! The scripting-engine-independent half of `flux:p2p`: the bound `Endpoint`
//! (keypair identity, dial/accept, ticket encoding, path introspection), the
//! bidirectional `Stream` (a byte duplex), and the iroh-facing free functions. It names no scripting-engine types, and no iroh types cross
//! its boundary: `connect`/`accept_one` hand back an assembled `Stream`, and
//! `connect_io`/`accept_io` a byte duplex. The marshalling layer (flux
//! `forge_plugins/p2p.rs`) decodes JS args into these types, builds the JS
//! classes and the accept async-iterable, and encodes results back to JS.
//!
//! "protocol" is the JS-facing name for what QUIC/iroh call the connection's
//! ALPN (RFC 7301): an opaque identifier negotiated in the handshake that both
//! selects and routes the connection. The bytes are passed through verbatim.
//!
//! A `Stream` is a byte-oriented duplex with the `net::Conn` shape: the read
//! half is a `ByteStream` view (`readable()`) that advances only as it is
//! polled; a write awaits the transport under a lock, so a fast producer sees
//! QUIC's flow control as backpressure and a failed write reaches the caller.
//! Nothing runs in the background.

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex;
use url::Url;

use iroh::endpoint::{presets, Connection, RecvStream, RelayMode, SendStream, TransportAddrUsage, VarInt};
use iroh::tls::CaTlsConfig;
use iroh::{
  Endpoint as IrohEndpoint, EndpointAddr, EndpointId, RelayConfig, RelayMap, RelayUrl, SecretKey, TransportAddr,
  Watcher,
};

use super::{default_route_ips, encode_hex, IPV6_HTTPS_UNREACHABLE, PIN_PARAM, TICKET_SEPARATOR};
use crate::stream::{holding, read_half, ByteStream, Closing, ReadSlot};
use crate::tls::{HostTrust, PIN_LEN};
use crate::Value;

/// Read granularity: each read off a `Stream` pulls at most this many bytes.
const READ_CHUNK: usize = 64 * 1024;

/// The application error code an aborted stream resets its send half with.
/// Nothing reads it: the peer sees the reset itself.
const RESET_CODE: u32 = 0;

/// How long `ticket` waits for the addresses it encodes (the configured
/// relay connected, or the first direct address): without a relay it then
/// encodes what it has, with one it fails, naming the relay and the reason.
pub(crate) const TICKET_WAIT: Duration = Duration::from_secs(3);

/// The reason `ticket` reports for a configured relay that has no status
/// entry while no other relay is home: iroh never chose it as the home
/// relay, which means its latency probe got no answer.
pub(crate) const RELAY_NOT_REACHED: &str = "the relay did not answer";

/// The fields of a ticket, `id|relay|ips`: a ticket with more is refused.
const TICKET_FIELDS: usize = 3;

/// What `ticket` rejects with when the relay is left out on an endpoint
/// that binds no UDP socket: such a ticket names no way to reach it.
const RELAY_ONLY_DIRECT_TICKET: &str =
  "A ticket without the relay cannot be dialed: this endpoint has paths \"relay\", so no direct address";

/// The name RFC 6761 reserves for loopback, itself and every name under it.
const LOCALHOST: &str = "localhost";

/// What `connect` rejects with, after "Relay <url>: ", for a ticket whose
/// relay it may not adopt and that names no direct address: nothing is
/// left to dial (iroh would only say it has no addressing information).
const UNADOPTED_RELAY_NO_ADDRESS: &str =
  "not known to this endpoint and adoptRelay is false, and the ticket carries no direct address";

/// The field the endpoint's extra relay roots come from, as errors name it.
const RELAY_CA_CERTS: &str = "relayCaCerts";

/// What a write reports once the stream was closed (not aborted).
const STREAM_CLOSED: &str = "stream is closed";

/// A bound iroh endpoint with a stable keypair. Cheaply cloned (the iroh handle
/// is itself a clone of shared state plus the 32-byte secret), so the caller can
/// move a clone into each async op rather than borrowing across an await.
#[derive(Clone)]
pub struct Endpoint {
  inner: IrohEndpoint,
  /// The 32-byte secret key, kept so it can be read back for persistence.
  secret: [u8; 32],
  /// The relays the endpoint knows, each with its access (`Relays`). A
  /// std mutex, never held across an await: a reader clones what it needs.
  relays: Arc<std::sync::Mutex<Relays>>,
  /// Serializes the look-up-then-write of a relay record and iroh's map:
  /// `connect`'s adoption of a ticket's relay (so two dials of one new
  /// relay insert it once) and `set_relay_url`.
  adopting: Arc<Mutex<()>>,
  /// How each HTTPS relay's host is checked, read by the endpoint's one
  /// certificate verifier: the configured relay's and every adopted
  /// relay's pin, or the chain against the web roots plus `relayCaCerts`.
  trust: HostTrust,
  /// No UDP socket (`paths: "relay"`): no direct address, so a ticket
  /// without the relay is refused.
  relay_only: bool,
}

/// The relays an endpoint knows, each with the token and pin it was first
/// used with.
#[derive(Default)]
struct Relays {
  /// The configured relay, if any (from `bind`, or the last
  /// `set_relay_url`): `ticket` waits for it to connect only then, and
  /// writes it (with its token and pin) into the ticket; `relay_status`
  /// reports it. The endpoint's own surface names no other relay, except
  /// `relays`.
  configured: Option<RelayAccess>,
  /// The relays `connect` adopted from tickets, in adoption order: what
  /// iroh's relay map holds beside the configured relay. An adopted relay
  /// stays until the endpoint closes (iroh's `remove_relay` would not stop
  /// a running relay connection, see okf/notes/iroh-relay-internals.md),
  /// unless `set_relay_url` makes it the configured one.
  adopted: Vec<RelayAccess>,
  /// The configured relays `set_relay_url` replaced: out of iroh's relay
  /// map, but the endpoint keeps their access, since iroh's connection to
  /// one may still run with its token and `HostTrust` never forgets a
  /// host's pin. A ticket naming one re-adopts it with that access, and
  /// `set_relay_url` makes it the configured one again the same way.
  dropped: Vec<RelayAccess>,
}

/// What `Relays` knows of one relay URL, the record cloned out.
enum Known {
  Configured(RelayAccess),
  Adopted(RelayAccess),
  Dropped(RelayAccess),
  Unknown,
}

impl Relays {
  /// Where `url` is recorded, if anywhere.
  fn find(&self, url: &RelayUrl) -> Known {
    let named = |relay: &&RelayAccess| &relay.url == url;
    if let Some(relay) = self.configured.as_ref().filter(named) {
      return Known::Configured(relay.clone());
    }
    if let Some(relay) = self.adopted.iter().find(named) {
      return Known::Adopted(relay.clone());
    }
    match self.dropped.iter().find(named) {
      Some(relay) => Known::Dropped(relay.clone()),
      None => Known::Unknown,
    }
  }
}

/// What supplied a relay, as a `RelayAccess::conflict` error names it.
const FROM_TICKET: &str = "the ticket";
const FROM_URL: &str = "the URL";

/// A configured relay as an app or a ticket names it: the URL proper, the
/// token the relay admits (`relay::Relay` issues one), carried as the
/// URL's user part on the way in and out (`http://<token>@host:port/`), and
/// the pin of an HTTPS relay's key, carried as `?pin=<hex>`.
#[derive(Clone)]
struct RelayAccess {
  url: RelayUrl,
  token: Option<String>,
  pin: Option<[u8; PIN_LEN]>,
}

impl RelayAccess {
  /// Add the relay's host to `trust`: its pin, or the chain check. Only an
  /// HTTPS relay has a certificate to check. An error names the relay
  /// (without its token) when its host is checked another way already
  /// (`HostTrust::add`).
  fn trust(&self, trust: &HostTrust) -> Result<(), String> {
    if self.url.scheme() != "https" {
      return Ok(());
    }
    let Some(host) = self.host() else {
      return Err(format!("Invalid relay URL: no host: {}", self.url));
    };
    trust.add(&host, self.pin).map_err(|e| format!("Relay {}: {e}", self.url))
  }

  /// The URL's host as the certificate check keys it: a DNS name, or an
  /// IP address without brackets.
  fn host(&self) -> Option<String> {
    match self.url.host()? {
      url::Host::Domain(name) => Some(name.to_string()),
      url::Host::Ipv4(ip) => Some(ip.to_string()),
      url::Host::Ipv6(ip) => Some(ip.to_string()),
    }
  }

  /// Why the endpoint's verifier last rejected this relay's certificate
  /// (`HostTrust::rejection`); `None` for a plain HTTP relay.
  fn rejection(&self, trust: &HostTrust) -> Option<String> {
    if self.url.scheme() != "https" {
      return None;
    }
    trust.rejection(&self.host()?)
  }

  /// The relay's entry in iroh's relay map. No QUIC address discovery
  /// port: the self-hosted relay (`relay`) serves no QUIC listener, and an
  /// entry with the default QUIC port (what `RelayMode::custom` writes)
  /// makes iroh probe it and wait out the timeouts before the HTTP probe
  /// counts, 3 s before the endpoint is online (measured 2026-10-17).
  fn config(&self) -> RelayConfig {
    let config = RelayConfig::new(self.url.clone(), None);
    match &self.token {
      Some(token) => config.with_auth_token(token.clone()),
      None => config,
    }
  }

  /// This relay as `relays` reports it: the URL proper, the pin in hex,
  /// and whether it is iroh's home relay now (`homes`: the URLs of iroh's
  /// home relay entries).
  fn known(&self, configured: bool, homes: &[RelayUrl]) -> KnownRelay {
    KnownRelay {
      url: self.url.to_string(),
      pin: self.pin.map(|pin| encode_hex(&pin)),
      configured,
      home: homes.contains(&self.url),
    }
  }

  /// How `named`, this known relay as a ticket or a `set_relay_url` URL
  /// names it, conflicts with the access the endpoint uses for it, `None`
  /// for no conflict. `from` says what supplied it (`FROM_TICKET`,
  /// `FROM_URL`), as the error names it. `check_token` is false for the
  /// configured relay named by a ticket, which admits the endpoint's own
  /// token whatever token a ticket carries. A ticket or URL without a token
  /// or without a pin supplies none: the endpoint keeps the one it knows.
  /// The error names the relay without its token or pin, and each
  /// difference.
  fn conflict(&self, named: &RelayAccess, check_token: bool, from: &str) -> Option<String> {
    let token = match (&self.token, &named.token) {
      (_, None) => None,
      _ if !check_token => None,
      (Some(known), Some(token)) if known == token => None,
      (Some(_), Some(_)) => Some(format!("{from} carries another token than this endpoint uses for it")),
      (None, Some(_)) => Some(format!("{from} carries a token; this endpoint uses it without a token")),
    };
    let pin = match (&self.pin, &named.pin) {
      (_, None) => None,
      (Some(known), Some(pin)) if known == pin => None,
      (Some(_), Some(_)) => Some(format!("{from} carries another pin than this endpoint checks it by")),
      (None, Some(_)) => Some(format!("{from} pins it; this endpoint checks its certificate chain")),
    };
    let what: Vec<String> = token.into_iter().chain(pin).collect();
    (!what.is_empty()).then(|| {
      format!(
        "Relay {}: {}; an endpoint keeps the token and pin it first used for a relay, so a new token or key needs a \
         new endpoint",
        self.url,
        what.join("; ")
      )
    })
  }
}

/// How `Endpoint::connect` treats the relay a ticket names.
#[derive(Clone, Copy, Debug)]
pub struct ConnectOptions {
  /// Adopt the ticket's relay when the endpoint does not know it yet (see
  /// `Endpoint::adopt_relay`). With `false` an unknown ticket relay is not
  /// added and not dialed: the dial uses the ticket's relay only when the
  /// endpoint knows it already, and the ticket's direct addresses. iroh
  /// never routes to a peer through the dialer's own relay unless the
  /// ticket names it, so an unknown relay and no direct address is an error
  /// before the dial (`UNADOPTED_RELAY_NO_ADDRESS`). A known ticket relay is
  /// used either way. Default `true`.
  pub adopt_relay: bool,
}

impl Default for ConnectOptions {
  fn default() -> ConnectOptions {
    ConnectOptions { adopt_relay: true }
  }
}

/// What `Endpoint::ticket` writes.
#[derive(Clone, Copy, Debug)]
pub struct TicketOptions {
  /// Write the configured relay, and wait for it to connect first. With
  /// `false` the ticket is direct-only (no relay field), `ticket` waits for
  /// a direct address only and never fails for the relay. Default `true`.
  pub relay: bool,
  /// Which direct addresses the ticket carries. Default `Auto`.
  pub addresses: TicketAddresses,
}

impl Default for TicketOptions {
  fn default() -> TicketOptions {
    TicketOptions { relay: true, addresses: TicketAddresses::Auto }
  }
}

/// Which direct addresses `Endpoint::ticket` writes. A ticket is a string
/// made to be passed on, so by default it carries only what a peer needs,
/// not the machine's network layout (a VPN, a container bridge, the LAN
/// subnet).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TicketAddresses {
  /// A ticket that carries a relay: none. The peers dial through the relay
  /// and exchange their direct addresses inside the encrypted connection,
  /// which then moves to a direct path. The relay is then the only way in:
  /// a peer that cannot reach it cannot connect, even on the same LAN. A
  /// relay on a loopback, link-local or unspecified address (or
  /// `localhost`) reaches no other machine (`local_relay`), so beside it the
  /// ticket carries what a ticket without a relay does. A ticket without a
  /// relay: the IPv4 of the default-route interface; with no default route,
  /// or none of its IPv4 among the endpoint's addresses, every IPv4 address.
  /// Never IPv6. See `local_ticket_addresses`.
  Auto,
  /// Every direct address the endpoint knows, also beside a relay: a peer
  /// that cannot reach the relay can still dial one of them. With a
  /// portmapped (UPnP, PCP) public address, when iroh reports one.
  All,
}

impl TicketAddresses {
  /// The option as an app names it: `"auto"` or `"all"`.
  pub fn parse(name: &str) -> Result<TicketAddresses, String> {
    match name {
      "auto" => Ok(TicketAddresses::Auto),
      "all" => Ok(TicketAddresses::All),
      other => Err(format!("addresses must be \"auto\" or \"all\", not \"{other}\"")),
    }
  }
}

/// A relay the endpoint knows, as `relays` reports it: the URL without its
/// token or pin, the pin in hex when the relay is checked by one, whether
/// it is the configured relay (else adopted from a ticket; a relay that
/// `set_relay_url` replaced is not listed), and whether
/// iroh made it the home relay (it has an entry in iroh's home relay
/// status, connected or not).
pub struct KnownRelay {
  pub url: String,
  pub pin: Option<String>,
  pub configured: bool,
  pub home: bool,
}

/// `{ url, pin?, configured, home }`.
impl From<KnownRelay> for Value {
  fn from(r: KnownRelay) -> Value {
    let mut fields = vec![("url", Value::from(r.url))];
    if let Some(pin) = r.pin {
      fields.push(("pin", Value::from(pin)));
    }
    fields.push(("configured", Value::from(r.configured)));
    fields.push(("home", Value::from(r.home)));
    Value::map(fields)
  }
}

impl Endpoint {
  /// Bind an endpoint. `secret` is the 32-byte key (generated when `None`);
  /// `relay_url` selects a self-hosted relay (`None`: no relay of its own,
  /// only those `connect` adopts from tickets; `set_relay_url` changes it
  /// later), its token as the URL's user part when the relay admits one;
  /// an HTTPS relay's `?pin=<hex>` replaces the chain and name check of its
  /// certificate by the key check (see `parse_relay_url`); `relay_ca_certs`
  /// (PEM blobs) are extra roots, on top of the web roots, for every HTTPS
  /// relay without a pin, the configured one and adopted ones alike;
  /// `alpns` lists the protocols this endpoint will `accept`. No public
  /// infrastructure is ever used: no public relay and no address publishing or
  /// lookup, so nothing about the endpoint leaves the machine except the
  /// ticket, the sole carrier of addressing (the relay when one is
  /// configured, else the default-route interface's IPv4; see
  /// `TicketAddresses`). `bind_port` pins the UDP
  /// port (IPv4-only) so the ticket's direct address stays fixed across
  /// restarts; paired with a persisted `secret` this makes the whole ticket
  /// stable, so a client can re-dial the old ticket after a restart. `None`
  /// lets iroh pick an ephemeral port. `relay_only` binds no UDP socket at
  /// all: every packet goes through the relay (which must then be given),
  /// for a device whose UDP sends fail
  /// (okf/upstream/noq-udp-android-gso-eio-first-call-stall.md).
  ///
  /// Bind runs on a worker thread (`tokio::spawn`), not the caller's thread:
  /// iroh's bind does blocking work that otherwise stalls a single-threaded host
  /// (observed on Android, starving the render/init commands). This also turns a
  /// bind panic into a returned error instead of a silent teardown.
  pub async fn bind(
    secret: Option<[u8; 32]>,
    relay_url: Option<String>,
    alpns: Vec<Vec<u8>>,
    bind_port: Option<u16>,
    relay_only: bool,
    relay_ca_certs: Vec<Vec<u8>>,
  ) -> Result<Endpoint, String> {
    let relay = relay_url.as_deref().map(parse_relay_url).transpose()?;
    if relay_only && relay.is_none() {
      return Err("Paths \"relay\" needs a relayUrl".to_string());
    }
    let roots = Arc::new(crate::tls::web_roots(&relay_ca_certs, RELAY_CA_CERTS)?);
    let trust = HostTrust::new();
    if let Some(relay) = &relay {
      relay.trust(&trust)?;
    }
    // Always a relay transport, over an empty map without a relay, so a
    // ticket's relay can be added at `connect`.
    let relays = RelayMap::empty();
    if let Some(relay) = &relay {
      relays.insert(relay.url.clone(), Arc::new(relay.config()));
    }
    let verifier = trust.clone();
    let ca_tls = CaTlsConfig::custom_server_cert_verifier(Arc::new(move |provider| {
      verifier.verifier(roots.clone(), provider, RELAY_CA_CERTS).map_err(std::io::Error::other)
    }));
    match tokio::spawn(build_endpoint(secret, relays, ca_tls, alpns, bind_port, relay_only)).await {
      Ok(Ok((inner, secret))) => Ok(Endpoint {
        inner,
        secret,
        relays: Arc::new(std::sync::Mutex::new(Relays { configured: relay, ..Relays::default() })),
        adopting: Arc::new(Mutex::new(())),
        trust,
        relay_only,
      }),
      Ok(Err(e)) => Err(e),
      Err(e) => Err(format!("bind task failed: {e}")),
    }
  }

  /// This endpoint's dial address: the string peers pass to `connect`.
  pub fn id(&self) -> String {
    self.inner.id().to_string()
  }

  /// The secret key as 64 hex chars, for the caller to persist and feed back to
  /// `bind` to keep a stable identity across restarts.
  pub fn secret_key_hex(&self) -> String {
    encode_hex(&self.secret)
  }

  /// A self-contained dial token (`id|relay|ips`) carrying this endpoint's id,
  /// configured relay (with its token and pin, so the peer can use it too;
  /// never a relay adopted from a ticket, even when iroh made it the home
  /// relay), and direct addresses, so a peer can `connect` without any
  /// lookup. `opts.addresses` picks the addresses (`TicketAddresses`): by
  /// default none beside a relay (the local ones beside a relay only this
  /// machine reaches, `local_relay`), else the default-route interface's
  /// IPv4. Waits
  /// (bounded, `TICKET_WAIT`) for what makes it dialable: the configured
  /// relay's own home relay entry connected (not iroh's
  /// `online`, which any home relay satisfies, an adopted one too), else
  /// the first direct address. A configured relay still not connected after
  /// the wait is an error naming the relay (without its token and pin) and
  /// the reason, since a ticket naming it would fail only later, on the
  /// peer's side. `opts.relay == false` writes no relay and skips its wait;
  /// on a relay-only endpoint (no direct address) it is an error at once,
  /// since that ticket could not be dialed. The relay is the one configured
  /// when `ticket` is called: a call already waiting when `set_relay_url`
  /// switches names the relay it started with.
  pub async fn ticket(&self, opts: TicketOptions) -> Result<String, String> {
    if !opts.relay && self.relay_only {
      return Err(RELAY_ONLY_DIRECT_TICKET.to_string());
    }
    let relay = self.configured().filter(|_| opts.relay);
    match &relay {
      Some(relay) => {
        if tokio::time::timeout(TICKET_WAIT, self.relay_connected(relay)).await.is_err() {
          return Err(self.not_online(relay));
        }
      }
      None => {
        let _ = tokio::time::timeout(TICKET_WAIT, self.direct_addr_known()).await;
      }
    }
    let relay = relay.as_ref();
    let addr = self.inner.addr();
    let all: Vec<SocketAddr> = addr.ip_addrs().copied().collect();
    let addrs = match (opts.addresses, relay) {
      (TicketAddresses::All, _) => all,
      (TicketAddresses::Auto, Some(relay)) if !local_relay(&relay.url) => Vec::new(),
      (TicketAddresses::Auto, _) => {
        let state = netwatch::interfaces::State::new().await;
        local_ticket_addresses(&all, default_route_ips(&state).as_deref())
      }
    };
    Ok(encode_ticket(addr.id, relay, &addrs))
  }

  /// Resolves once `relay`'s own entry in iroh's home relay status is
  /// connected; never, once the watcher is gone (the endpoint closed).
  async fn relay_connected(&self, relay: &RelayAccess) {
    let mut watcher = self.inner.home_relay_status();
    let mut value = watcher.get();
    while !value.iter().any(|status| status.url() == &relay.url && status.is_connected()) {
      value = match watcher.updated().await {
        Ok(value) => value,
        Err(_disconnected) => return std::future::pending().await,
      };
    }
  }

  /// Why `relay` is not connected, as `ticket` rejects: the status error
  /// (`status_of`), else that the relay did not answer.
  fn not_online(&self, relay: &RelayAccess) -> String {
    let status = self.status_of(relay);
    let reason = status.error.unwrap_or_else(|| RELAY_NOT_REACHED.to_string());
    format!("Relay {} is not online after {:?}: {reason}", status.url, TICKET_WAIT)
  }

  /// The connection state of the configured relay (`None` without one): its
  /// entry in iroh's home relay status. A relay iroh has not chosen as its
  /// home relay (not reached yet, or never) has no entry and reads as not
  /// connected, with no error, unless its certificate failed the check
  /// (iroh's probe of the relay fails then, before it ever connects): the
  /// error is then the rejection the endpoint's verifier recorded. When iroh
  /// made another relay (one adopted from a ticket) the home relay instead,
  /// the error says so, naming that relay without its token and pin; after
  /// `set_relay_url`, until the new relay answers, that other relay is the
  /// one it replaced. A relay adopted from a ticket is never reported, even
  /// as the home relay.
  pub fn relay_status(&self) -> Option<RelayStatus> {
    self.configured().map(|relay| self.status_of(&relay))
  }

  /// The relays this endpoint knows: the configured one first, then the
  /// ones `connect` adopted, in adoption order, each with whether iroh made
  /// it the home relay. Only `set_relay_url` removes one: the configured
  /// relay it replaces.
  pub fn relays(&self) -> Vec<KnownRelay> {
    let (configured, adopted) = {
      let relays = self.relays.lock().expect("relays lock");
      (relays.configured.clone(), relays.adopted.clone())
    };
    let homes: Vec<RelayUrl> =
      self.inner.home_relay_status().get().into_iter().map(|status| status.url().clone()).collect();
    let configured = configured.iter().map(|relay| relay.known(true, &homes));
    configured.chain(adopted.iter().map(|relay| relay.known(false, &homes))).collect()
  }

  /// The configured relay now, cloned out of the record.
  fn configured(&self) -> Option<RelayAccess> {
    self.relays.lock().expect("relays lock").configured.clone()
  }

  /// Make `url` the configured relay: from now on `ticket` and
  /// `relay_status` name it, and `relays` lists it first. It parses as
  /// `bind` parses `relay_url` (the token as the user part, `?pin=`).
  /// Resolves once the switch is made, not when the relay connects:
  /// `ticket` waits for that, and `relay_status` reports it.
  ///
  /// A relay the endpoint knows (the configured one, one `connect`
  /// adopted, one a former `set_relay_url` replaced) keeps the token and
  /// pin it was first used with, as for `connect` (`RelayAccess::conflict`,
  /// the token checked: iroh's running connection to it keeps its token).
  /// A URL with another token or pin is an error, naming the relay and
  /// what differs; a URL without a token or without a pin uses the known
  /// one. So a new token or key on one URL needs a new endpoint. The
  /// configured relay named with its own access is a no-op. A new relay's
  /// host goes into the host trust first: one host, one check, so a relay
  /// on a host the endpoint checks another way already is an error, and
  /// the endpoint stays as it was.
  ///
  /// The replaced relay always leaves iroh's relay map, even while a
  /// connection runs through it, and its record moves to `Relays::dropped`.
  /// It stays iroh's home relay until the new relay answers a net report
  /// (iroh keeps the current home when no relay answered, also one out of
  /// the map): `relay_status` then reports the new relay not connected,
  /// with "iroh made <old> the home relay". Open connections keep running:
  /// `remove_relay` does not stop iroh's connection to the old relay, which
  /// lasts while a connection uses it. Tickets handed out before the
  /// switch name the old relay, and stop reaching the endpoint once that
  /// connection ends. Relays `connect` adopted stay in the map, so iroh
  /// may still make one of them the home relay instead of the new one (the
  /// gap in okf/upstream/iroh-single-home-relay.md).
  ///
  /// Cancel safety: a call may be dropped at any await. Past the lock, the
  /// records and both of iroh's map writes happen in one poll: the records
  /// first, then `insert_relay` of the new relay (unless it is in the map
  /// already, as an adopted one is) joined with `remove_relay` of the old
  /// one, and each writes the map before its own await (which only tells
  /// iroh's actor to re-run the net report). So a dropped call never leaves
  /// a map entry without a record, which would let a later ticket replace
  /// its token unchecked, nor the old relay in the map as a dropped one. A
  /// closed endpoint writes the records only (`insert_relay` and
  /// `remove_relay` change nothing then), and dials nothing anyway. A
  /// failed parse, conflict or trust check writes nothing.
  pub async fn set_relay_url(&self, url: String) -> Result<(), String> {
    let named = parse_relay_url(&url)?;
    let _adopting = self.adopting.lock().await;
    let known = self.relays.lock().expect("relays lock").find(&named.url);
    // A known relay as `named` may use it: its known access, kept.
    let keep = |known: RelayAccess| known.conflict(&named, true, FROM_URL).map_or(Ok(known), Err);
    let (next, in_map) = match known {
      Known::Configured(own) => return keep(own).map(|_| ()),
      Known::Adopted(known) => (keep(known)?, true),
      Known::Dropped(known) => (keep(known)?, false),
      Known::Unknown => {
        named.trust(&self.trust)?;
        (named, false)
      }
    };
    let old = {
      let mut relays = self.relays.lock().expect("relays lock");
      relays.adopted.retain(|relay| relay.url != next.url);
      relays.dropped.retain(|relay| relay.url != next.url);
      let old = relays.configured.replace(next.clone());
      relays.dropped.extend(old.clone());
      old
    };
    let insert = async {
      if !in_map {
        self.inner.insert_relay(next.url.clone(), Arc::new(next.config())).await;
      }
    };
    let remove = async {
      if let Some(old) = &old {
        self.inner.remove_relay(&old.url).await;
      }
    };
    tokio::join!(insert, remove);
    Ok(())
  }

  /// The URLs of iroh's home relay entries, adopted ones included: what
  /// `relay_status` filters, for a test to see what iroh chose.
  #[cfg(test)]
  pub(crate) fn home_relays(&self) -> Vec<String> {
    self.inner.home_relay_status().get().into_iter().map(|status| status.url().to_string()).collect()
  }

  fn status_of(&self, relay: &RelayAccess) -> RelayStatus {
    let entries = self.inner.home_relay_status().get();
    let entry = entries.iter().find(|status| status.url() == &relay.url);
    let (connected, error) = match entry {
      Some(status) => {
        let error = match status.auth_denied_reason() {
          Some(reason) => Some(format!("Authentication denied ({reason})")),
          None => status.last_error().map(|e| crate::tls::error_chain_message(e)),
        };
        (status.is_connected(), error)
      }
      // iroh keeps one home relay: another one's entry is why this relay
      // has none.
      None => (false, entries.first().map(|other| format!("iroh made {} the home relay", other.url()))),
    };
    // A failed certificate check fails iroh's first probe of the relay, so
    // iroh never connects to it and reports no error: the endpoint's own
    // verifier recorded the reason.
    let error = error.or_else(|| relay.rejection(&self.trust));
    RelayStatus { url: relay.url.to_string(), connected, error: if connected { None } else { error } }
  }

  /// Resolves once the endpoint knows at least one direct address.
  async fn direct_addr_known(&self) {
    let mut watcher = self.inner.watch_addr();
    let mut addr = watcher.get();
    while addr.ip_addrs().next().is_none() {
      addr = match watcher.updated().await {
        Ok(addr) => addr,
        Err(_disconnected) => return,
      };
    }
  }

  /// Dial a peer and open one bidirectional stream over `protocol`. `peer` is a
  /// `ticket`. A bare endpoint `id` parses, but with no address lookup
  /// registered it only reaches a peer this endpoint already knows a path to.
  /// A ticket relay this endpoint does not know yet, or one `set_relay_url`
  /// replaced, is adopted first, unless `opts.adopt_relay` is false (see
  /// `adopt_relay`).
  pub async fn connect(&self, peer: String, protocol: String, opts: ConnectOptions) -> Result<Stream, String> {
    let (conn, send, recv) = self.connect_raw(peer, protocol, opts).await?;
    Ok(Stream::open(conn, send, recv))
  }

  /// Like `connect`, but packaged as a byte duplex (see `accept_io`): for a
  /// byte-oriented consumer such as the dev tunnel's TCP forwarder.
  pub async fn connect_io(&self, peer: String, protocol: String, opts: ConnectOptions) -> Result<ConnIo, String> {
    let (conn, send, recv) = self.connect_raw(peer, protocol, opts).await?;
    Ok(ConnIo { io: tokio::io::join(recv, send), conn })
  }

  async fn connect_raw(
    &self,
    peer: String,
    protocol: String,
    opts: ConnectOptions,
  ) -> Result<(Connection, SendStream, RecvStream), String> {
    let alpn = protocol.into_bytes();
    let ticket = decode_ticket(&peer)?;
    let relay = match &ticket.relay {
      Some(relay) => {
        let used = self.adopt_relay(relay, opts.adopt_relay).await?;
        if used.is_none() && ticket.ips.is_empty() {
          return Err(format!("Relay {}: {UNADOPTED_RELAY_NO_ADDRESS}", relay.url));
        }
        used
      }
      None => None,
    };
    let conn = self.inner.connect(ticket.addr(relay), &alpn).await.map_err(|e| e.to_string())?;
    let (send, recv) = conn.open_bi().await.map_err(|e| e.to_string())?;
    Ok((conn, send, recv))
  }

  /// The relay a dial of a ticket naming `relay` goes through: the relay
  /// itself when the endpoint knows it or adopts it now, `None` when it is
  /// unknown and `adopt` is false. Adopting adds the relay to iroh's relay
  /// map with the ticket's token, so the dial can go through it. iroh reads
  /// a relay's token once, when it starts talking to the relay, so the
  /// insert comes before the dial; the relay's pin (or its chain check)
  /// goes into the endpoint's host trust before that, so the first
  /// handshake already sees it.
  ///
  /// A known relay keeps the token and pin it was first used with (iroh's
  /// running relay connection keeps its token, and `remove_relay` does not
  /// stop it), so a ticket that names it with other access is an error,
  /// naming the relay and what differs (`RelayAccess::conflict`): another
  /// pin, or a pin for a relay checked by its chain, for any known relay;
  /// another token, or a token for a relay adopted without one, for an
  /// adopted relay. The configured relay admits the endpoint's own token,
  /// so a ticket carrying another token for it is fine and the own token is
  /// used. A ticket without a token or without a pin supplies none: the
  /// known one is used. A relay whose host the endpoint checks another way
  /// already (another pin, or a pin against the chain: the verifier sees
  /// the host, not the port) is an error too, and is not added. iroh may
  /// make an adopted relay its home relay; `ticket` and `relay_status`
  /// still name only the configured one.
  ///
  /// A relay `set_relay_url` replaced is out of iroh's map, so it counts as
  /// unknown when `adopt` is false. Adopting it checks the ticket against
  /// its known access (another token is an error, as for an adopted relay)
  /// and inserts it with that access, not the ticket's.
  ///
  /// Cancel safety: a `connect` may be dropped at any await. Past the lock,
  /// the host trust, the record and iroh's map entry are written in one
  /// poll: `insert_relay` puts the entry into the map before its own await
  /// (which only tells iroh's actor to re-run the net report), so a dropped
  /// connect never leaves an entry without a record, which would let a
  /// later ticket replace its token unchecked. The record comes first, so
  /// the one case of a record without an entry is a closed endpoint
  /// (`insert_relay` inserts nothing then), which dials nothing anyway. A
  /// failed conflict or trust check writes neither.
  async fn adopt_relay<'a>(&self, relay: &'a RelayAccess, adopt: bool) -> Result<Option<&'a RelayUrl>, String> {
    let _adopting = self.adopting.lock().await;
    let known = self.relays.lock().expect("relays lock").find(&relay.url);
    let adopted = match known {
      Known::Configured(own) => {
        return own.conflict(relay, false, FROM_TICKET).map_or(Ok(Some(&relay.url)), Err);
      }
      Known::Adopted(known) => {
        return known.conflict(relay, true, FROM_TICKET).map_or(Ok(Some(&relay.url)), Err);
      }
      Known::Dropped(_) | Known::Unknown if !adopt => return Ok(None),
      Known::Dropped(known) => {
        if let Some(error) = known.conflict(relay, true, FROM_TICKET) {
          return Err(error);
        }
        known
      }
      Known::Unknown => {
        relay.trust(&self.trust)?;
        relay.clone()
      }
    };
    {
      let mut relays = self.relays.lock().expect("relays lock");
      relays.dropped.retain(|known| known.url != adopted.url);
      relays.adopted.push(adopted.clone());
    }
    self.inner.insert_relay(adopted.url.clone(), Arc::new(adopted.config())).await;
    Ok(Some(&relay.url))
  }

  /// Accept the next incoming connection matching `alpn` and open its first
  /// bidirectional stream as a `Stream`: it arrives with the dialer's first
  /// bytes, since QUIC announces a stream by its data. `Ok(None)` once the
  /// endpoint stops accepting (closed).
  pub async fn accept_one(&self, alpn: &[u8]) -> Result<Option<Stream>, String> {
    Ok(accept_one(&self.inner, alpn).await?.map(|(conn, send, recv)| Stream::open(conn, send, recv)))
  }

  /// Like `accept_one`, but packaged as a byte duplex: the remote peer's id plus
  /// a `ConnIo` over the connection's first bi-stream, ready for a byte-oriented
  /// consumer (the HTTP server serves it like an accepted TCP socket).
  pub async fn accept_io(&self, alpn: &[u8]) -> Result<Option<(String, ConnIo)>, String> {
    match accept_one(&self.inner, alpn).await? {
      Some((conn, send, recv)) => {
        let remote = conn.remote_id().to_string();
        Ok(Some((remote, ConnIo { io: tokio::io::join(recv, send), conn })))
      }
      None => Ok(None),
    }
  }

  /// Snapshot of how the connection to `id` is currently carried; see `ConnInfo`.
  /// iroh starts on the relay and upgrades to direct after hole-punching, so poll
  /// this to watch the path settle.
  pub async fn conn_info(&self, id: String) -> Result<ConnInfo, String> {
    let id: EndpointId = id.parse().map_err(|e| format!("Invalid endpoint id: {e}"))?;
    let info = self.inner.remote_info(id).await;
    let mut addrs = Vec::new();
    let (mut has_direct, mut has_relay) = (false, false);
    if let Some(info) = info {
      for ta in info.addrs() {
        let active = matches!(ta.usage(), TransportAddrUsage::Active);
        let addr = ta.addr();
        let kind = if addr.is_relay() {
          has_relay |= active;
          "relay"
        } else if addr.is_ip() {
          has_direct |= active;
          "direct"
        } else {
          "custom"
        };
        addrs.push(AddrEntry { kind, addr: addr.to_string(), active });
      }
    }
    let path = match (has_direct, has_relay) {
      (true, true) => "mixed",
      (true, false) => "direct",
      (false, true) => "relay",
      (false, false) => "none",
    };
    Ok(ConnInfo { path, addrs })
  }

  /// Close the endpoint, ending any `accept_one` loop.
  pub async fn close(&self) {
    self.inner.close().await;
  }
}

/// The configured relay's connection state, as `relay_status` reports it.
/// `url` is the relay URL without its token and pin (the token is the
/// relay's whole secret, and this lands in logs and error text); `error` is
/// the most recent
/// failure while not connected (a refused token, a connection error).
pub struct RelayStatus {
  pub url: String,
  pub connected: bool,
  pub error: Option<String>,
}

/// `{ url, connected, error? }`.
impl From<RelayStatus> for Value {
  fn from(s: RelayStatus) -> Value {
    let mut fields = vec![("url", Value::from(s.url)), ("connected", Value::from(s.connected))];
    if let Some(error) = s.error {
      fields.push(("error", Value::from(error)));
    }
    Value::map(fields)
  }
}

/// One known transport address for a peer, as reported by `conn_info`.
pub struct AddrEntry {
  /// `"relay"`, `"direct"` (an IP path), or `"custom"`.
  pub kind: &'static str,
  pub addr: String,
  pub active: bool,
}

/// A snapshot of how a peer connection is currently carried. `path` is
/// `"direct"` (a direct IP path is active), `"relay"` (only a relay path is
/// active), `"mixed"` (both), or `"none"`; `addrs` lists every known transport
/// address.
pub struct ConnInfo {
  pub path: &'static str,
  pub addrs: Vec<AddrEntry>,
}

impl From<AddrEntry> for Value {
  fn from(e: AddrEntry) -> Value {
    Value::map([("kind", Value::from(e.kind)), ("addr", Value::from(e.addr)), ("active", Value::from(e.active))])
  }
}

/// `{ path, addrs: [{ kind, addr, active }] }`.
impl From<ConnInfo> for Value {
  fn from(c: ConnInfo) -> Value {
    Value::map([("path", Value::from(c.path)), ("addrs", Value::list(c.addrs))])
  }
}

/// A single bidirectional p2p stream: a byte duplex. The read half is read
/// through the `readable()` view; writes await the send half under a lock.
/// Holds the `Connection` to keep the QUIC connection (and thus the stream)
/// alive for the stream's lifetime; the read view holds its own handle, so a
/// consumer still reading keeps it up after the `Stream` is gone.
pub struct Stream {
  conn: Connection,
  recv: ReadSlot<RecvStream>,
  send: Mutex<Option<SendStream>>,
  closing: Closing,
}

impl Stream {
  /// Assemble a stream from an opened bi-stream.
  fn open(conn: Connection, send: SendStream, recv: RecvStream) -> Stream {
    Stream { conn, recv: ReadSlot::new(recv), send: Mutex::new(Some(send)), closing: Closing::new() }
  }

  /// The recv half as a `ByteStream`: at most `READ_CHUNK` bytes per chunk,
  /// pulled as the consumer polls, ending at end-of-stream or when the stream
  /// is closed (as an error when aborted). One view per stream: dropping it
  /// releases the recv half, which tells the peer to stop sending (QUIC
  /// STOP_SENDING), while the send half stays usable.
  pub fn readable(&self) -> ByteStream {
    holding(read_half(&self.recv, &self.closing, READ_CHUNK), self.conn.clone())
  }

  /// Write all of `bytes` on the send half, serialized behind the lock and
  /// paced by QUIC flow control. Errors once the send half was finished, the
  /// stream closed or aborted, or when the peer stopped reading or went away.
  pub async fn write(&self, bytes: Vec<u8>) -> Result<(), String> {
    let mut guard = self.send.lock().await;
    let Some(send) = guard.as_mut() else {
      return Err(self.write_closed());
    };
    let r = tokio::select! {
      r = send.write_all(&bytes) => r.map_err(|e| e.to_string()),
      _ = self.closing.closed() => Err(self.closing.write_error(STREAM_CLOSED)),
    };
    if self.closing.is_closed() {
      // close()/abort() could not reach the locked send half; end it here.
      if let Some(send) = guard.take() {
        self.end_send(send);
      }
    }
    r
  }

  /// Half-close: finish the send half (QUIC FIN) after the writes so far,
  /// keeping the recv half open for replies. After it, `write` errors.
  /// Idempotent, and a no-op once the stream is closed.
  pub async fn finish(&self) -> Result<(), String> {
    let mut guard = self.send.lock().await;
    let Some(mut send) = guard.take() else {
      return Ok(());
    };
    if self.closing.is_closed() {
      self.end_send(send);
      return Err(self.closing.write_error(STREAM_CLOSED));
    }
    send.finish().map_err(|e| e.to_string())
  }

  /// Tear the stream down gracefully: a pending read ends, the recv half is
  /// released and the send half finished (QUIC FIN). Idempotent.
  pub fn close(&self) {
    self.closing.close();
    self.release();
  }

  /// Tear the stream down with an error: as `close()`, except that the send
  /// half is reset instead of finished, so the peer's read fails rather than
  /// ending cleanly, and a pending or later read here reports `reason`.
  /// Idempotent (the first end wins).
  pub fn abort(&self, reason: String) {
    self.closing.abort(reason);
    self.release();
  }

  fn release(&self) {
    self.recv.close();
    if let Ok(mut guard) = self.send.try_lock() {
      if let Some(send) = guard.take() {
        self.end_send(send);
      }
    } // else a write is in flight; its closed branch ends the half
  }

  /// End a send half the way the stream ended: reset after an abort, else
  /// finished (which dropping an unfinished half does).
  fn end_send(&self, mut send: SendStream) {
    if self.closing.reason().is_some() {
      let _ = send.reset(VarInt::from_u32(RESET_CODE));
    }
  }

  fn write_closed(&self) -> String {
    if self.closing.is_closed() {
      self.closing.write_error(STREAM_CLOSED)
    } else {
      "send half is finished".to_string()
    }
  }

  /// The remote peer's endpoint id.
  pub fn remote_id(&self) -> String {
    self.conn.remote_id().to_string()
  }
}

/// One connection as a tokio byte duplex: the joined recv/send halves of its
/// first bi-stream, holding the `Connection` so the QUIC connection lives
/// exactly as long as the io handle. Built by `Endpoint::accept_io`.
pub struct ConnIo {
  conn: Connection,
  io: tokio::io::Join<RecvStream, SendStream>,
}

impl ConnIo {
  /// A future resolving once the QUIC connection fully closes (peer close or
  /// idle timeout). Await it after the io has been consumed and served: bytes
  /// written to QUIC are only guaranteed delivery while a connection handle is
  /// alive, so dropping the io right after the final write can discard the
  /// unacknowledged tail. The future holds its own handle, keeping the
  /// connection up until the peer has read everything and closed.
  pub fn closed(&self) -> impl std::future::Future<Output = ()> {
    let conn = self.conn.clone();
    async move {
      conn.closed().await;
    }
  }
}

impl tokio::io::AsyncRead for ConnIo {
  fn poll_read(
    mut self: std::pin::Pin<&mut Self>,
    cx: &mut std::task::Context<'_>,
    buf: &mut tokio::io::ReadBuf<'_>,
  ) -> std::task::Poll<std::io::Result<()>> {
    std::pin::Pin::new(&mut self.io).poll_read(cx, buf)
  }
}

impl tokio::io::AsyncWrite for ConnIo {
  fn poll_write(
    mut self: std::pin::Pin<&mut Self>,
    cx: &mut std::task::Context<'_>,
    buf: &[u8],
  ) -> std::task::Poll<std::io::Result<usize>> {
    std::pin::Pin::new(&mut self.io).poll_write(cx, buf)
  }

  fn poll_flush(
    mut self: std::pin::Pin<&mut Self>,
    cx: &mut std::task::Context<'_>,
  ) -> std::task::Poll<std::io::Result<()>> {
    std::pin::Pin::new(&mut self.io).poll_flush(cx)
  }

  fn poll_shutdown(
    mut self: std::pin::Pin<&mut Self>,
    cx: &mut std::task::Context<'_>,
  ) -> std::task::Poll<std::io::Result<()>> {
    std::pin::Pin::new(&mut self.io).poll_shutdown(cx)
  }
}

/// Whether a relay at `url` reaches no machine but this one (or few): its
/// host is `localhost` (also with a trailing dot, and any `*.localhost`,
/// which RFC 6761 reserves for loopback) or a loopback, link-local or
/// unspecified IP address.
/// A ticket naming such a relay also carries the local addresses
/// (`TicketAddresses::Auto`), since a peer elsewhere cannot use the relay.
/// A DNS name other than `localhost` counts as reachable. Pure.
pub(crate) fn local_relay(url: &Url) -> bool {
  let ip = match url.host() {
    Some(url::Host::Domain(name)) => {
      let name = name.strip_suffix('.').unwrap_or(name).to_ascii_lowercase();
      return name == LOCALHOST || name.ends_with(&format!(".{LOCALHOST}"));
    }
    Some(url::Host::Ipv4(ip)) => IpAddr::V4(ip),
    Some(url::Host::Ipv6(ip)) => IpAddr::V6(ip).to_canonical(),
    None => return false,
  };
  match ip {
    IpAddr::V4(ip) => ip.is_loopback() || ip.is_link_local() || ip.is_unspecified(),
    IpAddr::V6(ip) => ip.is_loopback() || ip.is_unicast_link_local() || ip.is_unspecified(),
  }
}

/// The direct addresses a ticket without a relay (or with a `local_relay`)
/// carries (`TicketAddresses::Auto`), out of `addrs` (iroh's direct addresses, every
/// non-loopback interface with the bound port): the entries whose IP is an
/// IPv4 of the default-route interface (`default_route`, its IPs as
/// `default_route_ips` reads them), the one a peer on the LAN can use. With
/// no default route, or none of its IPv4 among `addrs` (an IPv6-only
/// default route), every IPv4 entry. Never an IPv6 one: a global IPv6 is
/// routable, long-lived and names the provider. Pure, so a test drives it
/// with made-up interfaces.
pub(crate) fn local_ticket_addresses(addrs: &[SocketAddr], default_route: Option<&[IpAddr]>) -> Vec<SocketAddr> {
  let ipv4: Vec<SocketAddr> = addrs.iter().copied().filter(SocketAddr::is_ipv4).collect();
  let routed: Vec<SocketAddr> = match default_route {
    Some(ips) => ipv4.iter().copied().filter(|addr| ips.contains(&addr.ip())).collect(),
    None => Vec::new(),
  };
  if routed.is_empty() {
    ipv4
  } else {
    routed
  }
}

/// Encode a ticket as a compact string `id|relay|ip1,ip2,...` (relay and
/// ips optional). The relay is the configured one, with its token as its
/// user part, never a relay address iroh reports in `addr` (that may be
/// one adopted from a ticket). Hand-rolled rather than serde to keep the
/// QR small.
fn encode_ticket(id: EndpointId, relay: Option<&RelayAccess>, addrs: &[SocketAddr]) -> String {
  let relay = relay.map(relay_with_token).unwrap_or_default();
  let ips: Vec<String> = addrs.iter().map(SocketAddr::to_string).collect();
  format!("{id}|{relay}|{}", ips.join(","))
}

/// A dial target parsed: a bare endpoint id or an `encode_ticket` string.
struct DialTicket {
  id: EndpointId,
  /// The ticket's relay, its token included.
  relay: Option<RelayAccess>,
  ips: Vec<SocketAddr>,
}

impl DialTicket {
  /// The address iroh dials: `relay`, the URL proper of the ticket's relay
  /// when the dial goes through it (the token is for the dialer's relay
  /// map, see `Endpoint::adopt_relay`), and the direct addresses.
  fn addr(&self, relay: Option<&RelayUrl>) -> EndpointAddr {
    let relay = relay.into_iter().map(|url| TransportAddr::Relay(url.clone()));
    let ips = self.ips.iter().map(|ip| TransportAddr::Ip(*ip));
    EndpointAddr::from_parts(self.id, relay.chain(ips).collect::<Vec<_>>())
  }
}

/// Parse a dial target that is either a bare endpoint id or an `encode_ticket`
/// string. A bare id (no `|`) yields an id-only target (with no lookup
/// registered, it dials only a peer already known); a ticket carries the
/// relay + direct addresses. More than `TICKET_FIELDS` fields is an error,
/// not a tail to ignore.
fn decode_ticket(s: &str) -> Result<DialTicket, String> {
  if s.split(TICKET_SEPARATOR).count() > TICKET_FIELDS {
    return Err(format!("Invalid ticket: more than {TICKET_FIELDS} fields (id|relay|addresses)"));
  }
  let mut parts = s.split(TICKET_SEPARATOR);
  let id: EndpointId = parts.next().unwrap_or("").trim().parse().map_err(|e| format!("Invalid endpoint id: {e}"))?;
  let relay = parts.next().filter(|s| !s.is_empty()).map(parse_relay_url).transpose()?;
  let mut ips = Vec::new();
  if let Some(list) = parts.next() {
    for ip in list.split(',').filter(|s| !s.is_empty()) {
      ips.push(ip.parse::<SocketAddr>().map_err(|e| format!("Invalid ticket address {ip}: {e}"))?);
    }
  }
  Ok(DialTicket { id, relay, ips })
}

/// A ticket's parts, as `parse_ticket` returns them: the endpoint id, the
/// relay URL with its token as its user part and its `?pin=` (`None` when
/// the ticket names no relay), and the direct addresses.
pub struct Ticket {
  pub id: String,
  pub relay_url: Option<String>,
  pub addresses: Vec<String>,
}

/// `{ id, relayUrl, addresses }`, `relayUrl` null without a relay.
impl From<Ticket> for Value {
  fn from(t: Ticket) -> Value {
    Value::map([
      ("id", Value::from(t.id)),
      ("relayUrl", Value::from(t.relay_url)),
      ("addresses", Value::list(t.addresses)),
    ])
  }
}

/// Parse a ticket (or a bare endpoint id) into its parts, as `connect`
/// reads it: for an app that wants the ticket's relay as its own
/// `relayUrl`. Pure; an invalid id, relay URL or address is an error.
pub fn parse_ticket(s: &str) -> Result<Ticket, String> {
  let ticket = decode_ticket(s)?;
  Ok(Ticket {
    id: ticket.id.to_string(),
    relay_url: ticket.relay.as_ref().map(relay_with_token),
    addresses: ticket.ips.iter().map(SocketAddr::to_string).collect(),
  })
}

/// Build and bind an iroh endpoint. Returns it together with the (possibly
/// generated) secret-key bytes so the caller can expose them for persistence.
///
/// Always built from `presets::Minimal` (crypto provider only), never
/// `presets::N0`: N0 uses n0's public relays and registers a pkarr publisher +
/// DNS lookup against n0's public server, announcing the endpoint's addresses
/// off-machine. The only relays are those in `relays`: the self-hosted one
/// `relay_url` names, and later the ones `connect` adopts from tickets and
/// the one `set_relay_url` names.
async fn build_endpoint(
  secret: Option<[u8; 32]>,
  relays: RelayMap,
  ca_tls: CaTlsConfig,
  alpns: Vec<Vec<u8>>,
  bind_port: Option<u16>,
  relay_only: bool,
) -> Result<(IrohEndpoint, [u8; 32]), String> {
  let secret_key = match secret {
    Some(bytes) => SecretKey::from_bytes(&bytes),
    None => SecretKey::generate(),
  };
  let bytes = secret_key.to_bytes();

  // The relays' certificates go through the endpoint's own verifier
  // (`HostTrust`): a pin by host, else the web roots plus `relayCaCerts`.
  let mut builder = IrohEndpoint::builder(presets::Minimal).secret_key(secret_key).alpns(alpns).ca_tls_config(ca_tls);
  // IPv4 only: `clear_ip_transports` drops the pre-configured unspecified
  // sockets (one per family), then `bind_addr` binds exactly one IPv4 socket,
  // on the pinned port when there is one (so the ticket's direct address is
  // stable across restarts, paired with a persisted key) or any free port.
  // iroh prefers an IPv6 path when both exist, and a phone's global IPv6
  // privacy addresses rotate under a connection (observed 2026-10-16: a
  // stream stalled after its first packets on such a path), while the LAN
  // path every device here shares is IPv4.
  // Relay-only: no IP transport at all, so nothing is sent over UDP.
  builder = builder.clear_ip_transports();
  if !relay_only {
    builder = builder
      .bind_addr((std::net::Ipv4Addr::UNSPECIFIED, bind_port.unwrap_or(0)))
      .map_err(|e| format!("invalid bind port: {e}"))?;
  }
  // Always the relay transport, over an empty map for an endpoint without
  // a relay: `RelayMode::Disabled` would leave no transport for a ticket's
  // relay to be added to. An empty map runs no net report, as Disabled did.
  let relay_mode = RelayMode::Custom(relays);
  builder = builder.relay_mode(relay_mode);
  // No GSO on Android: a phone kernel may answer a segmented send with EIO,
  // and noq-udp halts GSO then but drops that batch, so a fresh endpoint's
  // first bulk data stalls on the direct path until loss recovery finds it
  // (okf/upstream/noq-udp-android-gso-eio-first-call-stall.md). Starting
  // without it costs CPU per datagram, which is negligible at call rates.
  // Remove once a noq-udp release under iroh resends that batch (quinn
  // issue 2399); the note lists the steps and the re-test.
  #[cfg(target_os = "android")]
  {
    let transport = iroh::endpoint::QuicTransportConfig::builder().enable_segmentation_offload(false).build();
    builder = builder.transport_config(transport);
  }
  let endpoint = builder.bind().await.map_err(|e| e.to_string())?;
  Ok((endpoint, bytes))
}

/// Accept the next incoming connection matching `alpn` and open its first
/// bidirectional stream. Returns `None` once the endpoint stops accepting.
/// Non-matching or failed connections are skipped.
async fn accept_one(ep: &IrohEndpoint, alpn: &[u8]) -> Result<Option<(Connection, SendStream, RecvStream)>, String> {
  loop {
    let Some(incoming) = ep.accept().await else {
      return Ok(None);
    };
    let conn = match incoming.await {
      Ok(conn) => conn,
      Err(_) => continue,
    };
    if conn.alpn() != alpn {
      conn.close(0u32.into(), b"alpn mismatch");
      continue;
    }
    match conn.accept_bi().await {
      Ok((send, recv)) => return Ok(Some((conn, send, recv))),
      Err(_) => continue,
    }
  }
}

/// A relay URL as an app or a ticket gives it, `http://<token>@host:port/`
/// when the relay admits a token, `https://<token>@host:port/?pin=<hex>`
/// for an HTTPS relay checked by its key: the URL proper, the token and
/// the pin. The token and the query are stripped from the URL iroh sees.
/// The scheme is `http` or `https` only: iroh's relay client dials any
/// other one (`wss`, `ftp`) with TLS, past the endpoint's `HostTrust`,
/// which checks only `https` relays. `pin` is the only query parameter,
/// once, on `https://` only: a pin on plain HTTP (no certificate to check),
/// a malformed pin, a repeated one or any other parameter is an error, so a
/// typo never leaves a relay unchecked. A password in the user part (the
/// token is the whole user part), a fragment, a `|` anywhere (it separates
/// a ticket's fields) and an `https://` URL with an IPv6 address
/// (`IPV6_HTTPS_UNREACHABLE`) are errors too. No error echoes the URL: its
/// user part is the relay's secret.
fn parse_relay_url(s: &str) -> Result<RelayAccess, String> {
  if s.contains(TICKET_SEPARATOR) {
    return Err(format!("Invalid relay URL: a {TICKET_SEPARATOR:?} (it separates a ticket's fields)"));
  }
  let mut url = Url::parse(s).map_err(|e| format!("Invalid relay URL: {e}"))?;
  if !matches!(url.scheme(), "http" | "https") {
    return Err(format!("Invalid relay URL: the scheme is {:?}, not http or https", url.scheme()));
  }
  if url.password().is_some() {
    return Err("Invalid relay URL: a password in the user part (the token is the whole user part)".to_string());
  }
  if url.fragment().is_some() {
    return Err("Invalid relay URL: a fragment".to_string());
  }
  if url.scheme() == "https" && matches!(url.host(), Some(url::Host::Ipv6(_))) {
    return Err(format!("Invalid relay URL: {IPV6_HTTPS_UNREACHABLE}: use an IPv4 address or a DNS name"));
  }
  let token = (!url.username().is_empty()).then(|| url.username().to_string());
  url.set_username("").map_err(|_| "Invalid relay URL: it takes no token".to_string())?;
  let mut pin = None;
  for (key, value) in url.query_pairs() {
    if key != PIN_PARAM {
      return Err(format!("Invalid relay URL: unknown query parameter {key:?} (only {PIN_PARAM})"));
    }
    if pin.is_some() {
      return Err(format!("Invalid relay URL: {PIN_PARAM} given twice"));
    }
    pin = Some(crate::tls::parse_pin(&value).map_err(|e| format!("Invalid relay URL: {e}"))?);
  }
  if pin.is_some() && url.scheme() != "https" {
    return Err(format!("Invalid relay URL: a {PIN_PARAM} needs https://, the relay's certificate is what it checks"));
  }
  url.set_query(None);
  Ok(RelayAccess { url: RelayUrl::from(url), token, pin })
}

/// A relay URL as a ticket carries it: the token back as its user part and
/// the pin as `?pin=<hex>`, so the peer dialing the ticket can use and
/// check the same relay.
fn relay_with_token(relay: &RelayAccess) -> String {
  let mut url: Url = (*relay.url).clone();
  if let Some(token) = &relay.token {
    let _ = url.set_username(token);
  }
  if let Some(pin) = &relay.pin {
    url.set_query(Some(&format!("{PIN_PARAM}={}", encode_hex(pin))));
  }
  url.to_string()
}

/// Decode 64 hex chars into 32 bytes (a secret key). Engine-free; the caller
/// extracts the string from JS first.
pub fn decode_hex32(s: &str) -> Result<[u8; 32], String> {
  if s.len() != 64 {
    return Err("secretKey must be 64 hex characters (32 bytes)".to_string());
  }
  let mut out = [0u8; 32];
  for (i, slot) in out.iter_mut().enumerate() {
    *slot = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).map_err(|_| "secretKey is not valid hex".to_string())?;
  }
  Ok(out)
}
