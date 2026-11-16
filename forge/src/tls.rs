//! Engine-free TLS configuration.
//!
//! Builds the rustls client and server configs that `forge::net` (TLS
//! sockets) and later `forge::fetch` and `forge::websocket` hand to their
//! connections, from native types: PEM bytes, a pin, ALPN strings. All on
//! the ring provider through `builder_with_provider`, the provider reqwest
//! uses, so no process-wide default provider is needed.
//!
//! Two verifiers go beyond rustls's own:
//!
//! - The pin (`PinVerifier`): the server's key is its identity. The leaf's
//!   SubjectPublicKeyInfo hash must match the pin, which replaces both the
//!   chain check and the name check (a LAN server's self-signed certificate
//!   rarely names the address a client reaches it on). The handshake
//!   signatures are still verified, so a peer that replays the certificate
//!   without the key fails.
//! - Per host (`HostTrust`): a set of servers (an endpoint's relays) where
//!   a host may carry a pin, checked as above, and every other host gets
//!   the chain and name check against the roots. One verifier for them
//!   all, because the TLS stack that dials them takes one.
//! - Any client key (`AnyClientKey`): a client certificate is required, any
//!   key is accepted, and the app checks the peer's key itself (pairing).
//!   The signatures are verified here too.
//!
//! Session resumption is automatic. A connection over the config
//! `ClientConfigs` keeps for its options resumes a session from an earlier
//! connection with equal options to the same server name (a config owns its
//! session store, and rustls keys that store by server name). The options
//! are part of the key, not one config per engine: a resumed handshake
//! verifies no certificate, so a session verified under one trust policy
//! (roots, pin, client certificate) must not serve a connection under
//! another. `client_config_without_resumption` opts one connection out.
//!
//! `generate_certificate` makes the identities these configs take: an ECDSA
//! P-256 key and a certificate for it, self-signed or signed by a CA made the
//! same way, through rcgen on ring.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use sha2::{Digest, Sha256};
use tokio_rustls::rustls;
use tokio_rustls::rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use tokio_rustls::rustls::client::{Resumption, WebPkiServerVerifier};
use tokio_rustls::rustls::crypto::{verify_tls12_signature, verify_tls13_signature, CryptoProvider};
use tokio_rustls::rustls::pki_types::pem::PemObject;
use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime};
use tokio_rustls::rustls::server::danger::{ClientCertVerified, ClientCertVerifier};
use tokio_rustls::rustls::server::{ParsedCertificate, WebPkiClientVerifier};
use tokio_rustls::rustls::{
  CertificateError, CommonState, DigitallySignedStruct, DistinguishedName, OtherError, SignatureScheme,
};

pub use tokio_rustls::rustls::{ClientConfig, RootCertStore, ServerConfig};

/// The length of a pin and of a peer key: a SHA-256 hash.
pub const PIN_LEN: usize = 32;

/// How long a server-side handshake may take before the connection is
/// dropped: a client that connects and stalls holds nothing past it. Shared
/// by `net::listen_tls` and the HTTPS server of `http`.
pub(crate) const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

/// How many distinct client option sets keep their config (and so their
/// sessions) at once in a `ClientConfigs`; the least recently used one is
/// dropped past it.
pub const CLIENT_CONFIG_KEYS: usize = 16;

/// How a TLS client verifies the server and identifies itself.
#[derive(Default)]
pub struct ClientOptions {
  /// Extra trust anchors on top of the webpki roots, each a PEM blob that
  /// may hold several certificates.
  pub ca_certs: Vec<Vec<u8>>,
  /// A client certificate chain (PEM) for mutual TLS, with `key`.
  pub cert: Option<Vec<u8>>,
  /// The private key (PEM) of `cert`.
  pub key: Option<Vec<u8>>,
  /// The SHA-256 of the server's SubjectPublicKeyInfo. Replaces the chain
  /// and the name check, so it excludes `ca_certs`.
  pub pin: Option<[u8; PIN_LEN]>,
  /// The protocols to offer by ALPN, in preference order.
  pub alpn: Vec<String>,
}

/// One rustls client config per distinct client identity, so that
/// connections with equal options share sessions and no others do (see the
/// module doc). A config owns its session store, and rustls resumes a
/// session only under the very same verifier and credential objects it was
/// made under, so the config itself is what has to be shared. Holds at most
/// `CLIENT_CONFIG_KEYS` configs, dropping the least recently used.
#[derive(Default)]
pub struct ClientConfigs {
  /// The configs by identity, the most recently used last.
  configs: Mutex<Vec<(ClientIdentity, Arc<ClientConfig>)>>,
}

impl ClientConfigs {
  pub fn new() -> ClientConfigs {
    ClientConfigs::default()
  }

  /// The config for `opts`: the same one for equal options, a new one
  /// (`client_config`) for options not seen (or dropped) before.
  pub fn config_for(&self, opts: &ClientOptions) -> Result<Arc<ClientConfig>, String> {
    let key = client_identity(opts);
    let mut configs = self.configs.lock().expect("TLS client configs lock");
    if let Some(at) = configs.iter().position(|(k, _)| *k == key) {
      let entry = configs.remove(at);
      let config = entry.1.clone();
      configs.push(entry);
      return Ok(config);
    }
    let config = client_config(opts)?;
    if configs.len() >= CLIENT_CONFIG_KEYS {
      configs.remove(0);
    }
    configs.push((key, config.clone()));
    Ok(config)
  }
}

/// What `client_identity` gives: equal for equal client options only.
pub type ClientIdentity = [u8; PIN_LEN];

/// The identity of client options, the key `ClientConfigs` keeps a config
/// by: the SHA-256 of the options' trust policy and identity, every field
/// each length-prefixed and each `Option` behind a presence byte, so that
/// no two distinct options encode the same. A TLS client `net::Conn`
/// records the identity of the options it was made with.
pub fn client_identity(opts: &ClientOptions) -> ClientIdentity {
  fn bytes(hash: &mut Sha256, b: &[u8]) {
    hash.update((b.len() as u64).to_le_bytes());
    hash.update(b);
  }
  fn optional(hash: &mut Sha256, b: Option<&[u8]>) {
    match b {
      Some(b) => {
        hash.update([1]);
        bytes(hash, b);
      }
      None => hash.update([0]),
    }
  }
  let mut hash = Sha256::new();
  hash.update((opts.ca_certs.len() as u64).to_le_bytes());
  for blob in &opts.ca_certs {
    bytes(&mut hash, blob);
  }
  optional(&mut hash, opts.cert.as_deref());
  optional(&mut hash, opts.key.as_deref());
  optional(&mut hash, opts.pin.as_ref().map(|p| p.as_slice()));
  hash.update((opts.alpn.len() as u64).to_le_bytes());
  for p in &opts.alpn {
    bytes(&mut hash, p.as_bytes());
  }
  hash.finalize().into()
}

/// How a TLS server authenticates its clients.
#[derive(Default)]
pub enum ClientAuth {
  /// No client certificate is asked for.
  #[default]
  None,
  /// A client certificate that chains to one of these PEM blobs is required.
  Chain(Vec<Vec<u8>>),
  /// A client certificate is required, any key is accepted; the app checks
  /// the peer key itself.
  AnyKey,
}

/// A TLS server's identity and policy.
pub struct ServerOptions {
  /// The server certificate chain (PEM).
  pub cert: Vec<u8>,
  /// The private key (PEM) of `cert`.
  pub key: Vec<u8>,
  /// The protocols to accept by ALPN, in preference order.
  pub alpn: Vec<String>,
  pub client_auth: ClientAuth,
}

fn provider() -> Arc<CryptoProvider> {
  Arc::new(rustls::crypto::ring::default_provider())
}

fn certs_from_pem(pem: &[u8], what: &str) -> Result<Vec<CertificateDer<'static>>, String> {
  let certs = CertificateDer::pem_slice_iter(pem)
    .collect::<Result<Vec<_>, _>>()
    .map_err(|e| format!("{what}: not a PEM certificate: {e}"))?;
  if certs.is_empty() {
    return Err(format!("{what}: no PEM certificate found"));
  }
  Ok(certs)
}

fn key_from_pem(pem: &[u8], what: &str) -> Result<PrivateKeyDer<'static>, String> {
  PrivateKeyDer::from_pem_slice(pem).map_err(|e| format!("{what}: not a PEM private key: {e}"))
}

fn add_roots(roots: &mut RootCertStore, blobs: &[Vec<u8>], what: &str) -> Result<(), String> {
  for blob in blobs {
    for cert in certs_from_pem(blob, what)? {
      roots.add(cert).map_err(|e| format!("{what}: {e}"))?;
    }
  }
  Ok(())
}

/// The roots a server's chain is checked against: the webpki roots plus
/// `ca_certs` (PEM blobs, each possibly several certificates). Errors name
/// `what`, the field the PEM came from.
pub fn web_roots(ca_certs: &[Vec<u8>], what: &str) -> Result<RootCertStore, String> {
  let mut roots = RootCertStore { roots: webpki_roots::TLS_SERVER_ROOTS.to_vec() };
  add_roots(&mut roots, ca_certs, what)?;
  Ok(roots)
}

/// The chain and name check against `roots` (`web_roots`), rustls's own
/// verifier. Errors name `what`, the field the extra roots came from.
pub fn roots_verifier(
  roots: Arc<RootCertStore>,
  provider: Arc<CryptoProvider>,
  what: &str,
) -> Result<Arc<WebPkiServerVerifier>, String> {
  WebPkiServerVerifier::builder_with_provider(roots, provider).build().map_err(|e| format!("{what}: {e}"))
}

/// The pin check (see the module doc): the server's key against `pin`,
/// in place of the chain and name check.
pub fn pin_verifier(pin: [u8; PIN_LEN], provider: Arc<CryptoProvider>) -> Arc<dyn ServerCertVerifier> {
  Arc::new(PinVerifier { pin, provider })
}

fn alpn_bytes(alpn: &[String]) -> Vec<Vec<u8>> {
  alpn.iter().map(|p| p.as_bytes().to_vec()).collect()
}

/// Build a client config: the webpki roots plus `ca_certs`, or the pin; an
/// optional client certificate; the ALPN list.
pub fn client_config(opts: &ClientOptions) -> Result<Arc<ClientConfig>, String> {
  Ok(Arc::new(build_client_config(opts)?))
}

/// A client config as `client_config` builds it, but one that neither
/// offers nor stores sessions: a one-off config, not cached, for a
/// connection that opts out of resumption, so it does a full handshake and
/// the sessions of the cached config for equal options are neither offered
/// nor replaced.
pub fn client_config_without_resumption(opts: &ClientOptions) -> Result<Arc<ClientConfig>, String> {
  let mut config = build_client_config(opts)?;
  config.resumption = Resumption::disabled();
  Ok(Arc::new(config))
}

fn build_client_config(opts: &ClientOptions) -> Result<ClientConfig, String> {
  let provider = provider();
  let builder = ClientConfig::builder_with_provider(provider.clone())
    .with_safe_default_protocol_versions()
    .map_err(|e| e.to_string())?;
  let builder = match opts.pin {
    Some(pin) => {
      if !opts.ca_certs.is_empty() {
        return Err("pin and caCerts exclude each other: a pin replaces the chain check".to_string());
      }
      builder.dangerous().with_custom_certificate_verifier(pin_verifier(pin, provider))
    }
    None => {
      let roots = web_roots(&opts.ca_certs, "caCerts")?;
      builder.with_webpki_verifier(roots_verifier(Arc::new(roots), provider, "caCerts")?)
    }
  };
  let mut config = match (&opts.cert, &opts.key) {
    (Some(cert), Some(key)) => builder
      .with_client_auth_cert(certs_from_pem(cert, "cert")?, key_from_pem(key, "key")?)
      .map_err(|e| format!("cert and key: {e}"))?,
    (None, None) => builder.with_no_client_auth(),
    _ => return Err("cert and key go together".to_string()),
  };
  config.alpn_protocols = alpn_bytes(&opts.alpn);
  Ok(config)
}

/// Build a server config: the identity, the client authentication, the
/// ALPN list.
pub fn server_config(opts: &ServerOptions) -> Result<Arc<ServerConfig>, String> {
  server_config_with_versions(opts, rustls::DEFAULT_VERSIONS)
}

/// `server_config` limited to `versions` (the tests' TLS 1.2-only server).
pub(crate) fn server_config_with_versions(
  opts: &ServerOptions,
  versions: &[&'static rustls::SupportedProtocolVersion],
) -> Result<Arc<ServerConfig>, String> {
  let provider = provider();
  let builder = ServerConfig::builder_with_provider(provider.clone())
    .with_protocol_versions(versions)
    .map_err(|e| e.to_string())?;
  let builder = match &opts.client_auth {
    ClientAuth::None => builder.with_no_client_auth(),
    ClientAuth::Chain(blobs) => {
      let mut roots = RootCertStore::empty();
      add_roots(&mut roots, blobs, "caCerts")?;
      let verifier = WebPkiClientVerifier::builder_with_provider(Arc::new(roots), provider)
        .build()
        .map_err(|e| format!("caCerts: {e}"))?;
      builder.with_client_cert_verifier(verifier)
    }
    ClientAuth::AnyKey => builder.with_client_cert_verifier(Arc::new(AnyClientKey { provider })),
  };
  let mut config = builder
    .with_single_cert(certs_from_pem(&opts.cert, "cert")?, key_from_pem(&opts.key, "key")?)
    .map_err(|e| format!("cert and key: {e}"))?;
  config.alpn_protocols = alpn_bytes(&opts.alpn);
  Ok(Arc::new(config))
}

/// The server name to verify and send as SNI: a DNS name, or an IP address
/// (which rustls verifies but does not send).
pub fn server_name(host: &str) -> Result<ServerName<'static>, String> {
  ServerName::try_from(host.to_string()).map_err(|_| format!("invalid server name: {host}"))
}

/// The SHA-256 of a certificate's SubjectPublicKeyInfo; `None` for a
/// certificate that does not parse.
pub fn spki_sha256(cert: &CertificateDer<'_>) -> Option<[u8; PIN_LEN]> {
  let parsed = ParsedCertificate::try_from(cert).ok()?;
  Some(Sha256::digest(parsed.subject_public_key_info().as_ref()).into())
}

/// The leaf of a PEM certificate chain: its first certificate. Errors name
/// `what`, the field the PEM came from.
pub fn leaf_from_pem(pem: &[u8], what: &str) -> Result<CertificateDer<'static>, String> {
  Ok(certs_from_pem(pem, what)?.swap_remove(0))
}

/// Whether a certificate is self-signed: its issuer equals its subject, as
/// `generate_certificate` makes one without an `issuer`. `None` for a
/// certificate that does not parse.
pub fn is_self_signed(cert: &CertificateDer<'_>) -> Option<bool> {
  let parsed = webpki::EndEntityCert::try_from(cert).ok()?;
  Some(parsed.issuer() == parsed.subject())
}

/// The peer's key as lowercase hex (`spki_sha256` of its leaf), `None`
/// without a peer certificate.
pub fn peer_key(state: &CommonState) -> Option<String> {
  let leaf = state.peer_certificates()?.first()?;
  spki_sha256(leaf).map(|hash| encode_hex(&hash))
}

/// The negotiated ALPN protocol, `None` when none was agreed.
pub fn alpn_protocol(state: &CommonState) -> Option<String> {
  state.alpn_protocol().map(|p| String::from_utf8_lossy(p).into_owned())
}

/// Parse a pin from the lowercase (or uppercase) hex form `peer_key`
/// reports.
pub fn parse_pin(hex: &str) -> Result<[u8; PIN_LEN], String> {
  let invalid = || format!("pin must be {} hex characters (a SHA-256), got {hex:?}", PIN_LEN * 2);
  if hex.len() != PIN_LEN * 2 || !hex.is_ascii() {
    return Err(invalid());
  }
  let mut pin = [0u8; PIN_LEN];
  for (i, byte) in pin.iter_mut().enumerate() {
    *byte = u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).map_err(|_| invalid())?;
  }
  Ok(pin)
}

/// A failed handshake as a message (see `rustls_message`).
pub(crate) fn handshake_error(e: &std::io::Error) -> String {
  rustls_message(e).unwrap_or_else(|| e.to_string())
}

/// The message of an error that is, or is `io::Error`s around, a rustls
/// error of our own verifiers: rustls renders one through `Debug`
/// (`Other(OtherError(..))`), so it is rendered by its `Display` here.
/// `None` for any other error.
pub(crate) fn rustls_message(e: &(dyn std::error::Error + 'static)) -> Option<String> {
  let mut e = e;
  // The connectors wrap the rustls error in one io::Error each.
  while let Some(io) = e.downcast_ref::<std::io::Error>() {
    e = io.get_ref()?;
  }
  let tls = e.downcast_ref::<rustls::Error>()?;
  match tls {
    rustls::Error::InvalidCertificate(CertificateError::Other(OtherError(inner))) => {
      Some(format!("invalid peer certificate: {inner}"))
    }
    _ => None,
  }
}

/// An error and its sources as one message, each part once (a source
/// whose text the message already ends with adds nothing). A TLS error of
/// our verifiers is rendered by `rustls_message`, so a pin mismatch reads
/// the same through fetch, a socket or a relay.
#[cfg(any(feature = "http", feature = "p2p"))]
pub(crate) fn error_chain_message(e: &(dyn std::error::Error + 'static)) -> String {
  let mut message = rustls_message(e).unwrap_or_else(|| e.to_string());
  let mut source = e.source();
  while let Some(err) = source {
    let part = rustls_message(err).unwrap_or_else(|| err.to_string());
    if !message.ends_with(&part) {
      message.push_str(": ");
      message.push_str(&part);
    }
    source = err.source();
  }
  message
}

/// How many days a generated certificate is valid by default: 10 years, for
/// a LAN identity made once on first run.
pub const DEFAULT_CERT_VALID_DAYS: u32 = 3650;

/// What `generate_certificate` is asked for.
pub struct CertificateRequest<'a> {
  /// The subject alternative names: DNS names and IP addresses alike (a
  /// name that parses as an IP address is one). The first is also the
  /// subject's common name. At least one.
  pub names: &'a [String],
  /// How many days from now the certificate is valid; at least 1.
  pub valid_days: u32,
  /// Whether the certificate is a CA, which can be an `issuer`.
  pub is_ca: bool,
  /// The CA that signs it: the CA's certificate and its PKCS#8 private key,
  /// both PEM. `None` self-signs.
  pub issuer: Option<(&'a [u8], &'a [u8])>,
}

/// What `generate_certificate` makes.
pub struct GeneratedCertificate {
  /// The certificate (PEM).
  pub cert: String,
  /// Its private key (PKCS#8 PEM).
  pub key: String,
  /// The key's pin: `spki_sha256` of the certificate as lowercase hex, the
  /// form `peer_key` reports and `parse_pin` reads.
  pub pin: String,
}

/// How far before its issue a generated certificate is already valid: a
/// device whose clock runs behind the issuing one by less than this still
/// accepts it at once (public CAs backdate the same way).
const CERT_BACKDATE: time::Duration = time::Duration::hours(1);

/// Make a fresh ECDSA P-256 key and a certificate for it, valid from
/// `CERT_BACKDATE` before now for `valid_days`. Each name is an IP address
/// or a DNS name (`server_name`'s rules, with an optional leading `*.`
/// wildcard label). Synchronous on purpose: a P-256 key takes well under a
/// millisecond. Errors name the field at fault (`names`, `validDays`,
/// `issuer.cert`, `issuer.key`).
pub fn generate_certificate(req: &CertificateRequest) -> Result<GeneratedCertificate, String> {
  let Some(common_name) = req.names.first() else {
    return Err("names: at least one DNS name or IP address".to_string());
  };
  for name in req.names {
    if name.parse::<std::net::IpAddr>().is_ok() {
      continue;
    }
    let host = name.strip_prefix("*.").unwrap_or(name);
    if host.parse::<std::net::IpAddr>().is_ok() || server_name(host).is_err() {
      return Err(format!("names: not a DNS name or IP address: {name:?}"));
    }
  }
  if req.valid_days == 0 {
    return Err("validDays: at least one day".to_string());
  }
  let mut params = rcgen::CertificateParams::new(req.names.to_vec()).map_err(|e| format!("names: {e}"))?;
  let now = time::OffsetDateTime::now_utc();
  params.not_before = now - CERT_BACKDATE;
  params.not_after = now
    .checked_add(time::Duration::days(i64::from(req.valid_days)))
    .ok_or_else(|| format!("validDays: {} days is past the last date a certificate holds", req.valid_days))?;
  params.distinguished_name = rcgen::DistinguishedName::new();
  params.distinguished_name.push(rcgen::DnType::CommonName, common_name.as_str());
  if req.is_ca {
    params.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
  }
  let key = rcgen::KeyPair::generate().map_err(|e| format!("key: {e}"))?;
  let cert = match req.issuer {
    Some((cert, issuer_key)) => {
      let cert = std::str::from_utf8(cert).map_err(|_| "issuer.cert: not PEM text".to_string())?;
      let issuer_key = std::str::from_utf8(issuer_key).map_err(|_| "issuer.key: not PEM text".to_string())?;
      let issuer_key = rcgen::KeyPair::from_pem(issuer_key).map_err(|e| format!("issuer.key: {e}"))?;
      // rcgen signs with whatever key it is handed; a key that is not the
      // certificate's makes a leaf no client verifies, so check it here.
      let issuer_cert = certs_from_pem(cert.as_bytes(), "issuer.cert")?;
      let issuer_spki = ParsedCertificate::try_from(&issuer_cert[0])
        .map_err(|e| format!("issuer.cert: {e:?}"))?
        .subject_public_key_info();
      if rcgen::PublicKeyData::subject_public_key_info(&issuer_key) != issuer_spki.as_ref() {
        return Err("issuer.key: does not belong to issuer.cert".to_string());
      }
      let issuer = rcgen::Issuer::from_ca_cert_pem(cert, issuer_key).map_err(|e| format!("issuer.cert: {e}"))?;
      params.signed_by(&key, &issuer).map_err(|e| format!("issuer: {e}"))?
    }
    None => params.self_signed(&key).map_err(|e| format!("certificate: {e}"))?,
  };
  let pin = spki_sha256(cert.der()).ok_or_else(|| "certificate: does not parse".to_string())?;
  Ok(GeneratedCertificate { cert: cert.pem(), key: key.serialize_pem(), pin: encode_hex(&pin) })
}

fn encode_hex(bytes: &[u8]) -> String {
  bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The pin mismatch, as the handshake error reports it.
#[derive(Debug)]
struct PinMismatch;

impl std::fmt::Display for PinMismatch {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.write_str("the server's key does not match the pin")
  }
}

impl std::error::Error for PinMismatch {}

/// The pin check of `PinVerifier` and `HostVerifier`.
fn check_pin(pin: &[u8; PIN_LEN], end_entity: &CertificateDer<'_>) -> Result<ServerCertVerified, rustls::Error> {
  match spki_sha256(end_entity) {
    Some(hash) if hash == *pin => Ok(ServerCertVerified::assertion()),
    Some(_) => Err(CertificateError::Other(OtherError(Arc::new(PinMismatch))).into()),
    None => Err(CertificateError::BadEncoding.into()),
  }
}

/// How a host's server is checked, by `HostTrust`: by its key (`Some`, a
/// pin) or by its chain and name (`None`).
pub type HostCheck = Option<[u8; PIN_LEN]>;

/// The check of each host in a set of servers, shared between the owner
/// that adds hosts and the one verifier (`verifier`) that reads it at every
/// handshake. A host with a pin is checked by the pin alone; a host not
/// added, or added without a pin, by the chain and name against the roots.
/// Keyed by host, not by port: a rustls verifier sees the server name
/// only, so every server on one host is checked one way, and `add`
/// refuses a second, different check for a host. The verifier also
/// records the last certificate it rejected per host (`rejection`), cleared
/// when a certificate for that host passes: the one observation of a
/// failed check when the stack that dials reports none. Cheaply cloned
/// (shared tables).
#[derive(Clone, Default)]
pub struct HostTrust {
  hosts: Arc<RwLock<HashMap<String, HostCheck>>>,
  rejections: Arc<Mutex<HashMap<String, String>>>,
}

impl HostTrust {
  pub fn new() -> HostTrust {
    HostTrust::default()
  }

  /// Add `host` (a DNS name or an IP address) with its check. Adding a host
  /// again with the same check is a no-op; with a different one, an error
  /// naming what the host is checked by already.
  pub fn add(&self, host: &str, check: HostCheck) -> Result<(), String> {
    let key = host_key(&server_name(host)?);
    let mut hosts = self.hosts.write().expect("TLS host trust lock");
    match hosts.get(&key) {
      Some(known) if *known == check => Ok(()),
      Some(known) => {
        let by = match known {
          Some(_) => "another pin",
          None => "its certificate chain (no pin)",
        };
        Err(format!(
          "host {host} is already checked by {by}; one host is checked one way, so give its relays one \
           certificate or reach them by different host names"
        ))
      }
      None => {
        hosts.insert(key, check);
        Ok(())
      }
    }
  }

  /// Why the verifier last rejected a certificate of `host`, in the
  /// message form of connectTls and fetch (`rustls_message`); `None` when
  /// it has rejected none, or a certificate of the host passed since.
  pub fn rejection(&self, host: &str) -> Option<String> {
    let key = host_key(&server_name(host).ok()?);
    self.rejections.lock().expect("TLS host rejections lock").get(&key).cloned()
  }

  /// The verifier over this table: a host's pin, else the chain and name
  /// check against `roots`. Hosts added later are seen by it. Errors name
  /// `what`, the field the extra roots came from.
  pub fn verifier(
    &self,
    roots: Arc<RootCertStore>,
    provider: Arc<CryptoProvider>,
    what: &str,
  ) -> Result<Arc<dyn ServerCertVerifier>, String> {
    let roots = roots_verifier(roots, provider.clone(), what)?;
    Ok(Arc::new(HostVerifier { hosts: self.hosts.clone(), rejections: self.rejections.clone(), roots, provider }))
  }
}

/// The table key of a server name: a DNS name lowercased without a trailing
/// dot, an IP address in its canonical text form.
fn host_key(name: &ServerName<'_>) -> String {
  match name {
    ServerName::DnsName(dns) => dns.as_ref().trim_end_matches('.').to_ascii_lowercase(),
    ServerName::IpAddress(ip) => std::net::IpAddr::from(*ip).to_string(),
    other => format!("{other:?}"),
  }
}

/// Verifies by `HostTrust` (see the module doc).
#[derive(Debug)]
struct HostVerifier {
  hosts: Arc<RwLock<HashMap<String, HostCheck>>>,
  rejections: Arc<Mutex<HashMap<String, String>>>,
  roots: Arc<WebPkiServerVerifier>,
  provider: Arc<CryptoProvider>,
}

impl ServerCertVerifier for HostVerifier {
  fn verify_server_cert(
    &self,
    end_entity: &CertificateDer<'_>,
    intermediates: &[CertificateDer<'_>],
    server_name: &ServerName<'_>,
    ocsp_response: &[u8],
    now: UnixTime,
  ) -> Result<ServerCertVerified, rustls::Error> {
    let key = host_key(server_name);
    let pin = self.hosts.read().expect("TLS host trust lock").get(&key).copied().flatten();
    let verified = match pin {
      Some(pin) => check_pin(&pin, end_entity),
      None => self.roots.verify_server_cert(end_entity, intermediates, server_name, ocsp_response, now),
    };
    let mut rejections = self.rejections.lock().expect("TLS host rejections lock");
    match &verified {
      Ok(_) => {
        rejections.remove(&key);
      }
      Err(e) => {
        rejections.insert(key, rustls_message(e).unwrap_or_else(|| e.to_string()));
      }
    }
    verified
  }

  fn verify_tls12_signature(
    &self,
    message: &[u8],
    cert: &CertificateDer<'_>,
    dss: &DigitallySignedStruct,
  ) -> Result<HandshakeSignatureValid, rustls::Error> {
    verify_tls12_signature(message, cert, dss, &self.provider.signature_verification_algorithms)
  }

  fn verify_tls13_signature(
    &self,
    message: &[u8],
    cert: &CertificateDer<'_>,
    dss: &DigitallySignedStruct,
  ) -> Result<HandshakeSignatureValid, rustls::Error> {
    verify_tls13_signature(message, cert, dss, &self.provider.signature_verification_algorithms)
  }

  fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
    self.provider.signature_verification_algorithms.supported_schemes()
  }
}

/// Verifies the server by its key alone (see the module doc).
#[derive(Debug)]
struct PinVerifier {
  pin: [u8; PIN_LEN],
  provider: Arc<CryptoProvider>,
}

impl ServerCertVerifier for PinVerifier {
  fn verify_server_cert(
    &self,
    end_entity: &CertificateDer<'_>,
    _intermediates: &[CertificateDer<'_>],
    _server_name: &ServerName<'_>,
    _ocsp_response: &[u8],
    _now: UnixTime,
  ) -> Result<ServerCertVerified, rustls::Error> {
    check_pin(&self.pin, end_entity)
  }

  fn verify_tls12_signature(
    &self,
    message: &[u8],
    cert: &CertificateDer<'_>,
    dss: &DigitallySignedStruct,
  ) -> Result<HandshakeSignatureValid, rustls::Error> {
    verify_tls12_signature(message, cert, dss, &self.provider.signature_verification_algorithms)
  }

  fn verify_tls13_signature(
    &self,
    message: &[u8],
    cert: &CertificateDer<'_>,
    dss: &DigitallySignedStruct,
  ) -> Result<HandshakeSignatureValid, rustls::Error> {
    verify_tls13_signature(message, cert, dss, &self.provider.signature_verification_algorithms)
  }

  fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
    self.provider.signature_verification_algorithms.supported_schemes()
  }
}

/// Requires a client certificate and accepts any key (see the module doc).
#[derive(Debug)]
struct AnyClientKey {
  provider: Arc<CryptoProvider>,
}

impl ClientCertVerifier for AnyClientKey {
  fn root_hint_subjects(&self) -> &[DistinguishedName] {
    &[]
  }

  fn verify_client_cert(
    &self,
    end_entity: &CertificateDer<'_>,
    _intermediates: &[CertificateDer<'_>],
    _now: UnixTime,
  ) -> Result<ClientCertVerified, rustls::Error> {
    // Only a certificate whose key can be read: the app identifies the
    // client by that key.
    match ParsedCertificate::try_from(end_entity) {
      Ok(_) => Ok(ClientCertVerified::assertion()),
      Err(_) => Err(CertificateError::BadEncoding.into()),
    }
  }

  fn verify_tls12_signature(
    &self,
    message: &[u8],
    cert: &CertificateDer<'_>,
    dss: &DigitallySignedStruct,
  ) -> Result<HandshakeSignatureValid, rustls::Error> {
    verify_tls12_signature(message, cert, dss, &self.provider.signature_verification_algorithms)
  }

  fn verify_tls13_signature(
    &self,
    message: &[u8],
    cert: &CertificateDer<'_>,
    dss: &DigitallySignedStruct,
  ) -> Result<HandshakeSignatureValid, rustls::Error> {
    verify_tls13_signature(message, cert, dss, &self.provider.signature_verification_algorithms)
  }

  fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
    self.provider.signature_verification_algorithms.supported_schemes()
  }
}
