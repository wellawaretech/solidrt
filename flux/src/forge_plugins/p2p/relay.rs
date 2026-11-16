//! `serveRelay`: the self-hosted relay, marshalled over `forge::p2p::Relay`.
//! Only in a build with the `p2p-relay` feature, which `Flux.capabilities`
//! reports as "p2p-relay"; without it the export is absent, like `record`
//! without `video-encode`.

use std::cell::Cell;
use std::future::Future;

use rquickjs::class::Trace;
use rquickjs::function::Opt;
use rquickjs::promise::Promised;
use rquickjs::{Ctx, Exception, JsLifetime, Object, Value};

use crate::pending::{Hold, PendingOps};
use crate::plugins::js_error::JsResult;
use crate::plugins::marshal::{with_in_flight, OptArg};
use forge::p2p::{Relay, RelayOptions, RelayTls};

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

  /// `setAllow(ids | null)`: replace the allowlist (`null` admits any
  /// endpoint with a token); connected endpoints no longer allowed are
  /// disconnected. The argument is required, so a call that forgot it does
  /// not open the relay: missing or `undefined` throws, and so does a
  /// malformed endpoint id (forge prefixes its errors `setAllow:`).
  #[qjs(rename = "setAllow")]
  pub fn set_allow<'js>(&self, ctx: Ctx<'js>, ids: Opt<Value<'js>>) -> rquickjs::Result<()> {
    let ids = match ids.0 {
      Some(ids) if ids.is_null() => None,
      Some(ids) if !ids.is_undefined() => Some(
        ids
          .get::<Vec<String>>()
          .map_err(|_| Exception::throw_message(&ctx, "setAllow: ids must be an array of strings or null"))?,
      ),
      _ => return Err(Exception::throw_message(&ctx, "setAllow: takes the endpoint ids, or null to admit any")),
    };
    self.inner.set_allow(ids).map_err(|m| Exception::throw_message(&ctx, &m))
  }

  /// The endpoint ids connected to the relay now.
  pub fn clients(&self) -> Vec<String> {
    self.inner.clients()
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

/// `serveRelay(opts?)` -> `Promise<Relay>`: `port` (0, the default, is a
/// free one), `host`, `token` (a string or a list), `allow`, `url` and
/// `tls: { cert, key, pinned? }`, read into `forge::p2p::RelayOptions` and
/// validated there before anything binds, so an invalid option throws. The
/// `tls` object of `generateCertificate` passes as it is: its `pin` (the
/// hex string) is not read. The relay is standing work until its `close`.
pub fn serve_relay<'js>(
  ctx: Ctx<'js>,
  opts: OptArg<Object<'js>>,
) -> rquickjs::Result<Promised<impl Future<Output = JsResult<P2pRelay>>>> {
  let throw = |m: &str| Exception::throw_message(&ctx, &format!("serveRelay: {m}"));
  let options = match opts.0 {
    Some(opts) => relay_options(&ctx, &opts)?,
    None => RelayOptions::default(),
  };
  let setup = options.check().map_err(|m| throw(&m))?;
  let hold = PendingOps::of(&ctx).standing("p2p relay");
  Ok(with_in_flight(&ctx, "p2p", async move {
    Relay::start(setup).await.map(|inner| P2pRelay { inner, hold: Cell::new(Some(hold)) })
  }))
}

/// The JS options object as `RelayOptions`; the values are checked by
/// forge (`RelayOptions::check`). Throws, prefixed `serveRelay:`, on what
/// is not an option of a relay.
fn relay_options<'js>(ctx: &Ctx<'js>, opts: &Object<'js>) -> rquickjs::Result<RelayOptions> {
  let throw = |m: &str| Exception::throw_message(ctx, &format!("serveRelay: {m}"));
  let tokens = match opts.get::<_, Option<Value<'js>>>("token")? {
    None => None,
    Some(v) if v.is_string() => Some(vec![v.get::<String>()?]),
    Some(v) => Some(v.get::<Vec<String>>().map_err(|_| throw("token must be a string or an array of strings"))?),
  };
  let tls = match opts.get::<_, Option<Object<'js>>>("tls")? {
    Some(tls) => {
      // Relay clients present no certificate: client authentication
      // options do not apply.
      for key in ["caCerts", "anyClientCert"] {
        if tls.get::<_, Option<Value<'js>>>(key)?.is_some() {
          return Err(throw(&format!("tls: {key} does not apply to a relay (its clients present no certificate)")));
        }
      }
      let (Some(cert), Some(key)) = (tls.get::<_, Option<String>>("cert")?, tls.get::<_, Option<String>>("key")?)
      else {
        return Err(throw("tls: cert and key are required"));
      };
      Some(RelayTls { cert: cert.into_bytes(), key: key.into_bytes(), pinned: tls.get("pinned")? })
    }
    None => None,
  };
  Ok(RelayOptions {
    port: opts.get::<_, Option<u16>>("port")?.unwrap_or(0),
    host: opts.get("host")?,
    tokens,
    allow: opts.get("allow")?,
    url: opts.get("url")?,
    tls,
  })
}
