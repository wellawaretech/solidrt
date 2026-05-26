use bytes::Bytes;
use http_body_util::Full;
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use rquickjs::{function::MutFn, Ctx, Function, Object, TypedArray, Value};
use tokio::net::TcpListener;

use crate::logger::{CtxLogger, Logger};
use crate::pending::PendingOps;

const RESPONSE_CLASS_JS: &str = r#"
globalThis.Response = class Response {
  constructor(body, init) {
    init = init || {};
    this.body = body;
    this.status = init.status || 200;
    this.statusText = init.statusText || "";
    this.headers = init.headers || {};
  }
  static json(obj, init) {
    init = init || {};
    let headers = Object.assign({}, init.headers || {});
    let hasCT = false;
    for (let k in headers) {
      if (k.toLowerCase() === "content-type") { hasCT = true; break; }
    }
    if (!hasCT) headers["Content-Type"] = "application/json";
    return new Response(JSON.stringify(obj), {
      status: init.status,
      statusText: init.statusText,
      headers,
    });
  }
};
"#;

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

fn text_response(status: StatusCode, body: &str) -> Response<Full<Bytes>> {
  Response::builder()
    .status(status)
    .header("Content-Type", "text/plain")
    .body(Full::new(Bytes::copy_from_slice(body.as_bytes())))
    .expect("build response")
}

fn body_bytes_from_value(val: &Value<'_>) -> Option<Bytes> {
  if val.is_null() || val.is_undefined() {
    return Some(Bytes::new());
  }
  if let Some(s) = val.as_string() {
    return Some(Bytes::from(s.to_string().ok()?.into_bytes()));
  }
  if let Ok(ta) = TypedArray::<u8>::from_value(val.clone()) {
    let bytes = ta.as_bytes()?.to_vec();
    return Some(Bytes::from(bytes));
  }
  None
}

fn response_from_object<'js>(
  obj: &Object<'js>,
  logger: &Logger,
) -> Response<Full<Bytes>> {
  let status_u16: u16 = obj.get("status").unwrap_or(200);
  let status =
    StatusCode::from_u16(status_u16).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);

  let body_val: Value<'_> = obj.get("body").unwrap_or_else(|_| Value::new_undefined(obj.ctx().clone()));
  let body = match body_bytes_from_value(&body_val) {
    Some(b) => b,
    None => {
      logger.warn("[flux] serve Response body must be string, Uint8Array, null, or undefined");
      return text_response(StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error");
    }
  };

  let mut builder = Response::builder().status(status);
  let mut has_content_type = false;

  if let Ok(headers) = obj.get::<_, Object<'_>>("headers") {
    for key in headers.keys::<String>().flatten() {
      if let Ok(Some(val)) = headers.get::<_, Option<String>>(&key) {
        if key.eq_ignore_ascii_case("content-type") {
          has_content_type = true;
        }
        builder = builder.header(key.as_str(), val.as_str());
      }
    }
  }

  if !has_content_type {
    builder = builder.header("Content-Type", "text/plain");
  }

  builder
    .body(Full::new(body))
    .unwrap_or_else(|_| text_response(StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error"))
}

fn response_from_value<'js>(val: Value<'js>, logger: &Logger) -> Response<Full<Bytes>> {
  if let Some(s) = val.as_string() {
    let s = s.to_string().unwrap_or_default();
    return text_response(StatusCode::OK, &s);
  }
  if let Some(obj) = val.as_object() {
    return response_from_object(obj, logger);
  }
  logger.warn("[flux] serve fetch must return a string or a Response");
  text_response(StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error")
}

fn handle_request<'js>(
  req: &Request<Incoming>,
  fetch_fn: Option<&Function<'js>>,
  logger: &Logger,
) -> Response<Full<Bytes>> {
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
  if let Err(e) = ctx.eval::<(), _>(RESPONSE_CLASS_JS) {
    panic!("install Response class: {e}");
  }

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