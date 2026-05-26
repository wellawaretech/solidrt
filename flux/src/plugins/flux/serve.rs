use bytes::Bytes;
use http_body_util::Full;
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response as HyperResponse, StatusCode};
use hyper_util::rt::TokioIo;
use rquickjs::{function::MutFn, Class, Ctx, Function, Object, Value};
use tokio::net::TcpListener;

use crate::logger::{CtxLogger, Logger};
use crate::pending::PendingOps;
use crate::plugins::flux::response::Response;

fn build_request_obj<'js>(
  ctx: &Ctx<'js>,
  method: &str,
  url: &str,
) -> rquickjs::Result<Object<'js>> {
  let obj = Object::new(ctx.clone())?;
  obj.set("method", method)?;
  obj.set("url", url)?;
  Ok(obj)
}

fn text_response(status: StatusCode, body: &str) -> HyperResponse<Full<Bytes>> {
  HyperResponse::builder()
    .status(status)
    .header("Content-Type", "text/plain")
    .body(Full::new(Bytes::copy_from_slice(body.as_bytes())))
    .expect("build response")
}

fn response_from_native(r: &Response) -> HyperResponse<Full<Bytes>> {
  let status =
    StatusCode::from_u16(r.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
  let mut builder = HyperResponse::builder().status(status);
  let mut has_content_type = false;
  for (k, v) in &r.headers {
    if k.eq_ignore_ascii_case("content-type") {
      has_content_type = true;
    }
    builder = builder.header(k.as_str(), v.as_str());
  }
  if !has_content_type {
    builder = builder.header("Content-Type", "text/plain");
  }
  builder
    .body(Full::new(Bytes::from(r.body.clone())))
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

fn handle_request<'js>(
  req: &Request<Incoming>,
  fetch_fn: Option<&Function<'js>>,
  logger: &Logger,
) -> HyperResponse<Full<Bytes>> {
  let method = req.method().as_str();
  let url = req.uri().to_string();
  logger.log(&format!("[flux] serve {} {}", method, url));

  match fetch_fn {
    Some(f) => {
      let ctx = f.ctx().clone();
      match build_request_obj(&ctx, method, &url)
        .and_then(|req_obj| f.call::<(Object<'_>,), Value<'_>>((req_obj,)))
      {
        Ok(val) => response_from_value(val, logger),
        Err(e) => {
          logger.warn(&format!("[flux] serve fetch callback error: {e}"));
          text_response(StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error")
        }
      }
    }
    None => text_response(StatusCode::NOT_FOUND, "Not Found"),
  }
}

async fn run_server<'js>(
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
    let io = TokioIo::new(sock);

    let service = service_fn(|req: Request<Incoming>| {
      let resp = handle_request(&req, fetch_fn.as_ref(), &logger);
      async move { Ok::<_, std::convert::Infallible>(resp) }
    });

    if let Err(e) = http1::Builder::new().serve_connection(io, service).await {
      logger.warn(&format!("[flux] serve connection error: {e}"));
    }
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
      ctx.spawn(async move {
        run_server(listener, fetch_fn, logger).await;
      });
      Ok(())
    }),
  )
  .expect("create Flux.serve function");
  flux.set("serve", serve_fn).expect("set Flux.serve");
}