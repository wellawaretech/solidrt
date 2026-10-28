//! The `flux:p2p` module: peer-to-peer connectivity for flux, built on iroh.
//!
//! Marshalling only: decode JS args into the native types of the engine-free
//! `forge::p2p` core, drive its `Endpoint`/`Stream` methods, and encode the
//! results back to JS. The iroh-facing logic (binding, dial/accept, ticket
//! encoding, the read/write mechanics) lives in `forge::p2p`.
//!
//! Surface (stage 1, deliberately minimal):
//! - `Endpoint.create(opts)` binds an iroh endpoint. Identity is a keypair; an
//!   ephemeral one is generated unless `secretKey` (64 hex chars) is supplied.
//!   `relayUrl` selects a self-hosted relay (no public relay or address lookup is
//!   ever used); `protocols` lists what it `accept`s.
//! - `endpoint.connect(peer, protocol)` dials a peer (by `ticket` or bare `id`)
//!   and opens one bidirectional stream.
//! - `endpoint.accept(protocol)` is an async-iterable of incoming streams.
//! - A `P2pStream` is a byte duplex as the web shapes it: `stream.readable`
//!   reads, `stream.writable` writes, `writable.close()` ends the send half
//!   (QUIC FIN, the recv half stays open), `writable.abort(reason)` resets it
//!   and errors the stream, `stream.close()` tears it down cleanly.
//! - `serveRelay(opts)` (the `p2p-relay` feature) serves the self-hosted
//!   relay from this process; its `url` is what peers pass as `relayUrl`.
//!
//! "protocol" is the JS-facing name for the QUIC/iroh ALPN. Out of scope for
//! stage 1: unidirectional streams, multiple streams per peer, gossip/blobs, and
//! key persistence (the caller stores the `secretKey` getter value itself).
//!
//! Each half is its own feature: `p2p` the endpoint (`endpoint.rs`), and
//! `p2p-relay` the relay (`relay.rs`). An app that serves the relay for others
//! need not be a peer, so a relay-only build exports `serveRelay` alone.

use rquickjs::module::{Declarations, Exports, ModuleDef};
#[cfg(feature = "p2p")]
use rquickjs::Class;
use rquickjs::Ctx;
#[cfg(feature = "p2p-relay")]
use rquickjs::Function;

#[cfg(feature = "p2p")]
mod endpoint;
#[cfg(feature = "p2p")]
pub use endpoint::P2pEndpoint;

#[cfg(feature = "p2p-relay")]
mod relay;

pub struct P2pModule;

impl ModuleDef for P2pModule {
  fn declare(decl: &Declarations<'_>) -> rquickjs::Result<()> {
    #[cfg(feature = "p2p")]
    decl.declare("Endpoint")?;
    #[cfg(feature = "p2p-relay")]
    decl.declare("serveRelay")?;
    Ok(())
  }

  fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> rquickjs::Result<()> {
    #[cfg(feature = "p2p")]
    {
      let ctor = Class::<P2pEndpoint>::create_constructor(ctx)?.expect("Endpoint class has a constructor");
      exports.export("Endpoint", ctor)?;
    }
    #[cfg(feature = "p2p-relay")]
    exports.export("serveRelay", Function::new(ctx.clone(), relay::serve_relay)?)?;
    Ok(())
  }
}
