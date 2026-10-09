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
//!   `relayUrl` selects a self-hosted relay; `protocols` lists what it `accept`s.
//! - `endpoint.connect(peer, protocol)` dials a peer (by `ticket` or bare `id`)
//!   and opens one bidirectional stream.
//! - `endpoint.accept(protocol)` is an async-iterable of incoming streams.
//! - A `P2pStream` is a byte duplex as the web shapes it: `stream.readable`
//!   reads, `stream.writable` writes, `writable.close()` ends the send half
//!   (QUIC FIN, the recv half stays open), `writable.abort(reason)` resets it
//!   and errors the stream, `stream.close()` tears it down cleanly.
//!
//! "protocol" is the JS-facing name for the QUIC/iroh ALPN. Out of scope for
//! stage 1: unidirectional streams, multiple streams per peer, gossip/blobs, and
//! key persistence (the caller stores the `secretKey` getter value itself).
//!
//! The stream-building paths (`connect`, the `accept` iterator's `next`) keep a
//! hand-rolled `Promised` rather than `with_in_flight`: they must build a JS
//! class, so the future captures `Ctx`. They report errors
//! with `Exception::throw_message` (a clean `Error`, no `IO Error:` prefix), the
//! same clean rejection `with_in_flight`/`JsResult` give the other methods.

use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

use rquickjs::class::Trace;
use rquickjs::module::{Declarations, Exports, ModuleDef};
use rquickjs::promise::Promised;
use rquickjs::{Class, Ctx, Exception, Function, IntoJs, JsLifetime, Object};

use crate::pending::PendingOps;
use crate::plugins::js_error::JsResult;
use crate::plugins::marshal::{attach_async_iterator, iter_result, with_in_flight, OptArg};
use crate::plugins::value::Neutral;
use crate::standards_plugins::body::byte_stream_readable;
use crate::standards_plugins::streams::writable_to;
use forge::p2p::{decode_hex32, Endpoint, Stream};

/// `next()` of the `accept` async-iterable: a promise resolving to an iterator
/// result object (boxed so the closure has a nameable return type).
type AcceptStep<'js> = Promised<Pin<Box<dyn Future<Output = rquickjs::Result<Object<'js>>> + 'js>>>;

/// The `flux:p2p` `Endpoint`: a thin JS wrapper over the forge endpoint core.
#[derive(Trace, JsLifetime)]
#[rquickjs::class(rename = "Endpoint")]
pub struct P2pEndpoint {
  #[qjs(skip_trace)]
  inner: Endpoint,
}

impl P2pEndpoint {
  /// The engine-free forge endpoint, cloned out for native consumers (the
  /// flux:http serve `endpoint` option accepts connections on it).
  pub(crate) fn core(&self) -> Endpoint {
    self.inner.clone()
  }
}

#[rquickjs::methods]
impl P2pEndpoint {
  #[qjs(constructor)]
  pub fn new(ctx: Ctx<'_>) -> rquickjs::Result<P2pEndpoint> {
    Err(Exception::throw_message(&ctx, "use Endpoint.create() to bind a p2p endpoint"))
  }

  /// Bind an endpoint. `opts`: `{ secretKey?, relayUrl?, protocols?, local?,
  /// port? }`. `secretKey` is 64 hex chars (omit for an ephemeral key);
  /// `relayUrl` selects a self-hosted relay (omit for the public n0 relays);
  /// `protocols` lists the protocols this endpoint will `accept`. `local: true`
  /// binds local-only: no relay and no address publishing/lookup - nothing
  /// leaves the machine except the ticket itself, whose direct IPs same-network
  /// peers dial. Excludes `relayUrl`; a bare-id `connect` cannot resolve a local
  /// endpoint (tickets only). `port` pins the UDP bind port (IPv4-only) so the
  /// ticket stays stable across restarts; omit for an ephemeral port.
  #[qjs(static)]
  pub fn create<'js>(
    ctx: Ctx<'js>,
    opts: OptArg<Object<'js>>,
  ) -> rquickjs::Result<Promised<impl Future<Output = JsResult<P2pEndpoint>>>> {
    let (secret, relay_url, alpns, local, port) = parse_create_opts(&ctx, opts.0)?;
    Ok(with_in_flight(&ctx, "p2p", async move {
      Endpoint::bind(secret, relay_url, alpns, local, port).await.map(|inner| P2pEndpoint { inner })
    }))
  }

  /// This endpoint's dial address: the string peers pass to `connect`.
  #[qjs(get)]
  pub fn id(&self) -> String {
    self.inner.id()
  }

  /// The secret key as 64 hex chars, for the caller to persist and feed back to
  /// `create` to keep a stable identity across restarts.
  #[qjs(get, rename = "secretKey")]
  pub fn secret_key(&self) -> String {
    self.inner.secret_key_hex()
  }

  /// A self-contained dial token carrying this endpoint's id, home relay, and
  /// direct addresses, so a peer can `connect` without relying on discovery.
  pub fn ticket<'js>(&self, ctx: Ctx<'js>) -> rquickjs::Result<Promised<impl Future<Output = JsResult<String>>>> {
    let inner = self.inner.clone();
    Ok(with_in_flight(&ctx, "p2p", async move { Ok::<String, String>(inner.ticket().await) }))
  }

  /// Dial a peer and open one bidirectional stream over `protocol`. `peer` is
  /// either a `ticket` (preferred: connects directly, no discovery) or a bare
  /// endpoint `id` (needs discovery to resolve the peer's address).
  pub fn connect<'js>(
    &self,
    ctx: Ctx<'js>,
    peer: String,
    protocol: String,
  ) -> rquickjs::Result<Promised<impl Future<Output = rquickjs::Result<Class<'js, P2pStream>>>>> {
    let inner = self.inner.clone();
    let hold = PendingOps::of(&ctx).in_flight("p2p connect");
    let ctx2 = ctx.clone();
    Ok(Promised(async move {
      let r = inner.connect(peer, protocol).await;
      drop(hold);
      match r {
        Ok(stream) => P2pStream::create(&ctx2, stream),
        Err(msg) => Err(Exception::throw_message(&ctx2, &msg)),
      }
    }))
  }

  /// An async-iterable of incoming streams whose protocol matches `protocol`.
  /// Iterating ends (`done`) when the endpoint is closed.
  pub fn accept<'js>(&self, ctx: Ctx<'js>, protocol: String) -> rquickjs::Result<Object<'js>> {
    let inner = self.inner.clone();
    let alpn = Rc::new(protocol.into_bytes());
    let iter = Object::new(ctx.clone())?;

    let next_fn = Function::new(ctx.clone(), move |ctx: Ctx<'js>| -> rquickjs::Result<AcceptStep<'js>> {
      let inner = inner.clone();
      let alpn = alpn.clone();
      // A pending accept waits on a peer: standing, like the endpoint.
      let hold = PendingOps::of(&ctx).standing("p2p accept");
      let ctx2 = ctx.clone();
      Ok(Promised(Box::pin(async move {
        let r = inner.accept_one(&alpn).await;
        drop(hold);
        match r {
          Ok(Some(stream)) => {
            let stream = P2pStream::create(&ctx2, stream)?;
            iter_result(&ctx2, Some(stream.into_js(&ctx2)?))
          }
          Ok(None) => iter_result(&ctx2, None),
          Err(msg) => Err(Exception::throw_message(&ctx2, &msg)),
        }
      })))
    })?;
    iter.set("next", next_fn)?;
    attach_async_iterator(&ctx, &iter)?;
    Ok(iter)
  }

  /// Snapshot of how the connection to `id` is currently carried. Resolves to
  /// `{ path, addrs }`; see `forge::p2p::ConnInfo`. iroh starts on the relay and
  /// upgrades to direct after hole-punching, so poll this to watch it settle.
  #[qjs(rename = "connInfo")]
  pub fn conn_info<'js>(
    &self,
    ctx: Ctx<'js>,
    id: String,
  ) -> rquickjs::Result<Promised<impl Future<Output = JsResult<Neutral>>>> {
    let inner = self.inner.clone();
    Ok(with_in_flight(&ctx, "p2p", async move { inner.conn_info(id).await.map(|c| Neutral(c.into())) }))
  }

  /// Close the endpoint, ending any `accept` iteration.
  pub fn close<'js>(&self, ctx: Ctx<'js>) -> rquickjs::Result<Promised<impl Future<Output = JsResult<()>>>> {
    let inner = self.inner.clone();
    Ok(with_in_flight(&ctx, "p2p", async move {
      inner.close().await;
      Ok::<(), String>(())
    }))
  }
}

/// A single bidirectional p2p stream: a byte duplex as a `readable`/`writable`
/// pair of web streams over the forge `Stream`, plus `close()` and `remoteId`.
#[derive(Trace, JsLifetime)]
#[rquickjs::class(rename = "P2pStream")]
pub struct P2pStream {
  #[qjs(skip_trace)]
  inner: Rc<Stream>,
}

impl P2pStream {
  /// Build the JS stream object over the forge `Stream`: the class instance
  /// with its `readable` (the recv half, a read waiting on the peer held as
  /// standing) and `writable` (the send half; a write is work in flight) as
  /// own properties. The streams capture only the shared `Rc` of the core.
  fn create<'js>(ctx: &Ctx<'js>, stream: Stream) -> rquickjs::Result<Class<'js, P2pStream>> {
    let inner = Rc::new(stream);
    let pending = PendingOps::of(ctx);
    let readable = byte_stream_readable(ctx, inner.readable(), move || pending.standing("p2p stream read"))?;
    let writable = writable_to(
      ctx,
      "P2pStream.writable",
      "p2p write",
      {
        let stream = inner.clone();
        move |bytes| {
          let stream = stream.clone();
          Box::pin(async move { stream.write(bytes).await })
        }
      },
      {
        let stream = inner.clone();
        move || {
          let stream = stream.clone();
          Box::pin(async move { stream.finish().await })
        }
      },
      {
        let stream = inner.clone();
        move |reason| {
          stream.abort(reason);
          Box::pin(std::future::ready(Ok(())))
        }
      },
    )?;
    let instance = Class::instance(ctx.clone(), P2pStream { inner })?;
    instance.set("readable", readable)?;
    instance.set("writable", writable)?;
    Ok(instance)
  }
}

#[rquickjs::methods]
impl P2pStream {
  /// Tear the stream down: a pending read ends, the recv half is released and
  /// the send half finished.
  pub fn close(&self) -> rquickjs::Result<()> {
    self.inner.close();
    Ok(())
  }

  /// The remote peer's endpoint id.
  #[qjs(get, rename = "remoteId")]
  pub fn remote_id(&self) -> String {
    self.inner.remote_id()
  }
}

pub struct P2pModule;

impl ModuleDef for P2pModule {
  fn declare(decl: &Declarations<'_>) -> rquickjs::Result<()> {
    decl.declare("Endpoint")?;
    Ok(())
  }

  fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> rquickjs::Result<()> {
    let ctor = Class::<P2pEndpoint>::create_constructor(ctx)?.expect("Endpoint class has a constructor");
    exports.export("Endpoint", ctor)?;
    Ok(())
  }
}

/// Parsed `create` options: `(secretKey bytes, relayUrl, protocols/alpns,
/// local, bind port)`, the native config `Endpoint::bind` takes.
type CreateOpts = (Option<[u8; 32]>, Option<String>, Vec<Vec<u8>>, bool, Option<u16>);

/// Parse the `create` options object into native config. `opts` may be absent.
fn parse_create_opts<'js>(ctx: &Ctx<'js>, opts: Option<Object<'js>>) -> rquickjs::Result<CreateOpts> {
  let Some(opts) = opts else {
    return Ok((None, None, Vec::new(), false, None));
  };
  let secret = match opts.get::<_, Option<String>>("secretKey")? {
    Some(s) => Some(decode_hex32(&s).map_err(|m| Exception::throw_message(ctx, &m))?),
    None => None,
  };
  let relay_url = opts.get::<_, Option<String>>("relayUrl")?;
  let alpns =
    opts.get::<_, Option<Vec<String>>>("protocols")?.unwrap_or_default().into_iter().map(String::into_bytes).collect();
  let local = opts.get::<_, Option<bool>>("local")?.unwrap_or(false);
  let port = opts.get::<_, Option<u16>>("port")?;
  Ok((secret, relay_url, alpns, local, port))
}
