//! `createHttpClient` of `flux:http`: a `fetch` client on a TLS config of its
//! own (extra roots, a client certificate, a pin), used as
//! `fetch(url, { client })` and `new WebSocket(url, { client })`.
//! Marshalling only: the config is
//! `forge::tls::client_config`, the client `forge::fetch::Client::with_tls`.
//! Every client of an engine shares the engine's per-host limits, which the
//! `fetch` global holds.

use std::cell::RefCell;
use std::sync::Arc;

use rquickjs::class::Trace;
use rquickjs::{Class, Ctx, Exception, JsLifetime, Object};

use crate::plugins::marshal::OptArg;

/// The JS `HttpClient`: a `forge::fetch::Client` until `close()`, and the
/// TLS config it was built from, which a `WebSocket` opened with
/// `{ client }` connects with.
#[derive(Trace, JsLifetime)]
#[rquickjs::class(rename = "HttpClient")]
pub struct FetchClient {
  #[qjs(skip_trace)]
  client: RefCell<Option<forge::fetch::Client>>,
  #[qjs(skip_trace)]
  tls: Arc<forge::tls::ClientConfig>,
}

impl FetchClient {
  /// The client to send through; `None` once closed.
  pub(crate) fn client(&self) -> Option<forge::fetch::Client> {
    self.client.borrow().clone()
  }

  /// The TLS config of the client; `None` once closed.
  pub(crate) fn tls(&self) -> Option<Arc<forge::tls::ClientConfig>> {
    self.client.borrow().as_ref().map(|_| self.tls.clone())
  }
}

#[rquickjs::methods]
impl FetchClient {
  /// Close the client and release its connections. A later fetch through it
  /// rejects; a fetch in flight finishes.
  pub fn close(&self) -> rquickjs::Result<()> {
    self.client.borrow_mut().take();
    Ok(())
  }
}

/// `createHttpClient({ caCerts?, cert?, key?, pin? })` -> `HttpClient`. The
/// certificates are checked here, once, so a malformed one throws.
pub(crate) fn create_http_client<'js>(
  ctx: Ctx<'js>,
  opts: OptArg<Object<'js>>,
) -> rquickjs::Result<Class<'js, FetchClient>> {
  let throw = |m: String| Exception::throw_message(&ctx, &format!("createHttpClient: {m}"));
  let options = crate::forge_plugins::net::tls_client_options(&ctx, "createHttpClient", &opts)?;
  let config = forge::tls::client_config(&options).map_err(throw)?;
  let agent = crate::standards_plugins::http::user_agent(&ctx);
  let client = forge::fetch::Client::with_tls(&agent, &config).map_err(throw)?;
  Class::instance(ctx.clone(), FetchClient { client: RefCell::new(Some(client)), tls: config })
}
