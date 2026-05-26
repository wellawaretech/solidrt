use rquickjs::{function::MutFn, Ctx, Function, Object};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use crate::logger::{CtxLogger, Logger};
use crate::pending::PendingOps;

const MAX_HEADERS_BYTES: usize = 64 * 1024;

struct RequestLine {
  method: String,
  url: String,
}

async fn read_request<R: AsyncReadExt + Unpin>(sock: &mut R) -> Option<RequestLine> {
  let mut buf = Vec::with_capacity(1024);
  let mut tmp = [0u8; 1024];
  loop {
    match sock.read(&mut tmp).await {
      Ok(0) => return None,
      Ok(n) => {
        buf.extend_from_slice(&tmp[..n]);
        if buf.windows(4).any(|w| w == b"\r\n\r\n") {
          break;
        }
        if buf.len() > MAX_HEADERS_BYTES {
          return None;
        }
      }
      Err(_) => return None,
    }
  }
  let line_end = buf.windows(2).position(|w| w == b"\r\n")?;
  let line = std::str::from_utf8(&buf[..line_end]).ok()?;
  let mut parts = line.splitn(3, ' ');
  let method = parts.next()?.to_string();
  let url = parts.next()?.to_string();
  if method.is_empty() || url.is_empty() {
    return None;
  }
  Some(RequestLine { method, url })
}

fn build_response(status: u16, status_text: &str, body: &str) -> Vec<u8> {
  format!(
    "HTTP/1.1 {} {}\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
    status,
    status_text,
    body.len(),
    body,
  )
  .into_bytes()
}

fn build_request_obj<'js>(
  ctx: &Ctx<'js>,
  req: &RequestLine,
) -> rquickjs::Result<Object<'js>> {
  let obj = Object::new(ctx.clone())?;
  obj.set("method", req.method.as_str())?;
  obj.set("url", req.url.as_str())?;
  Ok(obj)
}

async fn run_server<'js>(
  listener: TcpListener,
  fetch_fn: Option<Function<'js>>,
  logger: Logger,
) {
  loop {
    let (mut sock, _) = match listener.accept().await {
      Ok(v) => v,
      Err(e) => {
        logger.warn(&format!("[flux] serve accept error: {e}"));
        continue;
      }
    };

    let req = match read_request(&mut sock).await {
      Some(r) => r,
      None => {
        let resp = build_response(400, "Bad Request", "Bad Request");
        let _ = sock.write_all(&resp).await;
        let _ = sock.shutdown().await;
        continue;
      }
    };

    logger.log(&format!("[flux] serve {} {}", req.method, req.url));

    let resp = match &fetch_fn {
      Some(f) => {
        let ctx = f.ctx().clone();
        match build_request_obj(&ctx, &req)
          .and_then(|req_obj| f.call::<(Object<'_>,), String>((req_obj,)))
        {
          Ok(s) => build_response(200, "OK", &s),
          Err(e) => {
            logger.warn(&format!("[flux] serve fetch callback error: {e}"));
            build_response(500, "Internal Server Error", "Internal Server Error")
          }
        }
      }
      None => build_response(404, "Not Found", "Not Found"),
    };

    let _ = sock.write_all(&resp).await;
    let _ = sock.shutdown().await;
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