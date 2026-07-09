//! The `flux:net` module: unprivileged TCP/UDP sockets and interface listing.
//!
//! Marshalling only: decode JS args into the engine-free `forge::net` core, drive
//! its calls, and encode the results back to JS. The socket mechanics live in
//! `forge::net`; this layer adds the JS surface, the async-iterable `Conn` /
//! `Listener`, and the byte (`Uint8Array`) conversions.
//!
//! Surface:
//! - `probe(host, port, opts?)` -> `"open" | "closed" | "filtered"`. The connect-
//!   scan primitive: `closed` (a refusal) still means the host is up.
//! - `connect(host, port, opts?)` -> `Conn`, a byte duplex: `for await (chunk of
//!   conn)` reads, `await conn.write(bytes)` writes, `conn.close()` ends it.
//! - `listen(port, opts?)` -> `Listener`, an async-iterable of incoming `Conn`s.
//! - `udp(opts?)` -> `Udp`: `send` / `recv` plus the broadcast / multicast knobs
//!   the peer beacon needs.
//! - `interfaces()` -> the local interfaces (name / mac / flags / addrs), the
//!   no-subprocess replacement for parsing `ip addr` to find the subnet to scan.
//!
//! Names follow flux idioms (factory functions, `timeoutMs`, async-iterables,
//! `Uint8Array`); the socket-option setters borrow Node/BSD vocabulary
//! (`setBroadcast`, `setMulticastTtl`, ...) where that is the expected term.
//!
//! The `connect` / `listen` / `udp` factories keep a hand-rolled `Promised`
//! rather than `with_pending`: they must build a JS class, so the future captures
//! `Ctx`. They reject with `Exception::throw_message` (a clean `Error`, no
//! `IO Error:` prefix), the same clean rejection `with_pending` gives the rest.

use std::future::Future;
use std::net::Ipv4Addr;
use std::rc::Rc;

use rquickjs::class::Trace;
use rquickjs::function::Opt;
use rquickjs::module::{Declarations, Exports, ModuleDef};
use rquickjs::promise::Promised;
use rquickjs::{Array, Class, Ctx, Exception, FromJs, Function, IntoJs, JsLifetime, Object, TypedArray, Value};

use crate::pending::PendingOps;
use crate::plugins::js_error::JsResult;
use crate::plugins::marshal::{attach_async_iterator, iter_result, with_pending};
use crate::plugins::standards::body::extract_body_value;

// ---- free functions ---------------------------------------------------------

/// `probe(host, port, { timeoutMs? })` -> `"open" | "closed" | "filtered"`.
/// Infallible (every failure maps to a string), so a sweep never has to catch.
fn net_probe<'js>(
  ctx: Ctx<'js>,
  host: String,
  port: u16,
  opts: Opt<Object<'js>>,
) -> rquickjs::Result<Promised<impl Future<Output = JsResult<String>>>> {
  let timeout_ms = opt_u64(&opts, "timeoutMs", 1000)?;
  Ok(with_pending(&ctx, async move {
    Ok::<String, String>(forge::net::probe(&host, port, timeout_ms).await.as_str().to_string())
  }))
}

/// `connect(host, port, { timeoutMs? })` -> `Promise<Conn>`. Default timeout 10s.
fn net_connect<'js>(
  ctx: Ctx<'js>,
  host: String,
  port: u16,
  opts: Opt<Object<'js>>,
) -> rquickjs::Result<Promised<impl Future<Output = rquickjs::Result<Class<'js, NetConn>>> + 'js>> {
  let timeout_ms = opt_u64(&opts, "timeoutMs", 10_000)?;
  let pending = ctx.userdata::<PendingOps>().expect("pending ops").clone();
  let ctx2 = ctx.clone();
  Ok(Promised(async move {
    pending.hold();
    let r = forge::net::connect(&host, port, timeout_ms).await;
    pending.release();
    match r {
      Ok(conn) => NetConn::create(&ctx2, conn),
      Err(msg) => Err(Exception::throw_message(&ctx2, &msg)),
    }
  }))
}

/// `listen(port, { host? })` -> `Promise<Listener>`. `host` defaults to `0.0.0.0`.
fn net_listen<'js>(
  ctx: Ctx<'js>,
  port: u16,
  opts: Opt<Object<'js>>,
) -> rquickjs::Result<Promised<impl Future<Output = rquickjs::Result<Class<'js, NetListener>>> + 'js>> {
  let host = opt_string(&opts, "host")?.unwrap_or_else(|| "0.0.0.0".to_string());
  let pending = ctx.userdata::<PendingOps>().expect("pending ops").clone();
  let ctx2 = ctx.clone();
  Ok(Promised(async move {
    pending.hold();
    let r = forge::net::listen(&host, port).await;
    pending.release();
    match r {
      Ok(listener) => NetListener::create(&ctx2, listener),
      Err(msg) => Err(Exception::throw_message(&ctx2, &msg)),
    }
  }))
}

/// `udp({ port?, reuse? })` -> `Promise<Udp>`. `port` 0 (default) = OS-assigned;
/// `reuse` sets `SO_REUSEADDR`/`REUSEPORT` so several sockets can share the port.
fn net_udp<'js>(
  ctx: Ctx<'js>,
  opts: Opt<Object<'js>>,
) -> rquickjs::Result<Promised<impl Future<Output = rquickjs::Result<Class<'js, NetUdp>>> + 'js>> {
  let port = opt_u64(&opts, "port", 0)? as u16;
  let reuse = opt_bool(&opts, "reuse")?.unwrap_or(false);
  let pending = ctx.userdata::<PendingOps>().expect("pending ops").clone();
  let ctx2 = ctx.clone();
  Ok(Promised(async move {
    pending.hold();
    let r = forge::net::udp_bind(port, reuse).await;
    pending.release();
    match r {
      Ok(udp) => NetUdp::create(&ctx2, udp),
      Err(msg) => Err(Exception::throw_message(&ctx2, &msg)),
    }
  }))
}

/// `interfaces()` -> array of `{ name, mac, up, loopback, multicast, addrs }`.
/// Synchronous: `netdev` reads the OS directly and returns quickly.
fn net_interfaces<'js>(ctx: Ctx<'js>) -> rquickjs::Result<Array<'js>> {
  let arr = Array::new(ctx.clone())?;
  for (i, iface) in forge::net::interfaces().into_iter().enumerate() {
    arr.set(i, iface_to_js(&ctx, iface)?)?;
  }
  Ok(arr)
}

// ---- Conn -------------------------------------------------------------------

/// A connected TCP stream: a byte duplex. It is its own async iterator (`for await
/// (chunk of conn)` reads the recv half). A thin wrapper over `forge::net::Conn`.
#[derive(Trace, JsLifetime)]
#[rquickjs::class(rename = "Conn")]
pub struct NetConn {
  #[qjs(skip_trace)]
  inner: Rc<forge::net::Conn>,
}

impl NetConn {
  fn create<'js>(ctx: &Ctx<'js>, conn: forge::net::Conn) -> rquickjs::Result<Class<'js, NetConn>> {
    let instance = Class::instance(ctx.clone(), NetConn { inner: Rc::new(conn) })?;
    attach_async_iterator(ctx, &instance)?;
    Ok(instance)
  }
}

#[rquickjs::methods]
impl NetConn {
  /// Async-iterator step: `{ value: Uint8Array, done: false }` for the next chunk,
  /// `{ done: true }` at EOF. Pull-based, so the socket only advances as JS reads.
  pub fn next<'js>(&self, ctx: Ctx<'js>) -> rquickjs::Result<Promised<impl Future<Output = JsResult<ReadStep>>>> {
    let inner = self.inner.clone();
    Ok(with_pending(&ctx, async move { inner.read_chunk().await.map(ReadStep) }))
  }

  /// Write all of `data` (string or Uint8Array). Resolves once it's handed off.
  pub fn write<'js>(
    &self,
    ctx: Ctx<'js>,
    data: Value<'js>,
  ) -> rquickjs::Result<Promised<impl Future<Output = JsResult<()>>>> {
    let bytes = extract_body_value(&data, "Conn.write")?;
    let inner = self.inner.clone();
    Ok(with_pending(&ctx, async move { inner.write(bytes).await }))
  }

  /// Stop reading and close the connection.
  pub fn close(&self) -> rquickjs::Result<()> {
    self.inner.close();
    Ok(())
  }

  /// The remote peer's address, e.g. `192.168.2.37:445`.
  #[qjs(get)]
  pub fn peer(&self) -> String {
    self.inner.peer()
  }
}

// ---- Listener ---------------------------------------------------------------

/// A bound TCP listener: an async-iterable of incoming `Conn`s
/// (`for await (conn of listener)`). A thin wrapper over `forge::net::Listener`.
#[derive(Trace, JsLifetime)]
#[rquickjs::class(rename = "Listener")]
pub struct NetListener {
  #[qjs(skip_trace)]
  inner: Rc<forge::net::Listener>,
}

impl NetListener {
  fn create<'js>(ctx: &Ctx<'js>, listener: forge::net::Listener) -> rquickjs::Result<Class<'js, NetListener>> {
    let instance = Class::instance(ctx.clone(), NetListener { inner: Rc::new(listener) })?;
    attach_async_iterator(ctx, &instance)?;
    Ok(instance)
  }
}

#[rquickjs::methods]
impl NetListener {
  /// Async-iterator step: resolve `{ value: Conn, done: false }` for the next
  /// accepted connection. Iteration is open-ended; drop the listener to stop.
  pub fn next<'js>(
    &self,
    ctx: Ctx<'js>,
  ) -> rquickjs::Result<Promised<impl Future<Output = rquickjs::Result<Object<'js>>>>> {
    let inner = self.inner.clone();
    let pending = ctx.userdata::<PendingOps>().expect("pending ops").clone();
    let ctx2 = ctx.clone();
    Ok(Promised(async move {
      pending.hold();
      let r = inner.accept().await;
      pending.release();
      match r {
        Ok(conn) => {
          let conn = NetConn::create(&ctx2, conn)?;
          iter_result(&ctx2, Some(conn.into_js(&ctx2)?))
        }
        Err(msg) => Err(Exception::throw_message(&ctx2, &msg)),
      }
    }))
  }

  /// The bound local address (with the OS-assigned port when `0` was requested).
  #[qjs(get, rename = "localAddr")]
  pub fn local_addr(&self) -> String {
    self.inner.local_addr()
  }
}

// ---- Udp --------------------------------------------------------------------

/// A bound UDP socket with the broadcast / multicast knobs the peer beacon needs.
/// A thin wrapper over `forge::net::Udp`.
#[derive(Trace, JsLifetime)]
#[rquickjs::class(rename = "Udp")]
pub struct NetUdp {
  #[qjs(skip_trace)]
  inner: Rc<forge::net::Udp>,
}

impl NetUdp {
  fn create<'js>(ctx: &Ctx<'js>, udp: forge::net::Udp) -> rquickjs::Result<Class<'js, NetUdp>> {
    Class::instance(ctx.clone(), NetUdp { inner: Rc::new(udp) })
  }
}

#[rquickjs::methods]
impl NetUdp {
  /// Send a datagram (string or Uint8Array) to `host:port` — a multicast group, a
  /// broadcast address, or a unicast peer.
  pub fn send<'js>(
    &self,
    ctx: Ctx<'js>,
    data: Value<'js>,
    host: String,
    port: u16,
  ) -> rquickjs::Result<Promised<impl Future<Output = JsResult<()>>>> {
    let bytes = extract_body_value(&data, "Udp.send")?;
    let inner = self.inner.clone();
    Ok(with_pending(&ctx, async move { inner.send(&bytes, &host, port).await.map(|_| ()) }))
  }

  /// Receive one datagram, resolving `{ data: Uint8Array, host, port }`.
  pub fn recv<'js>(&self, ctx: Ctx<'js>) -> rquickjs::Result<Promised<impl Future<Output = JsResult<RecvMsg>>>> {
    let inner = self.inner.clone();
    Ok(with_pending(&ctx, async move { inner.recv().await.map(RecvMsg) }))
  }

  /// Allow sending to the broadcast address (`SO_BROADCAST`).
  #[qjs(rename = "setBroadcast")]
  pub fn set_broadcast(&self, ctx: Ctx<'_>, on: bool) -> rquickjs::Result<()> {
    self.inner.set_broadcast(on).map_err(|m| Exception::throw_message(&ctx, &m))
  }

  /// TTL for outgoing multicast (`1` keeps it on the local link).
  #[qjs(rename = "setMulticastTtl")]
  pub fn set_multicast_ttl(&self, ctx: Ctx<'_>, ttl: u32) -> rquickjs::Result<()> {
    self.inner.set_multicast_ttl(ttl).map_err(|m| Exception::throw_message(&ctx, &m))
  }

  /// Whether multicast this socket sends loops back to sockets on this host.
  #[qjs(rename = "setMulticastLoop")]
  pub fn set_multicast_loop(&self, ctx: Ctx<'_>, on: bool) -> rquickjs::Result<()> {
    self.inner.set_multicast_loop(on).map_err(|m| Exception::throw_message(&ctx, &m))
  }

  /// Join multicast `group` on the interface with address `iface` (default
  /// `0.0.0.0`, OS-chosen). Required to receive that group's datagrams.
  #[qjs(rename = "joinMulticast")]
  pub fn join_multicast(&self, ctx: Ctx<'_>, group: String, iface: Opt<String>) -> rquickjs::Result<()> {
    let (group, iface) = parse_group_iface(&ctx, &group, &iface)?;
    self.inner.join_multicast(group, iface).map_err(|m| Exception::throw_message(&ctx, &m))
  }

  /// Leave a multicast group previously joined with `joinMulticast`.
  #[qjs(rename = "leaveMulticast")]
  pub fn leave_multicast(&self, ctx: Ctx<'_>, group: String, iface: Opt<String>) -> rquickjs::Result<()> {
    let (group, iface) = parse_group_iface(&ctx, &group, &iface)?;
    self.inner.leave_multicast(group, iface).map_err(|m| Exception::throw_message(&ctx, &m))
  }

  /// The bound local address (with the OS-assigned port when `0` was requested).
  #[qjs(get, rename = "localAddr")]
  pub fn local_addr(&self) -> String {
    self.inner.local_addr()
  }
}

// ---- result encodings -------------------------------------------------------

/// One async-iterator step over a `Conn`: a chunk, or end-of-stream. Built into
/// the iterator-result object `{ value, done }` by `IntoJs`. `pub` only to satisfy
/// `private_interfaces` (it names the return type of the `pub` `next` method).
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

/// A received datagram, encoded to `{ data: Uint8Array, host, port }`. `pub` for
/// the same `private_interfaces` reason as `ReadStep`.
pub struct RecvMsg((Vec<u8>, String, u16));

impl<'js> IntoJs<'js> for RecvMsg {
  fn into_js(self, ctx: &Ctx<'js>) -> rquickjs::Result<Value<'js>> {
    let (data, host, port) = self.0;
    let obj = Object::new(ctx.clone())?;
    obj.set("data", TypedArray::<u8>::new(ctx.clone(), data)?)?;
    obj.set("host", host)?;
    obj.set("port", port)?;
    Ok(obj.into_value())
  }
}

// ---- module + helpers -------------------------------------------------------

pub struct NetModule;

impl ModuleDef for NetModule {
  fn declare(decl: &Declarations<'_>) -> rquickjs::Result<()> {
    decl.declare("probe")?;
    decl.declare("connect")?;
    decl.declare("listen")?;
    decl.declare("udp")?;
    decl.declare("interfaces")?;
    Ok(())
  }

  fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> rquickjs::Result<()> {
    exports.export("probe", Function::new(ctx.clone(), net_probe)?)?;
    exports.export("connect", Function::new(ctx.clone(), net_connect)?)?;
    exports.export("listen", Function::new(ctx.clone(), net_listen)?)?;
    exports.export("udp", Function::new(ctx.clone(), net_udp)?)?;
    exports.export("interfaces", Function::new(ctx.clone(), net_interfaces)?)?;
    Ok(())
  }
}

/// Encode a `forge::net::NetInterface` to `{ name, mac, up, loopback, multicast,
/// addrs: [{ ip, prefix, family }] }`.
fn iface_to_js<'js>(ctx: &Ctx<'js>, iface: forge::net::NetInterface) -> rquickjs::Result<Object<'js>> {
  let obj = Object::new(ctx.clone())?;
  obj.set("name", iface.name)?;
  match iface.mac {
    Some(mac) => obj.set("mac", mac)?,
    None => obj.set("mac", Value::new_null(ctx.clone()))?,
  }
  obj.set("up", iface.up)?;
  obj.set("loopback", iface.loopback)?;
  obj.set("multicast", iface.multicast)?;
  let addrs = Array::new(ctx.clone())?;
  for (i, addr) in iface.addrs.into_iter().enumerate() {
    let entry = Object::new(ctx.clone())?;
    entry.set("ip", addr.ip)?;
    entry.set("prefix", addr.prefix)?;
    entry.set("family", addr.family)?;
    addrs.set(i, entry)?;
  }
  obj.set("addrs", addrs)?;
  Ok(obj)
}

/// Parse a multicast `group` and optional `iface` IPv4 string into addresses,
/// defaulting `iface` to `0.0.0.0` (OS-chosen). Throws on an unparseable address.
fn parse_group_iface(ctx: &Ctx<'_>, group: &str, iface: &Opt<String>) -> rquickjs::Result<(Ipv4Addr, Ipv4Addr)> {
  let group = group
    .parse::<Ipv4Addr>()
    .map_err(|_| Exception::throw_message(ctx, &format!("invalid multicast group: {group}")))?;
  let iface = match iface.0.as_deref() {
    Some(s) => {
      s.parse::<Ipv4Addr>().map_err(|_| Exception::throw_message(ctx, &format!("invalid interface address: {s}")))?
    }
    None => Ipv4Addr::UNSPECIFIED,
  };
  Ok((group, iface))
}

/// Read `key` from an optional options object, absent -> `None`.
fn opt_get<'js, V: FromJs<'js>>(opts: &Opt<Object<'js>>, key: &str) -> rquickjs::Result<Option<V>> {
  match opts.0.as_ref() {
    Some(obj) => obj.get::<_, Option<V>>(key),
    None => Ok(None),
  }
}

fn opt_u64(opts: &Opt<Object<'_>>, key: &str, default: u64) -> rquickjs::Result<u64> {
  Ok(opt_get::<f64>(opts, key)?.map(|v| v.max(0.0) as u64).unwrap_or(default))
}

fn opt_string(opts: &Opt<Object<'_>>, key: &str) -> rquickjs::Result<Option<String>> {
  opt_get::<String>(opts, key)
}

fn opt_bool(opts: &Opt<Object<'_>>, key: &str) -> rquickjs::Result<Option<bool>> {
  opt_get::<bool>(opts, key)
}
