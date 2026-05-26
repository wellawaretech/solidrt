use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request as HyperRequest, Response as HyperResponse, StatusCode};
use hyper_util::rt::TokioIo;
use rquickjs::promise::MaybePromise;
use rquickjs::{function::MutFn, Class, Ctx, Function, Object, Value};
use tokio::net::{TcpListener, TcpStream};

use crate::logger::{CtxLogger, Logger};
use crate::pending::PendingOps;
use crate::plugins::flux::request::{request_from_parts, Request};
use crate::plugins::flux::response::Response;

fn text_response(status: StatusCode, body: &str) -> HyperResponse<Full<Bytes>> {
  HyperResponse::builder()
    .status(status)
    .header("Content-Type", "text/plain")
    .body(Full::new(Bytes::copy_from_slice(body.as_bytes())))
    .expect("build response")
}

fn response_from_native<'js>(r: &Response<'js>) -> HyperResponse<Full<Bytes>> {
  let status =
    StatusCode::from_u16(r.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
  let mut builder = HyperResponse::builder().status(status);
  let mut has_content_type = false;
  let headers = r.headers.borrow().entries();
  for (k, v) in &headers {
    if k.eq_ignore_ascii_case("content-type") {
      has_content_type = true;
    }
    builder = builder.header(k.as_str(), v.as_str());
  }
  if !has_content_type {
    builder = builder.header("Content-Type", "text/plain");
  }
  let body = r.body.take().unwrap_or_default();
  builder
    .body(Full::new(Bytes::from(body)))
    .unwrap_or_else(|_| text_response(StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error"))
}

fn response_from_value<'js>(
  val: Value<'js>,
  logger: &Logger,
) -> HyperResponse<Full<Bytes>> {
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

async fn handle_request<'js>(
  req: HyperRequest<Incoming>,
  fetch_fn: Option<&Function<'js>>,
  logger: &Logger,
) -> HyperResponse<Full<Bytes>> {
  let method = req.method().as_str().to_string();
  let url = req.uri().to_string();
  let headers: Vec<(String, String)> = req
    .headers()
    .iter()
    .filter_map(|(name, value)| {
      value.to_str().ok().map(|v| (name.as_str().to_string(), v.to_string()))
    })
    .collect();
  logger.log(&format!("[flux] serve {} {}", method, url));

  let body_bytes = match req.into_body().collect().await {
    Ok(collected) => collected.to_bytes().to_vec(),
    Err(e) => {
      logger.warn(&format!("[flux] serve request body read error: {e}"));
      return text_response(StatusCode::BAD_REQUEST, "Bad Request");
    }
  };

  let f = match fetch_fn {
    Some(f) => f,
    None => return text_response(StatusCode::NOT_FOUND, "Not Found"),
  };

  let ctx = f.ctx().clone();
  let req_class = match request_from_parts(&ctx, method, url, body_bytes, headers) {
    Ok(c) => c,
    Err(e) => {
      logger.warn(&format!("[flux] serve build Request error: {e}"));
      return text_response(StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error");
    }
  };

  let val = match f.call::<(Class<'_, Request<'_>>,), Value<'_>>((req_class,)) {
    Ok(v) => v,
    Err(e) => {
      logger.warn(&format!("[flux] serve fetch callback error: {e}"));
      return text_response(StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error");
    }
  };

  let resolved = match MaybePromise::from_value(val)
    .into_future::<Value<'_>>()
    .await
  {
    Ok(v) => v,
    Err(e) => {
      logger.warn(&format!("[flux] serve fetch rejected: {e}"));
      return text_response(StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error");
    }
  };

  response_from_value(resolved, logger)
}

async fn serve_one_connection<'js>(
  sock: TcpStream,
  fetch_fn: Option<Function<'js>>,
  logger: Logger,
) {
  let io = TokioIo::new(sock);
  let service = service_fn(|req: HyperRequest<Incoming>| {
    let fetch_fn = fetch_fn.clone();
    let logger = logger.clone();
    async move { Ok::<_, std::convert::Infallible>(handle_request(req, fetch_fn.as_ref(), &logger).await) }
  });

  if let Err(e) = http1::Builder::new().serve_connection(io, service).await {
    logger.warn(&format!("[flux] serve connection error: {e}"));
  }
}

async fn run_server<'js>(
  ctx: Ctx<'js>,
  listener: TcpListener,
  fetch_fn: Option<Function<'js>>,
  logger: Logger,
) {
  loop {
    let (sock, _) = match listener.accept().await {
      Ok(v) => v,
      Err(e) => {
        logger.warn(&format!("[flux] serve accept error: {e}"));
        continue;
      }
    };
    let fetch_fn = fetch_fn.clone();
    let logger = logger.clone();
    ctx.spawn(async move {
      serve_one_connection(sock, fetch_fn, logger).await;
    });
  }
}

pub(crate) fn init_serve<'js>(ctx: &Ctx<'js>, flux: &Object<'js>) {
  let serve_fn = Function::new(
    ctx.clone(),
    MutFn::from(|opts: Object<'_>| -> rquickjs::Result<()> {
      let ctx = opts.ctx().clone();
      let port: u16 = opts.get("port")?;
      let fetch_fn: Option<Function<'_>> = opts.get("fetch").ok();
      let pending = ctx
        .userdata::<PendingOps>()
        .expect("pending ops")
        .clone();
      let logger = ctx.logger();

      let addr = format!("0.0.0.0:{port}");
      let listener = std::net::TcpListener::bind(&addr).map_err(rquickjs::Error::Io)?;
      listener.set_nonblocking(true).map_err(rquickjs::Error::Io)?;
      let listener = TcpListener::from_std(listener).map_err(rquickjs::Error::Io)?;

      pending.hold();
      let ctx_for_server = ctx.clone();
      ctx.spawn(async move {
        run_server(ctx_for_server, listener, fetch_fn, logger).await;
      });
      Ok(())
    }),
  )
  .expect("create Flux.serve function");
  flux.set("serve", serve_fn).expect("set Flux.serve");
}