//! The `flux:p2p` module: peer-to-peer connectivity for flux, built on iroh.
//!
//! Marshalling only: decode JS args into the native types of the engine-free
//! `forge::p2p` core, drive its `Endpoint`/`Stream` methods, and encode the
//! results back to JS. The iroh-facing logic (binding, dial/accept, ticket
//! encoding, the read/writer mechanics) lives in `forge::p2p`.
//!
//! Surface (stage 1, deliberately minimal):
//! - `Endpoint.create(opts)` binds an iroh endpoint. Identity is a keypair; an
//!   ephemeral one is generated unless `secretKey` (64 hex chars) is supplied.
//!   `relayUrl` selects a self-hosted relay; `protocols` lists what it `accept`s.
//! - `endpoint.connect(peer, protocol)` dials a peer (by `ticket` or bare `id`)
//!   and opens one bidirectional stream.
//! - `endpoint.accept(protocol)` is an async-iterable of incoming streams.
//! - A `P2pStream` is a byte duplex: read with `for await (chunk of stream)`,
//!   write with `stream.write(bytes)`, end the send half with `stream.finish()`.
//!
//! "protocol" is the JS-facing name for the QUIC/iroh ALPN. Out of scope for
//! stage 1: unidirectional streams, multiple streams per peer, gossip/blobs, and
//! key persistence (the caller stores the `secretKey` getter value itself).
//!
//! The stream-building paths (`connect`, the `accept` iterator's `next`) keep a
//! hand-rolled `Promised` rather than `with_pending`: they must build a JS class
//! and spawn the writer task, so the future captures `Ctx`. They report errors
//! with `Exception::throw_message` (a clean `Error`, no `IO Error:` prefix), the
//! same clean rejection `with_pending`/`JsResult` give the other methods.

use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;

use rquickjs::class::Trace;
use rquickjs::function::Opt;
use rquickjs::module::{Declarations, Exports, ModuleDef};
use rquickjs::promise::Promised;
use rquickjs::{Array, Class, Ctx, Exception, Function, IntoJs, JsLifetime, Object, TypedArray, Value};

use iroh::endpoint::{Connection, RecvStream, SendStream};

use crate::forge::p2p::{decode_hex32, run_writer, ConnInfo, Endpoint, Stream};
use crate::logger::CtxLogger;
use crate::pending::PendingOps;
use crate::plugins::body::extract_body_value;
use crate::plugins::js_error::JsResult;
use crate::plugins::marshal::{attach_async_iterator, iter_result, with_pending};

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

#[rquickjs::methods]
impl P2pEndpoint {
  #[qjs(constructor)]
  pub fn new(ctx: Ctx<'_>) -> rquickjs::Result<P2pEndpoint> {
    Err(Exception::throw_message(&ctx, "use Endpoint.create() to bind a p2p endpoint"))
  }

  /// Bind an endpoint. `opts`: `{ secretKey?, relayUrl?, protocols? }`.
  /// `secretKey` is 64 hex chars (omit for an ephemeral key); `relayUrl` selects
  /// a self-hosted relay (omit for the public n0 relays); `protocols` lists the
  /// protocols this endpoint will `accept`.
  #[qjs(static)]
  pub fn create<'js>(
    ctx: Ctx<'js>,
    opts: Opt<Object<'js>>,
  ) -> rquickjs::Result<Promised<impl Future<Output = JsResult<P2pEndpoint>>>> {
    let (secret, relay_url, alpns) = parse_create_opts(&ctx, opts.0)?;
    Ok(with_pending(&ctx, async move {
      Endpoint::bind(secret, relay_url, alpns).await.map(|inner| P2pEndpoint { inner })
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
  pub fn ticket<'js>(
    &self,
    ctx: Ctx<'js>,
  ) -> rquickjs::Result<Promised<impl Future<Output = JsResult<String>>>> {
    let inner = self.inner.clone();
    Ok(with_pending(&ctx, async move { Ok::<String, String>(inner.ticket().await) }))
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
    let pending = ctx.userdata::<PendingOps>().expect("pending ops").clone();
    let ctx2 = ctx.clone();
    Ok(Promised(async move {
      pending.hold();
      let r = inner.connect(peer, protocol).await;
      pending.release();
      match r {
        Ok((conn, send, recv)) => P2pStream::create(&ctx2, conn, send, recv),
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
      let pending = ctx.userdata::<PendingOps>().expect("pending ops").clone();
      let ctx2 = ctx.clone();
      Ok(Promised(Box::pin(async move {
        pending.hold();
        let r = inner.accept_one(&alpn).await;
        pending.release();
        match r {
          Ok(Some((conn, send, recv))) => {
            let stream = P2pStream::create(&ctx2, conn, send, recv)?;
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
  ) -> rquickjs::Result<Promised<impl Future<Output = JsResult<JsConnInfo>>>> {
    let inner = self.inner.clone();
    Ok(with_pending(&ctx, async move { inner.conn_info(id).await.map(JsConnInfo) }))
  }

  /// Close the endpoint, ending any `accept` iteration.
  pub fn close<'js>(&self, ctx: Ctx<'js>) -> rquickjs::Result<Promised<impl Future<Output = JsResult<()>>>> {
    let inner = self.inner.clone();
    Ok(with_pending(&ctx, async move {
      inner.close().await;
      Ok::<(), String>(())
    }))
  }
}

/// A single bidirectional p2p stream: a thin JS wrapper over the forge stream
/// core. It is its own async iterator (`for await` reads the recv half).
#[derive(Trace, JsLifetime)]
#[rquickjs::class(rename = "P2pStream")]
pub struct P2pStream {
  #[qjs(skip_trace)]
  inner: Rc<Stream>,
}

impl P2pStream {
  /// Build the JS stream object: assemble the forge `Stream`, spawn its writer
  /// task (spawning is host-specific, so it stays in marshalling), and make the
  /// instance async-iterable.
  fn create<'js>(
    ctx: &Ctx<'js>,
    conn: Connection,
    send: SendStream,
    recv: RecvStream,
  ) -> rquickjs::Result<Class<'js, P2pStream>> {
    let (inner, rx) = Stream::new(conn, recv);
    let pending = ctx.userdata::<PendingOps>().expect("pending ops").clone();
    let logger = ctx.logger();
    pending.hold();
    ctx.spawn(async move {
      run_writer(send, rx, &logger).await;
      pending.release();
    });

    let instance = Class::instance(ctx.clone(), P2pStream { inner })?;
    attach_async_iterator(ctx, &instance)?;
    Ok(instance)
  }
}

#[rquickjs::methods]
impl P2pStream {
  /// Async-iterator step: resolve `{ value: Uint8Array, done: false }` for the
  /// next chunk, or `{ done: true }` at end-of-stream. Pull-based, so the
  /// transport only advances as JS iterates.
  pub fn next<'js>(
    &self,
    ctx: Ctx<'js>,
  ) -> rquickjs::Result<Promised<impl Future<Output = JsResult<ReadStep>>>> {
    let inner = self.inner.clone();
    Ok(with_pending(&ctx, async move { inner.read_chunk().await.map(ReadStep) }))
  }

  /// Queue bytes (string or Uint8Array) on the send half.
  pub fn write(&self, data: Value<'_>) -> rquickjs::Result<()> {
    let bytes = extract_body_value(&data, "P2pStream.write")?;
    self.inner.write(bytes);
    Ok(())
  }

  /// Finish the send half (QUIC FIN) after any queued writes flush. The recv
  /// half stays open for replies.
  pub fn finish(&self) -> rquickjs::Result<()> {
    self.inner.finish();
    Ok(())
  }

  /// Tear the stream down: finish the send half and stop reading.
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

/// One async-iterator step over a `P2pStream`: a chunk, or end-of-stream. Built
/// into the iterator-result object `{ value, done }` by `IntoJs`.
pub struct ReadStep(Option<Vec<u8>>);

impl<'js> IntoJs<'js> for ReadStep {
  fn into_js(self, ctx: &Ctx<'js>) -> rquickjs::Result<Value<'js>> {
    let value = match self.0 {
      Some(buf) => Some(TypedArray::<u8>::new(ctx.clone(), buf)?.into_value()),
      None => None,
    };
    Ok(iter_result(ctx, value)?.into_value())
  }
}

// Marshalling newtype over the engine-free `forge::p2p::ConnInfo`, so its
// `IntoJs` stays in this crate once forge is split out (a foreign `IntoJs` on a
// foreign type would otherwise trip the orphan rule). The `connInfo` call site
// `.map(JsConnInfo)`s the bare forge result.
// `pub` (not re-exported) only to satisfy `private_interfaces`: it appears in the
// rquickjs `#[methods]` return type of `conn_info`, which is a `pub fn`.
pub struct JsConnInfo(ConnInfo);

impl<'js> IntoJs<'js> for JsConnInfo {
  fn into_js(self, ctx: &Ctx<'js>) -> rquickjs::Result<Value<'js>> {
    let obj = Object::new(ctx.clone())?;
    let addrs = Array::new(ctx.clone())?;
    for (i, entry) in self.0.addrs.into_iter().enumerate() {
      let e = Object::new(ctx.clone())?;
      e.set("kind", entry.kind)?;
      e.set("addr", entry.addr)?;
      e.set("active", entry.active)?;
      addrs.set(i, e)?;
    }
    obj.set("path", self.0.path)?;
    obj.set("addrs", addrs)?;
    Ok(obj.into_value())
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

/// Parsed `create` options: `(secretKey bytes, relayUrl, protocols/alpns)`, the
/// native config `Endpoint::bind` takes.
type CreateOpts = (Option<[u8; 32]>, Option<String>, Vec<Vec<u8>>);

/// Parse the `create` options object into native config. `opts` may be absent.
fn parse_create_opts<'js>(ctx: &Ctx<'js>, opts: Option<Object<'js>>) -> rquickjs::Result<CreateOpts> {
  let Some(opts) = opts else {
    return Ok((None, None, Vec::new()));
  };
  let secret = match opts.get::<_, Option<String>>("secretKey")? {
    Some(s) => Some(decode_hex32(&s).map_err(|m| Exception::throw_message(ctx, &m))?),
    None => None,
  };
  let relay_url = opts.get::<_, Option<String>>("relayUrl")?;
  let alpns = opts
    .get::<_, Option<Vec<String>>>("protocols")?
    .unwrap_or_default()
    .into_iter()
    .map(String::into_bytes)
    .collect();
  Ok((secret, relay_url, alpns))
}