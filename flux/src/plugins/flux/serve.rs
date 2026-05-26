use rquickjs::{function::MutFn, Ctx, Function, Object};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use crate::pending::PendingOps;

const RESPONSE: &[u8] = b"HTTP/1.1 200 OK\r\n\
Content-Type: text/plain\r\n\
Content-Length: 12\r\n\
Connection: close\r\n\
\r\n\
hello, world";

async fn run_server(listener: TcpListener) {
  loop {
    let (mut sock, _) = match listener.accept().await {
      Ok(v) => v,
      Err(e) => {
        log::warn!("[flux] serve accept error: {e}");
        continue;
      }
    };
    tokio::spawn(async move {
      let mut buf = [0u8; 1024];
      // Drain until end-of-headers or buffer fills; we don't parse the request.
      loop {
        match sock.read(&mut buf).await {
          Ok(0) => break,
          Ok(n) => {
            if buf[..n].windows(4).any(|w| w == b"\r\n\r\n") {
              break;
            }
          }
          Err(_) => return,
        }
      }
      let _ = sock.write_all(RESPONSE).await;
      let _ = sock.shutdown().await;
    });
  }
}

pub(crate) fn init_serve<'js>(ctx: &Ctx<'js>, flux: &Object<'js>) {
  let serve_fn = Function::new(
    ctx.clone(),
    MutFn::from(|ctx: Ctx<'_>, opts: Object<'_>| -> rquickjs::Result<()> {
      let port: u16 = opts.get("port")?;
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
        run_server(listener).await;
      });
      Ok(())
    }),
  )
  .expect("create Flux.serve function");
  flux.set("serve", serve_fn).expect("set Flux.serve");
}