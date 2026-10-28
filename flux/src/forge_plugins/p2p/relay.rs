//! `serveRelay`: the self-hosted relay, marshalled over `forge::p2p::Relay`.
//! Only in a build with the `p2p-relay` feature, which `Flux.capabilities`
//! reports as "p2p-relay"; without it the export is absent, like `record`
//! without `video-encode`.

use std::cell::Cell;
use std::future::Future;

use rquickjs::class::Trace;
use rquickjs::promise::Promised;
use rquickjs::{Ctx, JsLifetime, Object};

use crate::pending::{Hold, PendingOps};
use crate::plugins::js_error::JsResult;
use crate::plugins::marshal::{with_in_flight, OptArg};
use forge::p2p::Relay;

/// The `flux:p2p` `Relay`: a thin wrapper over the forge relay, holding
/// the engine (standing) while it serves.
#[derive(Trace, JsLifetime)]
#[rquickjs::class(rename = "Relay")]
pub struct P2pRelay {
  #[qjs(skip_trace)]
  inner: Relay,
  #[qjs(skip_trace)]
  hold: Cell<Option<Hold>>,
}

#[rquickjs::methods]
impl P2pRelay {
  /// The port the relay listens on.
  #[qjs(get)]
  pub fn port(&self) -> u16 {
    self.inner.port()
  }

  /// The relay URL peers pass as `relayUrl`; see `forge::p2p::Relay::serve`.
  #[qjs(get)]
  pub fn url(&self) -> String {
    self.inner.url().to_string()
  }

  /// Stop the relay; the connections it carries end.
  pub fn close<'js>(&self, ctx: Ctx<'js>) -> rquickjs::Result<Promised<impl Future<Output = JsResult<()>>>> {
    let inner = self.inner.clone();
    let hold = self.hold.take();
    Ok(with_in_flight(&ctx, "p2p", async move {
      inner.close().await;
      drop(hold);
      Ok::<(), String>(())
    }))
  }
}

/// `serveRelay({ port? })` -> `Promise<Relay>`; `port` 0 (the default) is
/// a free one. The relay is standing work until its `close`.
pub fn serve_relay<'js>(
  ctx: Ctx<'js>,
  opts: OptArg<Object<'js>>,
) -> rquickjs::Result<Promised<impl Future<Output = JsResult<P2pRelay>>>> {
  let port = match opts.0 {
    Some(opts) => opts.get::<_, Option<u16>>("port")?.unwrap_or(0),
    None => 0,
  };
  let hold = PendingOps::of(&ctx).standing("p2p relay");
  Ok(with_in_flight(&ctx, "p2p", async move {
    Relay::serve(port).await.map(|inner| P2pRelay { inner, hold: Cell::new(Some(hold)) })
  }))
}
