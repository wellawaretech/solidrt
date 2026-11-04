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
//! - Any client key (`AnyClientKey`): a client certificate is required, any
//!   key is accepted, and the app checks the peer's key itself (pairing).
//!   The signatures are verified here too.

use std::sync::Arc;

use sha2::{Digest, Sha256};
use tokio_rustls::rustls;
use tokio_rustls::rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use tokio_rustls::rustls::crypto::{verify_tls12_signature, verify_tls13_signature, CryptoProvider};
use tokio_rustls::rustls::pki_types::pem::PemObject;
use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime};
use tokio_rustls::rustls::server::danger::{ClientCertVerified, ClientCertVerifier};
use tokio_rustls::rustls::server::{ParsedCertificate, WebPkiClientVerifier};
use tokio_rustls::rustls::{
  CertificateError, CommonState, DigitallySignedStruct, DistinguishedName, OtherError, RootCertStore, SignatureScheme,
};

pub use tokio_rustls::rustls::{ClientConfig, ServerConfig};

/// The length of a pin and of a peer key: a SHA-256 hash.
pub const PIN_LEN: usize = 32;

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

fn add_roots(roots: &mut RootCertStore, blobs: &[Vec<u8>]) -> Result<(), String> {
  for blob in blobs {
    for cert in certs_from_pem(blob, "caCerts")? {
      roots.add(cert).map_err(|e| format!("caCerts: {e}"))?;
    }
  }
  Ok(())
}

fn alpn_bytes(alpn: &[String]) -> Vec<Vec<u8>> {
  alpn.iter().map(|p| p.as_bytes().to_vec()).collect()
}

/// Build a client config: the webpki roots plus `ca_certs`, or the pin; an
/// optional client certificate; the ALPN list.
pub fn client_config(opts: &ClientOptions) -> Result<Arc<ClientConfig>, String> {
  let provider = provider();
  let builder = ClientConfig::builder_with_provider(provider.clone())
    .with_safe_default_protocol_versions()
    .map_err(|e| e.to_string())?;
  let builder = match opts.pin {
    Some(pin) => {
      if !opts.ca_certs.is_empty() {
        return Err("pin and caCerts exclude each other: a pin replaces the chain check".to_string());
      }
      builder.dangerous().with_custom_certificate_verifier(Arc::new(PinVerifier { pin, provider }))
    }
    None => {
      let mut roots = RootCertStore { roots: webpki_roots::TLS_SERVER_ROOTS.to_vec() };
      add_roots(&mut roots, &opts.ca_certs)?;
      builder.with_root_certificates(roots)
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
  Ok(Arc::new(config))
}

/// Build a server config: the identity, the client authentication, the
/// ALPN list.
pub fn server_config(opts: &ServerOptions) -> Result<Arc<ServerConfig>, String> {
  let provider = provider();
  let builder = ServerConfig::builder_with_provider(provider.clone())
    .with_safe_default_protocol_versions()
    .map_err(|e| e.to_string())?;
  let builder = match &opts.client_auth {
    ClientAuth::None => builder.with_no_client_auth(),
    ClientAuth::Chain(blobs) => {
      let mut roots = RootCertStore::empty();
      add_roots(&mut roots, blobs)?;
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
    match spki_sha256(end_entity) {
      Some(hash) if hash == self.pin => Ok(ServerCertVerified::assertion()),
      Some(_) => Err(CertificateError::Other(OtherError(Arc::new(PinMismatch))).into()),
      None => Err(CertificateError::BadEncoding.into()),
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
