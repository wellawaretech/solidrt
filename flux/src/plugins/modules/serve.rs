use bytes::Bytes;
use http_body_util::BodyExt;
use hyper::body::Incoming;
use hyper::service::service_fn;
use hyper::upgrade::OnUpgrade;
use hyper::{Request as HyperRequest, Response as HyperResponse, StatusCode};
use rquickjs::class::Trace;
use rquickjs::function::{Opt, This};
use rquickjs::module::{Declarations, Exports, ModuleDef};
use rquickjs::promise::MaybePromise;
use rquickjs::{Class, Ctx, Exception, Function, JsLifetime, Object, Value};
use std::convert::Infallible;
use std::rc::Rc;
use std::sync::Arc;

use crate::logger::{format_js_error, CtxLogger, Logger};
use crate::pending::PendingOps;
use crate::plugins::modules::p2p::P2pEndpoint;
use crate::plugins::modules::websocket::{
  message_payload, parse_ws_handlers, spawn_socket, try_upgrade, ServeUpgrade, WsHandlers,
};
use crate::plugins::standards::body::{pump_async_iterable, to_byte_stream, ByteStream, MessageBody};
use crate::plugins::standards::headers::headers_from_init;
use crate::plugins::standards::request::{request_from_parts, Request};
use crate::plugins::standards::response::Response;
use forge::http::{
  accept_loop, accept_loop_p2p, bind_listener, build_response, channel_body, full_body, serve_connection,
  text_response, Remote, ResBody, Route, RouteTable, ServerShared,
};
use forge::websocket::Topics;

/// Read a server-built Response's buffered bytes. Server responses are buffered
/// (`new Response(string/bytes)`) or outgoing streams (handled separately), never
/// `Incoming`, so a non-buffered body just yields nothing here.
fn buffered_bytes(r: &Response<'_>) -> Bytes {
  match &r.body {
    MessageBody::Buffered(state) => Bytes::from(state.take().unwrap_or_default()),
    MessageBody::Incoming(_) => Bytes::new(),
  }
}

fn response_from_native<'js>(r: &Response<'js>) -> HyperResponse<ResBody> {
  build_response(r.status, &r.headers.borrow().entries(), full_body(buffered_bytes(r)))
}

/// Build a streamed response: spawn a task that drives the JS async-iterable body
/// (`resp.stream`), feeding its chunks into the response over chunked transfer
/// encoding (no Content-Length). The handler has already returned, so production
/// continues on the executor while hyper flushes frames as they arrive.
fn stream_response<'js>(ctx: &Ctx<'js>, resp: &Response<'js>) -> HyperResponse<ResBody> {
  let iterable = resp.stream.clone().expect("stream_response called without a stream");
  let (tx, body) = channel_body();

  let pump_ctx = ctx.clone();
  let logger = ctx.logger();
  ctx.spawn(async move {
    pump_async_iterable(pump_ctx, iterable, tx, logger).await;
  });

  build_response(resp.status, &resp.headers.borrow().entries(), body)
}

fn response_from_value<'js>(val: Value<'js>, logger: &Logger) -> HyperResponse<ResBody> {
  if let Some(s) = val.as_string() {
    let s = s.to_string().unwrap_or_default();
    return text_response(StatusCode::OK, &s);
  }
  if let Ok(class) = Class::<Response>::from_value(&val) {
    let resp = class.borrow();
    if resp.stream.is_some() {
      return stream_response(val.ctx(), &resp);
    }
    return response_from_native(&resp);
  }
  logger.warn("[flux] serve fetch must return a string or a Response");
  text_response(StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error")
}

/// A `routes` value that is a plain `Response`, captured once at registration so
/// the same bytes can be served on every request. A held JS `Response` would have
/// its body consumed (`take`) after the first serve, so we snapshot it instead.
struct StaticResponse {
  status: u16,
  headers: Vec<(String, String)>,
  body: Bytes,
}

fn snapshot_response<'js>(r: &Response<'js>) -> StaticResponse {
  StaticResponse { status: r.status, headers: r.headers.borrow().entries(), body: buffered_bytes(r) }
}

enum RouteHandler<'js> {
  Fn(Function<'js>),
  Static(StaticResponse),
  /// Per-method object, e.g. `{ GET, POST }`. Keys are uppercased HTTP methods.
  Methods(Vec<(String, Function<'js>)>),
}

/// Parse the `routes` option into a compiled table. A value may be a handler
/// function `(req, server) => Response` or a static `Response`; anything else is
/// warned about and skipped.
fn parse_routes<'js>(
  opts: &Object<'js>,
  logger: &Logger,
) -> rquickjs::Result<Option<Rc<RouteTable<RouteHandler<'js>>>>> {
  let Some(routes_obj): Option<Object<'js>> = opts.get("routes")? else {
    return Ok(None);
  };

  let mut routes = Vec::new();
  for key in routes_obj.keys::<String>().flatten() {
    let value: Value<'js> = routes_obj.get(&key)?;
    // Order matters: a Function is also an object, so check it first; a Response
    // class instance is also an object, so check it before the per-method object.
    let handler = if let Some(f) = value.as_function() {
      RouteHandler::Fn(f.clone())
    } else if let Ok(resp) = Class::<Response>::from_value(&value) {
      RouteHandler::Static(snapshot_response(&resp.borrow()))
    } else if let Some(obj) = value.as_object() {
      let mut methods = Vec::new();
      for method in obj.keys::<String>().flatten() {
        let mv: Value<'js> = obj.get(&method)?;
        match mv.as_function() {
          Some(f) => methods.push((method.to_uppercase(), f.clone())),
          None => logger.warn(&format!("[flux] serve route {key} method {method} ignored: value must be a function")),
        }
      }
      RouteHandler::Methods(methods)
    } else {
      logger.warn(&format!("[flux] serve route {key} ignored: value must be a function, Response, or method object"));
      continue;
    };
    routes.push(Route::new(&key, handler));
  }
  Ok(Some(Rc::new(RouteTable::from_routes(routes))))
}

/// The JS handler set a running server dispatches to. Cloned per connection and
/// per request; every field is a cheap GC-handle or `Rc` clone.
#[derive(Clone)]
struct Handlers<'js> {
  fetch_fn: Option<Function<'js>>,
  error_fn: Option<Function<'js>>,
  routes: Option<Rc<RouteTable<RouteHandler<'js>>>>,
  websocket: Option<Rc<WsHandlers<'js>>>,
  server: Class<'js, Server>,
}

/// Mark a returned promise as handled at the JS-engine level before its result is
/// read natively through `MaybePromise`/`into_future`. `PromiseFuture::poll`
/// takes a fast path: when the promise has already settled (e.g. a handler
/// synchronously returns `Promise.reject(e)`), it reads the result via
/// `JS_PromiseResult` directly and never calls `.then()`/`.catch()`. QuickJS's
/// unhandled-rejection tracker only clears when a reaction is attached, so
/// without this a rejection we genuinely route to `error()` is still reported by
/// `engine::flush_rejections` as if nobody looked at it.
///
/// The reaction must be a real no-op rejection handler, not `Undefined`:
/// `.then(_, undefined)` marks this promise handled but yields a derived promise
/// that re-rejects with the same reason and is itself unhandled, so the rejection
/// simply reappears. A no-op `onRejected` lets the derived promise resolve.
fn mark_observed<'js>(val: &Value<'js>) {
  let Some(promise) = val.as_promise() else { return };
  let Ok(noop) = Function::new(promise.ctx().clone(), || {}) else { return };
  let Ok(catch) = promise.catch() else { return };
  let _ = catch.call::<_, Value<'_>>((This(promise.clone()), noop));
}

/// Invoke a JS request handler `(req, server)`, await a promise result, and turn
/// the value into a response. A throw or rejection routes through the `error`
/// handler. A request upgraded to a websocket during the call gets its held 101
/// response instead.
async fn call_handler<'js>(
  f: &Function<'js>,
  req_class: Class<'js, Request<'js>>,
  handlers: &Handlers<'js>,
  logger: &Logger,
) -> HyperResponse<ResBody> {
  let ctx = f.ctx().clone();
  let error_fn = handlers.error_fn.as_ref();
  let val = match f
    .call::<(Class<'_, Request<'_>>, Class<'_, Server>), Value<'_>>((req_class.clone(), handlers.server.clone()))
  {
    Ok(v) => v,
    Err(e) => return error_response(&ctx, e, error_fn, logger).await,
  };
  mark_observed(&val);
  let resolved = match MaybePromise::from_value(val).into_future::<Value<'_>>().await {
    Ok(v) => v,
    Err(e) => return error_response(&ctx, e, error_fn, logger).await,
  };
  if let Some(resp) = take_upgrade(&ctx, &req_class, handlers, &resolved, logger) {
    return resp;
  }
  response_from_value(resolved, logger)
}

/// If `server.upgrade(req)` accepted a websocket handshake during this handler
/// call, spawn the socket tasks and return the held 101 response. Taking the
/// slot also drops an unused `Ready` capability: once the handler has returned
/// a normal response, the request can no longer upgrade.
fn take_upgrade<'js>(
  ctx: &Ctx<'js>,
  req_class: &Class<'js, Request<'js>>,
  handlers: &Handlers<'js>,
  resolved: &Value<'js>,
  logger: &Logger,
) -> Option<HyperResponse<ResBody>> {
  let slot = req_class.borrow().upgrade.borrow_mut().take();
  let Some(ServeUpgrade::Accepted { response, socket, data, remote }) = slot else {
    return None;
  };
  if !resolved.is_undefined() {
    logger.warn("[flux] serve: handler returned a value after upgrade(); it is ignored");
  }
  let Some(ws_handlers) = handlers.websocket.clone() else {
    // upgrade() refuses without a websocket option, so this should not happen.
    logger.warn("[flux] serve: upgraded request without websocket handlers");
    return Some(text_response(StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error"));
  };
  let server = handlers.server.borrow();
  let shutdown_rx = server.shared.subscribe();
  spawn_socket(ctx, socket, ws_handlers, shutdown_rx, logger.clone(), data, remote, server.topics.clone());
  Some(response)
}

/// Build the response for a failed `fetch`: either the callback threw or its
/// returned promise rejected. In both cases rquickjs re-throws the value into
/// the context, so `ctx.catch()` yields the JS error to hand to the user's
/// `error(err)` handler. With no handler (or if the handler itself throws or
/// rejects) we fall back to a plaintext 500. The handler is never re-entered on
/// its own failure, so there is no error loop.
async fn error_response<'js>(
  ctx: &Ctx<'js>,
  _err: rquickjs::Error,
  error_fn: Option<&Function<'js>>,
  logger: &Logger,
) -> HyperResponse<ResBody> {
  let exception = ctx.catch();
  logger.warn(&format!(
    "[flux] serve fetch error: {}",
    exception.as_exception().map_or_else(|| format!("{exception:?}"), |e| e.to_string())
  ));

  let ef = match error_fn {
    Some(f) => f,
    None => return text_response(StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error"),
  };

  let val = match ef.call::<(Value<'_>,), Value<'_>>((exception,)) {
    Ok(v) => v,
    Err(e) => {
      logger.warn(&format!("[flux] serve error handler threw: {}", format_js_error(ctx, e)));
      return text_response(StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error");
    }
  };
  mark_observed(&val);

  let resolved = match MaybePromise::from_value(val).into_future::<Value<'_>>().await {
    Ok(v) => v,
    Err(e) => {
      logger.warn(&format!("[flux] serve error handler rejected: {}", format_js_error(ctx, e)));
      return text_response(StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error");
    }
  };

  response_from_value(resolved, logger)
}

/// The parts of an incoming request, extracted once up front and then consumed by
/// whichever handler serves it (a route fn, a per-method fn, or the `fetch`
/// fallback). Bundling them keeps `dispatch_fn` under clippy's argument limit. The
/// `body` is the request stream, read incrementally by the handler (not buffered).
struct RequestParts {
  method: String,
  url: String,
  body: ByteStream,
  headers: Vec<(String, String)>,
  /// The hyper upgrade handle, carried into the JS Request so the handler can
  /// call `server.upgrade(req)`.
  upgrade: Option<OnUpgrade>,
  /// The connection's peer, carried into the JS Request for
  /// `server.requestIP(req)` and `ws.remoteAddress`.
  remote: Option<Remote>,
}

/// Build the JS `Request` from `parts` (plus any captured path `params`) and run
/// it through the handler function. Shared by route fns, per-method fns, and the
/// `fetch` fallback.
async fn dispatch_fn<'js>(
  f: &Function<'js>,
  parts: RequestParts,
  params: Vec<(String, String)>,
  handlers: &Handlers<'js>,
  logger: &Logger,
) -> HyperResponse<ResBody> {
  let ctx = f.ctx().clone();
  let req_class = match request_from_parts(
    &ctx,
    parts.method,
    parts.url,
    parts.body,
    parts.headers,
    params,
    parts.upgrade,
    parts.remote,
  ) {
    Ok(c) => c,
    Err(e) => {
      logger.warn(&format!("[flux] serve build Request error: {e}"));
      return text_response(StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error");
    }
  };
  call_handler(f, req_class, handlers, logger).await
}

async fn handle_request<'js>(
  mut req: HyperRequest<Incoming>,
  remote: Option<Remote>,
  handlers: &Handlers<'js>,
  logger: &Logger,
) -> HyperResponse<ResBody> {
  let fetch_fn = handlers.fetch_fn.as_ref();
  let routes = handlers.routes.as_deref();

  let method = req.method().as_str().to_string();
  let url = req.uri().to_string();
  let headers: Vec<(String, String)> = req
    .headers()
    .iter()
    .filter_map(|(name, value)| value.to_str().ok().map(|v| (name.as_str().to_string(), v.to_string())))
    .collect();

  // The upgrade handle rides along on the Request so a handler can accept a
  // websocket via server.upgrade(req); unused it is simply dropped.
  let upgrade = req.extensions_mut().remove::<OnUpgrade>();

  // Stream the request body to the handler rather than buffering it up front, so
  // large uploads stay constant-memory: the handler reads it on demand via
  // req.text()/req.json()/req.bytes() or by iterating req.body. A handler that
  // never reads it (e.g. a static route) just drops the unread stream.
  let body = to_byte_stream(req.into_body().into_data_stream());
  let parts = RequestParts { method, url, body, headers, upgrade, remote };

  // Try the route table first; a match dispatches to its handler. Static routes
  // need neither the body nor a Request, so they serve their snapshot directly.
  if let Some(table) = routes {
    let path = parts.url.split('?').next().unwrap_or(parts.url.as_str());
    if let Some((handler, params)) = table.lookup(path) {
      match handler {
        RouteHandler::Static(s) => return build_response(s.status, &s.headers, full_body(s.body.clone())),
        RouteHandler::Fn(f) => return dispatch_fn(f, parts, params, handlers, logger).await,
        // Per-method object: dispatch on the request method, else 405 + Allow.
        RouteHandler::Methods(methods) => {
          return match methods.iter().find(|(m, _)| *m == parts.method) {
            Some((_, f)) => dispatch_fn(f, parts, params, handlers, logger).await,
            None => {
              let allow = methods.iter().map(|(m, _)| m.clone()).collect::<Vec<_>>().join(", ");
              build_response(405, &[("Allow".to_string(), allow)], full_body(Bytes::from_static(b"Method Not Allowed")))
            }
          };
        }
      }
    }
  }

  // No route matched: fall through to the `fetch` handler, or 404 without one.
  match fetch_fn {
    Some(f) => dispatch_fn(f, parts, Vec::new(), handlers, logger).await,
    None => text_response(StatusCode::NOT_FOUND, "Not Found"),
  }
}

/// Handle returned by `Flux.serve`. Loosely models Bun's `Server`: `stop()` plus
/// `port`/`hostname`/`url` introspection. `stop()` is synchronous and graceful:
/// it stops accepting and asks each open connection to shut down gracefully, so
/// in-flight requests finish and idle keep-alive connections close promptly.
#[derive(Trace, JsLifetime)]
#[rquickjs::class(rename = "Server")]
pub struct Server {
  #[qjs(skip_trace)]
  shared: Arc<ServerShared>,
  #[qjs(skip_trace)]
  port: u16,
  #[qjs(skip_trace)]
  hostname: String,
  /// Whether `serve` got a `websocket` option; without one `upgrade()` refuses.
  #[qjs(skip_trace)]
  has_websocket: bool,
  /// The pub/sub topic registry shared with every socket of this server.
  #[qjs(skip_trace)]
  topics: Topics,
}

#[rquickjs::methods]
impl Server {
  /// Accept a websocket handshake for `req`. On true the handler must return
  /// nothing: the held 101 response is sent when it returns, and the `websocket`
  /// callbacks take over the connection. False means the request cannot upgrade
  /// (not a websocket request, already upgraded, or no `websocket` option), so
  /// the handler can serve a normal response instead. Options: `data` becomes
  /// `ws.data` on the socket handle; `headers` (object or Headers) are added to
  /// the 101 response (e.g. Set-Cookie).
  pub fn upgrade<'js>(&self, ctx: Ctx<'js>, req: Class<'js, Request<'js>>, opts: Opt<Object<'js>>) -> bool {
    let logger = ctx.logger();
    if !self.has_websocket {
      logger.warn("[flux] serve: upgrade() requires a websocket option on serve()");
      return false;
    }

    let mut data: Option<Value<'js>> = None;
    let mut extra_headers = Vec::new();
    if let Some(o) = opts.0 {
      data = o.get::<_, Value>("data").ok().filter(|v| !v.is_undefined() && !v.is_null());
      let headers_val: Option<Value<'js>> = o.get("headers").ok();
      if let Some(hv) = headers_val.filter(|v| !v.is_undefined() && !v.is_null()) {
        match headers_from_init(&ctx, Some(&hv)) {
          Ok(h) => extra_headers = h.borrow().entries(),
          Err(e) => {
            logger.warn(&format!("[flux] serve: upgrade failed: invalid headers option: {e}"));
            return false;
          }
        }
      }
    }

    let req = req.borrow();
    let headers = req.headers.borrow().entries();
    let mut slot = req.upgrade.borrow_mut();
    let Some(ServeUpgrade::Ready(on_upgrade)) = slot.take() else {
      return false;
    };
    match try_upgrade(&headers, on_upgrade, &extra_headers, data, req.remote.clone()) {
      Ok(accepted) => {
        *slot = Some(accepted);
        true
      }
      Err(e) => {
        logger.warn(&format!("[flux] serve: upgrade failed: {e}"));
        false
      }
    }
  }

  /// Publish a message (string or Uint8Array) to every socket subscribed to
  /// `topic`. Returns the number of sockets the message was queued to.
  pub fn publish<'js>(&self, topic: String, data: Value<'js>) -> rquickjs::Result<i32> {
    let (opcode, payload) = message_payload(&data)?;
    Ok(self.topics.publish(&topic, opcode, payload, None))
  }

  /// How many sockets are currently subscribed to `topic`.
  #[qjs(rename = "subscriberCount")]
  pub fn subscriber_count(&self, topic: String) -> usize {
    self.topics.subscriber_count(&topic)
  }

  /// The peer of the connection `req` arrived on, as `{ address, port,
  /// family }`, or null when unknown (e.g. a JS-constructed Request). Models
  /// Bun's `server.requestIP`. A p2p peer has no IP: `address` is its endpoint
  /// id, `port` is 0, and `family` is `"p2p"`.
  #[qjs(rename = "requestIP")]
  pub fn request_ip<'js>(&self, ctx: Ctx<'js>, req: Class<'js, Request<'js>>) -> rquickjs::Result<Value<'js>> {
    match &req.borrow().remote {
      Some(Remote::Ip(addr)) => {
        let obj = Object::new(ctx)?;
        obj.set("address", addr.ip().to_string())?;
        obj.set("port", addr.port())?;
        obj.set("family", if addr.is_ipv4() { "IPv4" } else { "IPv6" })?;
        Ok(obj.into_value())
      }
      Some(Remote::Peer(id)) => {
        let obj = Object::new(ctx)?;
        obj.set("address", id.as_str())?;
        obj.set("port", 0)?;
        obj.set("family", "p2p")?;
        Ok(obj.into_value())
      }
      None => Ok(Value::new_null(ctx)),
    }
  }

  /// Stop accepting new connections and gracefully shut down open ones. Safe to
  /// call more than once.
  pub fn stop(&self) {
    self.shared.stop();
  }

  #[qjs(get)]
  pub fn port(&self) -> u16 {
    self.port
  }

  #[qjs(get)]
  pub fn hostname(&self) -> String {
    self.hostname.clone()
  }

  #[qjs(get)]
  pub fn url(&self) -> String {
    format!("http://{}:{}/", self.hostname, self.port)
  }
}

/// Build the per-connection hyper service: each request dispatches into the JS
/// handlers with the connection's peer identity attached. Shared by the TCP and
/// p2p accept loops.
fn connection_service<'js>(
  handlers: Handlers<'js>,
  logger: Logger,
  remote: Option<Remote>,
) -> impl hyper::service::Service<HyperRequest<Incoming>, Response = HyperResponse<ResBody>, Error = Infallible> + 'js {
  service_fn(move |req: HyperRequest<Incoming>| {
    let handlers = handlers.clone();
    let logger = logger.clone();
    let remote = remote.clone();
    async move { Ok::<_, Infallible>(handle_request(req, remote, &handlers, &logger).await) }
  })
}

/// `serve(opts)`: bind a listener, spawn the accept loop, return a `Server`.
/// A free function (not a closure) so its `'js` is properly higher-ranked, which
/// the invariant `Class<'js, Server>` return type requires.
fn serve_impl<'js>(ctx: Ctx<'js>, opts: Object<'js>) -> rquickjs::Result<Class<'js, Server>> {
  let port: u16 = opts.get("port")?;
  let fetch_fn: Option<Function<'js>> = opts.get("fetch").ok();
  let error_fn: Option<Function<'js>> = opts.get("error").ok();
  let pending = ctx.userdata::<PendingOps>().expect("pending ops").clone();
  let logger = ctx.logger();
  let routes = parse_routes(&opts, &logger)?;
  let websocket = parse_ws_handlers(&opts)?;

  // The `p2p: { endpoint, protocol }` option: a flux:p2p Endpoint to accept
  // connections on next to the TCP listener, and the protocol (ALPN) to accept.
  // Cloning the forge core out here keeps the accept loop free of JS types.
  let p2p = match opts.get::<_, Option<Object<'js>>>("p2p")? {
    Some(o) => {
      let endpoint: Option<Class<'js, P2pEndpoint>> = o.get("endpoint")?;
      match (endpoint, o.get::<_, Option<String>>("protocol")?) {
        (Some(ep), Some(protocol)) => Some((ep.borrow().core(), protocol.into_bytes())),
        _ => return Err(Exception::throw_message(&ctx, "serve: p2p requires an endpoint and a protocol")),
      }
    }
    None => None,
  };

  let hostname: Option<String> = opts.get("hostname")?;
  let hostname = hostname.unwrap_or_else(|| "0.0.0.0".to_string());
  let listener = bind_listener(&hostname, port).map_err(|e| Exception::throw_message(&ctx, &e))?;

  let shared = ServerShared::new();

  // Build the handle up front so the same `Server` is both returned to the
  // caller and passed as the second `fetch(req, server)` argument.
  let server = Class::instance(
    ctx.clone(),
    Server { shared: shared.clone(), port, hostname, has_websocket: websocket.is_some(), topics: Topics::default() },
  )?;
  let handlers = Handlers { fetch_fn, error_fn, routes, websocket, server: server.clone() };

  pending.hold();
  let loop_ctx = ctx.clone();
  let loop_logger = logger.clone();
  let loop_shared = shared.clone();
  let loop_pending = pending.clone();
  let loop_handlers = handlers.clone();
  ctx.spawn(async move {
    let shutdown_rx = loop_shared.subscribe();
    let accept_logger = loop_logger.clone();
    // Each accepted socket spawns its own connection task. Spawning stays here in
    // the marshalling layer (the engine-free core hands sockets back through this
    // closure) so the core never touches `ctx.spawn`.
    let on_conn = move |sock: tokio::net::TcpStream| {
      let remote = sock.peer_addr().ok().map(Remote::Ip);
      let conn_rx = loop_shared.subscribe();
      let service = connection_service(loop_handlers.clone(), loop_logger.clone(), remote);
      loop_ctx.spawn(serve_connection(sock, service, loop_logger.clone(), conn_rx));
    };
    accept_loop(listener, accept_logger, shutdown_rx, on_conn).await;
    // Paired with the hold() above. Connection tasks shut themselves down on the
    // same signal, so the runtime drains and the engine can exit.
    loop_pending.release();
  });

  // The p2p twin of the TCP loop: a connection's io is its first bi-stream, its
  // peer identity the remote endpoint id. `stop()` stops both loops through the
  // shared shutdown signal; the endpoint itself stays open for its owner.
  if let Some((endpoint, alpn)) = p2p {
    pending.hold();
    let loop_ctx = ctx.clone();
    let loop_logger = logger.clone();
    let loop_shared = shared.clone();
    let loop_pending = pending.clone();
    ctx.spawn(async move {
      let shutdown_rx = loop_shared.subscribe();
      let accept_logger = loop_logger.clone();
      let on_conn = move |remote: String, io: forge::p2p::ConnIo| {
        let conn_rx = loop_shared.subscribe();
        let service = connection_service(handlers.clone(), loop_logger.clone(), Some(Remote::Peer(remote)));
        // Hold the QUIC connection past the served io: dropping it on the heels
        // of the final write would discard the unacknowledged tail (see
        // `ConnIo::closed`).
        let closed = io.closed();
        let conn_logger = loop_logger.clone();
        loop_ctx.spawn(async move {
          serve_connection(io, service, conn_logger, conn_rx).await;
          closed.await;
        });
      };
      accept_loop_p2p(endpoint, alpn, accept_logger, shutdown_rx, on_conn).await;
      loop_pending.release();
    });
  }

  Ok(server)
}

/// The `flux:http` module. Exports `serve`, the HTTP server entry point.
pub struct HttpModule;

impl ModuleDef for HttpModule {
  fn declare<'js>(decl: &Declarations<'js>) -> rquickjs::Result<()> {
    decl.declare("serve")?;
    Ok(())
  }

  fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> rquickjs::Result<()> {
    let serve_fn = Function::new(ctx.clone(), serve_impl)?;
    exports.export("serve", serve_fn)?;
    Ok(())
  }
}
