use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request as HyperRequest, Response as HyperResponse, StatusCode};
use hyper_util::rt::TokioIo;
use rquickjs::class::Trace;
use rquickjs::promise::MaybePromise;
use rquickjs::{Class, Ctx, Function, JsLifetime, Object, Value};
use std::rc::Rc;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;

use crate::logger::{CtxLogger, Logger};
use crate::pending::PendingOps;
use crate::plugins::request::{request_from_parts, Request};
use crate::plugins::response::Response;

fn text_response(status: StatusCode, body: &str) -> HyperResponse<Full<Bytes>> {
  HyperResponse::builder()
    .status(status)
    .header("Content-Type", "text/plain")
    .body(Full::new(Bytes::copy_from_slice(body.as_bytes())))
    .expect("build response")
}

/// Assemble a hyper response from already-extracted parts. Defaults the
/// Content-Type to text/plain when the headers don't set one.
fn build_response(status: u16, headers: &[(String, String)], body: Bytes) -> HyperResponse<Full<Bytes>> {
  let status = StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
  let mut builder = HyperResponse::builder().status(status);
  let mut has_content_type = false;
  for (k, v) in headers {
    if k.eq_ignore_ascii_case("content-type") {
      has_content_type = true;
    }
    builder = builder.header(k.as_str(), v.as_str());
  }
  if !has_content_type {
    builder = builder.header("Content-Type", "text/plain");
  }
  builder
    .body(Full::new(body))
    .unwrap_or_else(|_| text_response(StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error"))
}

fn response_from_native<'js>(r: &Response<'js>) -> HyperResponse<Full<Bytes>> {
  let body = Bytes::from(r.body.take().unwrap_or_default());
  build_response(r.status, &r.headers.borrow().entries(), body)
}

fn response_from_value<'js>(val: Value<'js>, logger: &Logger) -> HyperResponse<Full<Bytes>> {
  if let Some(s) = val.as_string() {
    let s = s.to_string().unwrap_or_default();
    return text_response(StatusCode::OK, &s);
  }
  if let Ok(class) = Class::<Response>::from_value(&val) {
    return response_from_native(&class.borrow());
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
  StaticResponse {
    status: r.status,
    headers: r.headers.borrow().entries(),
    body: Bytes::from(r.body.take().unwrap_or_default()),
  }
}

/// One `/`-delimited segment of a route pattern.
enum Segment {
  Literal(String),
  Param(String),
  Wildcard,
}

enum RouteHandler<'js> {
  Fn(Function<'js>),
  Static(StaticResponse),
}

/// A registered route: its compiled pattern, a match `tier` (0 = exact, 1 = has a
/// `:param`, 2 = has a `*`), and the handler to run.
struct Route<'js> {
  segments: Vec<Segment>,
  tier: u8,
  handler: RouteHandler<'js>,
}

/// The compiled `routes` table. Routes are pre-sorted by `tier` so exact patterns
/// beat `:param` patterns beat `*`; within a tier, registration order is kept.
struct RouteTable<'js> {
  routes: Vec<Route<'js>>,
}

impl<'js> RouteTable<'js> {
  /// Return the first matching route's handler and its captured path params.
  fn lookup(&self, path: &str) -> Option<(&RouteHandler<'js>, Vec<(String, String)>)> {
    let path_segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    for route in &self.routes {
      if let Some(params) = match_segments(&route.segments, &path_segs) {
        return Some((&route.handler, params));
      }
    }
    None
  }
}

/// Split a pattern like `/users/:id/*` into segments and compute its match tier.
fn parse_pattern(pattern: &str) -> (Vec<Segment>, u8) {
  let mut segments = Vec::new();
  let mut tier = 0u8;
  for part in pattern.split('/').filter(|s| !s.is_empty()) {
    if part == "*" {
      segments.push(Segment::Wildcard);
      tier = 2;
    } else if let Some(name) = part.strip_prefix(':') {
      segments.push(Segment::Param(name.to_string()));
      tier = tier.max(1);
    } else {
      segments.push(Segment::Literal(part.to_string()));
    }
  }
  (segments, tier)
}

/// Match request path segments against a pattern, capturing `:param` values. A
/// trailing `*` matches the remaining segments (including none).
fn match_segments(segments: &[Segment], path: &[&str]) -> Option<Vec<(String, String)>> {
  let mut params = Vec::new();
  let mut i = 0;
  for seg in segments {
    match seg {
      Segment::Wildcard => return Some(params),
      Segment::Literal(lit) => {
        if path.get(i) != Some(&lit.as_str()) {
          return None;
        }
        i += 1;
      }
      Segment::Param(name) => {
        let value = path.get(i)?;
        params.push((name.clone(), (*value).to_string()));
        i += 1;
      }
    }
  }
  // Without a wildcard the path must be fully consumed (no extra segments).
  (i == path.len()).then_some(params)
}

/// Parse the `routes` option into a compiled table. A value may be a handler
/// function `(req, server) => Response` or a static `Response`; anything else is
/// warned about and skipped.
fn parse_routes<'js>(opts: &Object<'js>, logger: &Logger) -> rquickjs::Result<Option<Rc<RouteTable<'js>>>> {
  let Some(routes_obj): Option<Object<'js>> = opts.get("routes")? else {
    return Ok(None);
  };

  let mut routes = Vec::new();
  for key in routes_obj.keys::<String>().flatten() {
    let value: Value<'js> = routes_obj.get(&key)?;
    let handler = if let Some(f) = value.as_function() {
      RouteHandler::Fn(f.clone())
    } else if let Ok(resp) = Class::<Response>::from_value(&value) {
      RouteHandler::Static(snapshot_response(&resp.borrow()))
    } else {
      logger.warn(&format!("[flux] serve route {key} ignored: value must be a function or a Response"));
      continue;
    };
    let (segments, tier) = parse_pattern(&key);
    routes.push(Route { segments, tier, handler });
  }
  // Stable sort keeps registration order within each tier.
  routes.sort_by_key(|r| r.tier);
  Ok(Some(Rc::new(RouteTable { routes })))
}

/// The JS handler set a running server dispatches to. Cloned per connection and
/// per request; every field is a cheap GC-handle or `Rc` clone.
#[derive(Clone)]
struct Handlers<'js> {
  fetch_fn: Option<Function<'js>>,
  error_fn: Option<Function<'js>>,
  routes: Option<Rc<RouteTable<'js>>>,
  server: Class<'js, Server>,
}

/// Invoke a JS request handler `(req, server)`, await a promise result, and turn
/// the value into a response. A throw or rejection routes through `error_fn`.
async fn call_handler<'js>(
  f: &Function<'js>,
  req_class: Class<'js, Request<'js>>,
  server: &Class<'js, Server>,
  error_fn: Option<&Function<'js>>,
  logger: &Logger,
) -> HyperResponse<Full<Bytes>> {
  let ctx = f.ctx().clone();
  let val = match f.call::<(Class<'_, Request<'_>>, Class<'_, Server>), Value<'_>>((req_class, server.clone())) {
    Ok(v) => v,
    Err(e) => return error_response(&ctx, e, error_fn, logger).await,
  };
  let resolved = match MaybePromise::from_value(val).into_future::<Value<'_>>().await {
    Ok(v) => v,
    Err(e) => return error_response(&ctx, e, error_fn, logger).await,
  };
  response_from_value(resolved, logger)
}

/// Build the response for a failed `fetch`: either the callback threw or its
/// returned promise rejected. In both cases rquickjs re-throws the value into
/// the context, so `ctx.catch()` yields the JS error to hand to the user's
/// `error(err)` handler. With no handler (or if the handler itself throws or
/// rejects) we fall back to a plaintext 500. The handler is never re-entered on
/// its own failure, so there is no error loop.
async fn error_response<'js>(
  ctx: &Ctx<'js>,
  err: rquickjs::Error,
  error_fn: Option<&Function<'js>>,
  logger: &Logger,
) -> HyperResponse<Full<Bytes>> {
  let exception = ctx.catch();
  logger.warn(&format!("[flux] serve fetch error: {err}"));

  let ef = match error_fn {
    Some(f) => f,
    None => return text_response(StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error"),
  };

  let val = match ef.call::<(Value<'_>,), Value<'_>>((exception,)) {
    Ok(v) => v,
    Err(e) => {
      logger.warn(&format!("[flux] serve error handler threw: {e}"));
      return text_response(StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error");
    }
  };

  let resolved = match MaybePromise::from_value(val).into_future::<Value<'_>>().await {
    Ok(v) => v,
    Err(e) => {
      logger.warn(&format!("[flux] serve error handler rejected: {e}"));
      return text_response(StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error");
    }
  };

  response_from_value(resolved, logger)
}

async fn handle_request<'js>(
  req: HyperRequest<Incoming>,
  handlers: &Handlers<'js>,
  logger: &Logger,
) -> HyperResponse<Full<Bytes>> {
  let fetch_fn = handlers.fetch_fn.as_ref();
  let error_fn = handlers.error_fn.as_ref();
  let routes = handlers.routes.as_deref();
  let server = &handlers.server;

  let method = req.method().as_str().to_string();
  let url = req.uri().to_string();
  let headers: Vec<(String, String)> = req
    .headers()
    .iter()
    .filter_map(|(name, value)| value.to_str().ok().map(|v| (name.as_str().to_string(), v.to_string())))
    .collect();
  logger.log(&format!("[flux] serve {} {}", method, url));

  let body_bytes = match req.into_body().collect().await {
    Ok(collected) => collected.to_bytes().to_vec(),
    Err(e) => {
      logger.warn(&format!("[flux] serve request body read error: {e}"));
      return text_response(StatusCode::BAD_REQUEST, "Bad Request");
    }
  };

  // Try the route table first; a match dispatches to its handler. Static routes
  // need neither the body nor a Request, so they serve their snapshot directly.
  if let Some(table) = routes {
    let path = url.split('?').next().unwrap_or(url.as_str());
    if let Some((handler, params)) = table.lookup(path) {
      match handler {
        RouteHandler::Static(s) => return build_response(s.status, &s.headers, s.body.clone()),
        RouteHandler::Fn(f) => {
          let ctx = f.ctx().clone();
          let req_class = match request_from_parts(&ctx, method, url, body_bytes, headers, params) {
            Ok(c) => c,
            Err(e) => {
              logger.warn(&format!("[flux] serve build Request error: {e}"));
              return text_response(StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error");
            }
          };
          return call_handler(f, req_class, server, error_fn, logger).await;
        }
      }
    }
  }

  // No route matched: fall through to the `fetch` handler, or 404 without one.
  let f = match fetch_fn {
    Some(f) => f,
    None => return text_response(StatusCode::NOT_FOUND, "Not Found"),
  };
  let ctx = f.ctx().clone();
  let req_class = match request_from_parts(&ctx, method, url, body_bytes, headers, Vec::new()) {
    Ok(c) => c,
    Err(e) => {
      logger.warn(&format!("[flux] serve build Request error: {e}"));
      return text_response(StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error");
    }
  };
  call_handler(f, req_class, server, error_fn, logger).await
}

/// Shutdown signal shared between the JS `Server` handle, its accept loop, and
/// each connection task. A `watch` channel latches (once set true it stays true)
/// and broadcasts to every subscriber. The sender lives inside the `Arc`, which
/// the accept loop also holds, so it stays alive independent of the JS handle's
/// lifetime: dropping the handle (e.g. an unsaved `Flux.serve(...)`) leaves the
/// server running, matching the previous behavior.
struct ServerShared {
  shutdown: watch::Sender<bool>,
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
}

#[rquickjs::methods]
impl Server {
  /// Stop accepting new connections and gracefully shut down open ones. Safe to
  /// call more than once. A send error means there are no live subscribers (the
  /// loop already exited), i.e. already stopped.
  pub fn stop(&self) {
    let _ = self.shared.shutdown.send(true);
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

/// Resolve once a stop has been signalled (value `true`). A dropped sender also
/// resolves it: nothing can signal a stop anymore, so treat it as one.
async fn wait_for_stop(rx: &mut watch::Receiver<bool>) {
  let _ = rx.wait_for(|&stop| stop).await;
}

async fn serve_one_connection<'js>(
  sock: TcpStream,
  handlers: Handlers<'js>,
  logger: Logger,
  mut shutdown_rx: watch::Receiver<bool>,
) {
  let io = TokioIo::new(sock);
  let service = service_fn(|req: HyperRequest<Incoming>| {
    let handlers = handlers.clone();
    let logger = logger.clone();
    async move { Ok::<_, std::convert::Infallible>(handle_request(req, &handlers, &logger).await) }
  });

  let conn = http1::Builder::new().serve_connection(io, service);
  tokio::pin!(conn);

  tokio::select! {
    res = conn.as_mut() => {
      if let Err(e) = res {
        logger.warn(&format!("[flux] serve connection error: {e}"));
      }
    }
    // On stop, finish any in-flight request then close. An idle keep-alive
    // connection has nothing in flight, so it closes promptly and the task ends.
    _ = wait_for_stop(&mut shutdown_rx) => {
      conn.as_mut().graceful_shutdown();
      if let Err(e) = conn.as_mut().await {
        logger.warn(&format!("[flux] serve connection error: {e}"));
      }
    }
  }
}

async fn run_server<'js>(
  ctx: Ctx<'js>,
  listener: TcpListener,
  handlers: Handlers<'js>,
  logger: Logger,
  shared: Arc<ServerShared>,
  pending: PendingOps,
) {
  let mut shutdown_rx = shared.shutdown.subscribe();
  loop {
    tokio::select! {
      accepted = listener.accept() => {
        let (sock, _) = match accepted {
          Ok(v) => v,
          Err(e) => {
            logger.warn(&format!("[flux] serve accept error: {e}"));
            continue;
          }
        };
        let handlers = handlers.clone();
        let logger = logger.clone();
        let conn_rx = shared.shutdown.subscribe();
        ctx.spawn(async move {
          serve_one_connection(sock, handlers, logger, conn_rx).await;
        });
      }
      _ = wait_for_stop(&mut shutdown_rx) => break,
    }
  }
  // Paired with the hold() taken at startup. Connection tasks shut themselves
  // down on the same signal, so the runtime drains and the engine can exit.
  pending.release();
}

/// `Flux.serve(opts)`: bind a listener, spawn the accept loop, return a `Server`.
/// A free function (not a closure) so its `'js` is properly higher-ranked, which
/// the invariant `Class<'js, Server>` return type requires.
fn serve_impl<'js>(ctx: Ctx<'js>, opts: Object<'js>) -> rquickjs::Result<Class<'js, Server>> {
  let port: u16 = opts.get("port")?;
  let fetch_fn: Option<Function<'js>> = opts.get("fetch").ok();
  let error_fn: Option<Function<'js>> = opts.get("error").ok();
  let pending = ctx.userdata::<PendingOps>().expect("pending ops").clone();
  let logger = ctx.logger();
  let routes = parse_routes(&opts, &logger)?;

  let hostname: Option<String> = opts.get("hostname")?;
  let hostname = hostname.unwrap_or_else(|| "0.0.0.0".to_string());
  let addr = format!("{hostname}:{port}");
  let listener = std::net::TcpListener::bind(&addr).map_err(rquickjs::Error::Io)?;
  listener.set_nonblocking(true).map_err(rquickjs::Error::Io)?;
  let listener = TcpListener::from_std(listener).map_err(rquickjs::Error::Io)?;

  let (shutdown_tx, _) = watch::channel(false);
  let shared = Arc::new(ServerShared { shutdown: shutdown_tx });

  // Build the handle up front so the same `Server` is both returned to the
  // caller and passed as the second `fetch(req, server)` argument.
  let server = Class::instance(ctx.clone(), Server { shared: shared.clone(), port, hostname })?;
  let handlers = Handlers { fetch_fn, error_fn, routes, server: server.clone() };

  pending.hold();
  let ctx_for_server = ctx.clone();
  let pending_for_loop = pending.clone();
  ctx.spawn(async move {
    run_server(ctx_for_server, listener, handlers, logger, shared, pending_for_loop).await;
  });

  Ok(server)
}

pub(crate) fn init_serve<'js>(ctx: &Ctx<'js>, flux: &Object<'js>) {
  let serve_fn = Function::new(ctx.clone(), serve_impl).expect("create Flux.serve function");
  flux.set("serve", serve_fn).expect("set Flux.serve");
}
