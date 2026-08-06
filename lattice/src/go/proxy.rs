// HTTP-backed fetch implementation that routes through the cli's dev server
// (--proxy-http). Global fetch is rewritten to call the cli's /__proxy__
// endpoint with the original URL in the X-SRT-Proxy-Url header; the cli
// forwards the request and relays the response (with its sqlite cache in
// front).
//
// A file proxy (--proxy-files, whole flux:fs module override) lived here
// until 2026-07-21; the assets pipeline in the version store replaced it.

use flux::rquickjs::{
  function::{MutFn, Opt},
  promise::Promised,
  Ctx, Function, JsLifetime, Object, Value,
};
use flux::{do_fetch, header_pairs_from_init, request_body_from_value, JsResponseData, JsResult};
use std::rc::Rc;

#[derive(Clone, JsLifetime)]
struct ProxyState {
  #[qjs(skip_trace)]
  client: Rc<reqwest::Client>,
}

pub fn install_proxy_state(ctx: Ctx<'_>, dev_server: String) {
  let client = reqwest::Client::builder().user_agent("lattice-go-proxy").build().expect("build proxy http client");
  ctx.store_userdata(ProxyState { client: Rc::new(client) }).expect("store proxy state");

  let proxy_url = Rc::new(format!("http://{dev_server}/__proxy__"));
  let fetch_fn = Function::new(
    ctx.clone(),
    MutFn::from({
      let proxy_url = proxy_url.clone();
      move |ctx: Ctx<'_>, url: String, opts: Opt<Object<'_>>| -> flux::rquickjs::Result<Promised<_>> {
        let state = ctx.userdata::<ProxyState>().expect("proxy state").clone();

        let method = opts
          .0
          .as_ref()
          .and_then(|o| o.get::<_, Option<String>>("method").ok().flatten())
          .unwrap_or_else(|| "GET".to_string())
          .to_uppercase();

        // Body and headers marshal exactly like the standard fetch (flux's
        // helpers), so a Headers instance, a streamed async-iterable body, and
        // the throw-on-unsupported-value rules behave the same through the proxy.
        let body = match opts.0.as_ref().and_then(|o| o.get::<_, Value>("body").ok()) {
          Some(val) => request_body_from_value(val)?,
          None => None,
        };

        let mut headers = match opts.0.as_ref().and_then(|o| o.get::<_, Value>("headers").ok()) {
          Some(val) => header_pairs_from_init(&val)?,
          None => Vec::new(),
        };
        headers.push(("x-srt-proxy-url".to_string(), url));

        let proxy_url = (*proxy_url).clone();
        let client = state.client.clone();
        Ok(Promised(
          async move { JsResult(do_fetch(client, &method, &proxy_url, headers, body).await.map(JsResponseData)) },
        ))
      }
    }),
  )
  .expect("create proxy fetch");
  ctx.globals().set("fetch", fetch_fn).expect("override fetch global");

  log::info!("[sgo] Installed http proxy -> http://{dev_server}/");
}
