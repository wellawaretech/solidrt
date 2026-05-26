use rquickjs::{function::MutFn, Ctx, Function, Object};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use crate::pending::PendingOps;

async fn drain_request<R: AsyncReadExt + Unpin>(sock: &mut R) -> bool {
  let mut buf = [0u8; 1024];
  loop {
    match sock.read(&mut buf).await {
      Ok(0) => return true,
      Ok(n) => {
        if buf[..n].windows(4).any(|w| w == b"\r\n\r\n") {
          return true;
        }
      }
      Err(_) => return false,
    }
  }
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

async fn run_server<'js>(listener: TcpListener, fetch_fn: Option<Function<'js>>) {
  loop {
    let (mut sock, _) = match listener.accept().await {
      Ok(v) => v,
      Err(e) => {
        log::warn!("[flux] serve accept error: {e}");
        continue;
      }
    };

    if !drain_request(&mut sock).await {
      continue;
    }

    let resp = match &fetch_fn {
      Some(f) => match f.call::<(), String>(()) {
        Ok(s) => build_response(200, "OK", &s),
        Err(e) => {
          log::warn!("[flux] serve fetch callback error: {e}");
          build_response(500, "Internal Server Error", "Internal Server Error")
        }
      },
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

      let addr = format!("0.0.0.0:{port}");
      let listener = std::net::TcpListener::bind(&addr).map_err(rquickjs::Error::Io)?;
      listener.set_nonblocking(true).map_err(rquickjs::Error::Io)?;
      let listener = TcpListener::from_std(listener).map_err(rquickjs::Error::Io)?;

      pending.hold();
      ctx.spawn(async move {
        run_server(listener, fetch_fn).await;
      });
      Ok(())
    }),
  )
  .expect("create Flux.serve function");
  flux.set("serve", serve_fn).expect("set Flux.serve");
}