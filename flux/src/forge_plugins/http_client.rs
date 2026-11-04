//! `createHttpClient` of `flux:http`: a `fetch` client on a TLS config of its
//! own (extra roots, a client certificate, a pin), used as
//! `fetch(url, { client })`. Marshalling only: the config is
//! `forge::tls::client_config`, the client `forge::fetch::Client::with_tls`.
//! Every client of an engine shares the engine's per-host limits, which the
//! `fetch` global holds.

use std::cell::RefCell;

use rquickjs::class::Trace;
use rquickjs::{Class, Ctx, Exception, JsLifetime, Object};

use crate::plugins::marshal::OptArg;

/// The JS `HttpClient`: a `forge::fetch::Client` until `close()`.
#[derive(Trace, JsLifetime)]
#[rquickjs::class(rename = "HttpClient")]
pub struct FetchClient {
  #[qjs(skip_trace)]
  client: RefCell<Option<forge::fetch::Client>>,
}

impl FetchClient {
  /// The client to send through; `None` once closed.
  pub(crate) fn client(&self) -> Option<forge::fetch::Client> {
    self.client.borrow().clone()
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
  Class::instance(ctx.clone(), FetchClient { client: RefCell::new(Some(client)) })
}
