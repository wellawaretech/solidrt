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
  Ctx, Function, JsLifetime, Object, TypedArray, Value,
};
use flux::{do_fetch, JsResponseData, JsResult};
use std::rc::Rc;

#[derive(Clone, JsLifetime)]
struct ProxyState {
  #[qjs(skip_trace)]
  client: Rc<reqwest::Client>,
}

fn extract_fetch_body<'js>(val: &Value<'js>) -> Option<Vec<u8>> {
  if val.is_null() || val.is_undefined() {
    return None;
  }
  if let Some(s) = val.as_string() {
    return Some(s.to_string().ok()?.into_bytes());
  }
  if let Ok(ta) = TypedArray::<u8>::from_value(val.clone()) {
    return Some(ta.as_bytes().map(|b| b.to_vec()).unwrap_or_default());
  }
  None
}

fn extract_fetch_headers<'js>(opts: &Object<'js>) -> Vec<(String, String)> {
  let h: Object = match opts.get("headers") {
    Ok(h) => h,
    Err(_) => return Vec::new(),
  };
  let mut out = Vec::new();
  for key in h.keys::<String>() {
    if let Ok(key) = key {
      if let Ok(Some(val)) = h.get::<_, Option<String>>(&key) {
        out.push((key, val));
      }
    }
  }
  out
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

        let body = opts.0.as_ref().and_then(|o| o.get::<_, Value>("body").ok()).and_then(|v| extract_fetch_body(&v));

        let mut headers = opts.0.as_ref().map(extract_fetch_headers).unwrap_or_default();
        headers.push(("x-srt-proxy-url".to_string(), url));

        let proxy_url = (*proxy_url).clone();
        let client = state.client.clone();
        Ok(Promised(async move {
          JsResult(
            do_fetch(client, &method, &proxy_url, headers, body.map(reqwest::Body::from)).await.map(JsResponseData),
          )
        }))
      }
    }),
  )
  .expect("create proxy fetch");
  ctx.globals().set("fetch", fetch_fn).expect("override fetch global");

  log::info!("[sgo] Installed http proxy -> http://{dev_server}/");
}
