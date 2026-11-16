//! Engine-free peer-to-peer core: the halves of `flux:p2p`, each behind its
//! own feature. `p2p` is the endpoint over iroh (`endpoint.rs`: dial, accept,
//! tickets, streams); `p2p-relay` is the self-hosted relay over iroh-relay
//! (`relay.rs`), which needs no endpoint of its own: an app that serves the
//! relay for others need not be a peer itself. Both read the default-route
//! interface the same way (`default_route_ips`), so the relay's URL and a
//! ticket's address never disagree on it.

use std::net::IpAddr;

#[cfg(feature = "p2p")]
mod endpoint;
#[cfg(feature = "p2p")]
pub use endpoint::*;

#[cfg(feature = "p2p-relay")]
pub mod relay;
#[cfg(feature = "p2p-relay")]
pub use relay::{Relay, RelayOptions, RelaySetup, RelayTls};

/// The one query parameter a relay URL takes: the SHA-256 of the relay's
/// public key, 64 hex (`tls::parse_pin`). The relay writes it into its URL,
/// the endpoint reads it from `relayUrl` and tickets.
pub(crate) const PIN_PARAM: &str = "pin";

/// What separates a ticket's fields (`id|relay|ips`). A relay URL that
/// contains it would split its ticket wrongly, so neither the relay's `url`
/// nor an endpoint's `relayUrl` may.
pub(crate) const TICKET_SEPARATOR: char = '|';

/// Why an HTTPS relay URL with an IPv6 address is refused, by the relay and
/// by the endpoint: the relay client (iroh-relay 1.4.1) names the server for
/// TLS by the URL's host text, brackets included, which rustls refuses.
pub(crate) const IPV6_HTTPS_UNREACHABLE: &str =
  "an HTTPS relay URL with an IPv6 address is unreachable (the relay client cannot check its certificate)";

/// Lowercase hex of `bytes`: a secret key's text form, and the relay's token.
pub(crate) fn encode_hex(bytes: &[u8]) -> String {
  bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The IP addresses of the default-route interface in `state`; `None`
/// when there is no default route, or its interface is not in the state.
/// The endpoint's ticket and the relay's URL both read the default route
/// here.
pub(crate) fn default_route_ips(state: &netwatch::interfaces::State) -> Option<Vec<IpAddr>> {
  let name = state.default_route_interface.as_ref()?;
  let interface = state.interfaces.get(name)?;
  Some(interface.addrs().map(|net| net.addr()).collect())
}
