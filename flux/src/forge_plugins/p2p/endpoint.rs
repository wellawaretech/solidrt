//! `Endpoint` and `P2pStream`: the endpoint half of `flux:p2p` (the `p2p`
//! feature), marshalled over `forge::p2p::Endpoint` and `Stream`.
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
use rquickjs::promise::Promised;
use rquickjs::{Class, Ctx, Exception, Function, IntoJs, JsLifetime, Object};

use crate::pending::PendingOps;
use crate::plugins::js_error::JsResult;
use crate::plugins::marshal::{attach_async_iterator, iter_result, with_in_flight, OptArg};
use crate::plugins::value::Neutral;
use crate::standards_plugins::body::byte_stream_readable;
use crate::standards_plugins::streams::writable_to;
use forge::p2p::{
  decode_hex32, parse_ticket as parse_ticket_core, ConnectOptions, Endpoint, Stream, TicketAddresses, TicketOptions,
};

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
  #[cfg(feature = "http")]
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

  /// Bind an endpoint. `opts`: `{ secretKey?, relayUrl?, relayCaCerts?, protocols?, port?, paths? }`.
  /// `secretKey` is 64 hex chars (omit for an ephemeral key); `relayUrl`
  /// selects a self-hosted relay (omit for none of its own: direct paths,
  /// plus the relays `connect` adopts from tickets);
  /// `protocols` lists the protocols this endpoint will `accept`. `port` pins
  /// the UDP bind port (IPv4-only) so the ticket stays stable across restarts;
  /// omit for an ephemeral port. `paths` is `"any"` (the default) or
  /// `"relay"` (no UDP socket; every packet through the relay).
  /// `relayCaCerts` are extra PEM roots for the relays' certificates (a
  /// `relayUrl` with `?pin=` is checked by the pin instead). See
  /// `forge::p2p::Endpoint::bind`.
  #[qjs(static)]
  pub fn create<'js>(
    ctx: Ctx<'js>,
    opts: OptArg<Object<'js>>,
  ) -> rquickjs::Result<Promised<impl Future<Output = JsResult<P2pEndpoint>>>> {
    let (secret, relay_url, alpns, port, relay_only, relay_ca_certs) = parse_create_opts(&ctx, opts.0)?;
    Ok(with_in_flight(&ctx, "p2p", async move {
      Endpoint::bind(secret, relay_url, alpns, port, relay_only, relay_ca_certs)
        .await
        .map(|inner| P2pEndpoint { inner })
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

  /// `ticket(opts?: { relay?, addresses? })`: a self-contained dial token
  /// carrying this endpoint's id, configured relay, and direct addresses, so
  /// a peer can `connect` without relying on discovery. Rejects when the
  /// configured relay is not connected after the wait; `relay: false` writes
  /// no relay (and rejects on a `paths: "relay"` endpoint); `addresses` is
  /// `"auto"` (the default) or `"all"`, any other value throws.
  /// See `forge::p2p::Endpoint::ticket`.
  pub fn ticket<'js>(
    &self,
    ctx: Ctx<'js>,
    opts: OptArg<Object<'js>>,
  ) -> rquickjs::Result<Promised<impl Future<Output = JsResult<String>>>> {
    let mut options = TicketOptions::default();
    if let Some(opts) = opts.0 {
      if let Some(relay) = opts.get::<_, Option<bool>>("relay")? {
        options.relay = relay;
      }
      if let Some(addresses) = opts.get::<_, Option<String>>("addresses")? {
        options.addresses = TicketAddresses::parse(&addresses).map_err(|m| Exception::throw_message(&ctx, &m))?;
      }
    }
    let inner = self.inner.clone();
    Ok(with_in_flight(&ctx, "p2p", async move { inner.ticket(options).await }))
  }

  /// The configured relay's state, `{ url, connected, error? }`, or `null`
  /// without a `relayUrl`. See `forge::p2p::Endpoint::relay_status`.
  #[qjs(rename = "relayStatus")]
  pub fn relay_status(&self) -> Neutral {
    Neutral(self.inner.relay_status().into())
  }

  /// The relays this endpoint knows, `[{ url, pin?, configured, home }]`. See
  /// `forge::p2p::Endpoint::relays`.
  pub fn relays(&self) -> Neutral {
    Neutral(forge::Value::list(self.inner.relays()))
  }

  /// Dial a peer and open one bidirectional stream over `protocol`. `peer` is
  /// either a `ticket` (preferred: connects directly, no discovery) or a bare
  /// endpoint `id` (needs discovery to resolve the peer's address). A
  /// ticket's relay unknown to this endpoint is adopted for the dial unless
  /// `opts.adoptRelay` is false; see `forge::p2p::Endpoint::connect`.
  pub fn connect<'js>(
    &self,
    ctx: Ctx<'js>,
    peer: String,
    protocol: String,
    opts: OptArg<Object<'js>>,
  ) -> rquickjs::Result<Promised<impl Future<Output = rquickjs::Result<Class<'js, P2pStream>>>>> {
    let mut options = ConnectOptions::default();
    if let Some(opts) = opts.0 {
      if let Some(adopt) = opts.get::<_, Option<bool>>("adoptRelay")? {
        options.adopt_relay = adopt;
      }
    }
    let inner = self.inner.clone();
    let hold = PendingOps::of(&ctx).in_flight("p2p connect");
    let ctx2 = ctx.clone();
    Ok(Promised(async move {
      let r = inner.connect(peer, protocol, options).await;
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

/// `parseTicket(ticket)` -> `{ id, relayUrl, addresses }`; throws on a ticket
/// that does not parse. See `forge::p2p::parse_ticket`.
pub fn parse_ticket(ctx: Ctx<'_>, ticket: String) -> rquickjs::Result<Neutral> {
  parse_ticket_core(&ticket).map(|t| Neutral(t.into())).map_err(|m| Exception::throw_message(&ctx, &m))
}

/// Parsed `create` options: `(secretKey bytes, relayUrl, protocols/alpns,
/// bind port, relay-only, relayCaCerts PEM blobs)`, the native config
/// `Endpoint::bind` takes.
type CreateOpts = (Option<[u8; 32]>, Option<String>, Vec<Vec<u8>>, Option<u16>, bool, Vec<Vec<u8>>);

/// Parse the `create` options object into native config. `opts` may be absent.
fn parse_create_opts<'js>(ctx: &Ctx<'js>, opts: Option<Object<'js>>) -> rquickjs::Result<CreateOpts> {
  let Some(opts) = opts else {
    return Ok((None, None, Vec::new(), None, false, Vec::new()));
  };
  let secret = match opts.get::<_, Option<String>>("secretKey")? {
    Some(s) => Some(decode_hex32(&s).map_err(|m| Exception::throw_message(ctx, &m))?),
    None => None,
  };
  let relay_url = opts.get::<_, Option<String>>("relayUrl")?;
  let alpns =
    opts.get::<_, Option<Vec<String>>>("protocols")?.unwrap_or_default().into_iter().map(String::into_bytes).collect();
  let port = opts.get::<_, Option<u16>>("port")?;
  let relay_only = match opts.get::<_, Option<String>>("paths")?.as_deref() {
    None | Some("any") => false,
    Some("relay") => true,
    Some(other) => {
      return Err(Exception::throw_message(ctx, &format!("paths must be \"any\" or \"relay\", not \"{other}\"")))
    }
  };
  // PEM blobs as `caCerts` of `tls_client_options` (flux:net) reads them;
  // forge parses and validates them at bind.
  let relay_ca_certs = opts
    .get::<_, Option<Vec<String>>>("relayCaCerts")?
    .unwrap_or_default()
    .into_iter()
    .map(String::into_bytes)
    .collect();
  Ok((secret, relay_url, alpns, port, relay_only, relay_ca_certs))
}
