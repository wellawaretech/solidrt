//! Engine-free peer-to-peer core: the halves of `flux:p2p`, each behind its
//! own feature. `p2p` is the endpoint over iroh (`endpoint.rs`: dial, accept,
//! tickets, streams); `p2p-relay` is the self-hosted relay over iroh-relay
//! (`relay.rs`), which needs no endpoint of its own: an app that serves the
//! relay for others need not be a peer itself.

#[cfg(feature = "p2p")]
mod endpoint;
#[cfg(feature = "p2p")]
pub use endpoint::*;

#[cfg(feature = "p2p-relay")]
pub mod relay;
#[cfg(feature = "p2p-relay")]
pub use relay::Relay;

/// Lowercase hex of `bytes`: a secret key's text form, and the relay's token.
pub(crate) fn encode_hex(bytes: &[u8]) -> String {
  bytes.iter().map(|b| format!("{b:02x}")).collect()
}
