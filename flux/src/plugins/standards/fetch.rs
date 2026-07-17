use rquickjs::{function::MutFn, promise::Promised, Ctx, Exception, Function, IntoJs, JsLifetime, Object, TypedArray, Value};
use std::path::PathBuf;
use std::rc::Rc;

use crate::logger::CtxLogger;
use crate::plugins::marshal::with_pending;
use crate::plugins::standards::body::{is_async_iterable, pump_async_iterable};
use crate::plugins::standards::http::HttpClient;
use crate::plugins::standards::response::response_from_parts;
use forge::cache::Cache;
use forge::fetch::{channel_request_body, do_fetch, do_fetch_cached, CacheMode, HostLimits, ResponseData};

/// Placeholder cap until a real default is decided (plan open question).
const FETCH_CACHE_MAX_BYTES: u64 = 256 * 1024 * 1024;

/// Browser convention for per-host connection concurrency; applies to cached
/// (asset-mode) fetches only.
const FETCHES_PER_HOST: usize = 6;

/// Builder-provided fetch cache directory (`FluxEngineBuilder::cache_dir`).
#[derive(Clone, JsLifetime)]
pub struct FetchCacheDir(#[qjs(skip_trace)] pub PathBuf);

pub(crate) fn init_fetch(ctx: &Ctx<'_>) {
  let globals = ctx.globals();

  let cache: Option<Rc<Cache>> =
    ctx.userdata::<FetchCacheDir>().map(|dir| Rc::new(Cache::new(dir.0.clone(), FETCH_CACHE_MAX_BYTES)));
  let limits = Rc::new(HostLimits::new(FETCHES_PER_HOST));

  let fetch_fn = Function::new(
    ctx.clone(),
    MutFn::from(
      move |ctx: Ctx<'_>, url: String, opts: rquickjs::function::Opt<Object<'_>>| -> rquickjs::Result<Promised<_>> {
        let client = ctx.userdata::<HttpClient>().expect("http client").0.clone();

        let cache_mode: Option<CacheMode> =
          match opts.0.as_ref().and_then(|o| o.get::<_, Option<String>>("cache").ok().flatten()) {
            None => None,
            Some(v) => match v.as_str() {
              "force-cache" => Some(CacheMode::ForceCache),
              "reload" => Some(CacheMode::Reload),
              // The rest of the standard vocabulary all means "just hit the
              // network" in this model (no freshness, no revalidation).
              "default" | "no-store" | "no-cache" => None,
              _ => return Err(Exception::throw_message(&ctx, &format!("Unknown cache mode: {v}"))),
            },
          };
        let cache = cache.clone();
        let limits = limits.clone();

        let method = opts
          .0
          .as_ref()
          .and_then(|o| o.get::<_, Option<String>>("method").ok().flatten())
          .unwrap_or_else(|| "GET".to_string())
          .to_uppercase();

        // Buffered bodies (string, Uint8Array) are checked first so they pay no
        // eval. An async-iterable body is streamed: a task drives it into a
        // channel that reqwest sends as a chunked body (see `pump_async_iterable`).
        let body: Option<reqwest::Body> = match opts.0.as_ref().and_then(|o| o.get::<_, Value>("body").ok()) {
          Some(val) if !(val.is_null() || val.is_undefined()) => {
            if let Some(s) = val.as_string() {
              Some(reqwest::Body::from(s.to_string()?.into_bytes()))
            } else if let Ok(ta) = TypedArray::<u8>::from_value(val.clone()) {
              Some(reqwest::Body::from(ta.as_bytes().map(|b| b.to_vec()).unwrap_or_default()))
            } else if is_async_iterable(val.ctx(), &val)? {
              // Use the value's own context so its lifetime unifies (the closure
              // gives `ctx` and `opts` independent lifetimes).
              let stream_ctx = val.ctx().clone();
              let logger = stream_ctx.logger();
              let iterable = val.into_object().expect("async iterable is an object");
              let (tx, body) = channel_request_body();
              let pump_ctx = stream_ctx.clone();
              stream_ctx.spawn(async move {
                pump_async_iterable(pump_ctx, iterable, tx, logger).await;
              });
              Some(body)
            } else {
              None
            }
          }
          _ => None,
        };

        let headers: Vec<(String, String)> = opts
          .0
          .as_ref()
          .map(|o| {
            let h: Object = match o.get("headers") {
              Ok(h) => h,
              Err(_) => return Vec::new(),
            };
            let mut out = Vec::new();
            for key in h.keys::<String>().flatten() {
              if let Ok(Some(val)) = h.get::<_, Option<String>>(&key) {
                out.push((key, val));
              }
            }
            out
          })
          .unwrap_or_default();

        Ok(with_pending(&ctx, async move {
          match (cache, cache_mode) {
            (Some(cache), Some(mode)) => do_fetch_cached(client, &method, &url, headers, body, cache, mode, limits).await,
            _ => do_fetch(client, &method, &url, headers, body).await,
          }
          .map(JsResponseData)
        }))
      },
    ),
  )
  .expect("create fetch function");

  globals.set("fetch", fetch_fn).expect("set fetch global");
}

/// Marshalling newtype over the engine-free `forge::fetch::ResponseData`, so its
/// `IntoJs` (building a JS `Response`) stays in this crate once forge is split
/// out - a foreign `IntoJs` on a foreign type would otherwise trip the orphan
/// rule. Public because the lattice dev-server proxy also returns it from its
/// own `fetch`; both the `fetch` global and that proxy `.map(JsResponseData)`
/// the bare `do_fetch` result.
pub struct JsResponseData(pub ResponseData);

impl<'js> IntoJs<'js> for JsResponseData {
  fn into_js(self, ctx: &Ctx<'js>) -> rquickjs::Result<Value<'js>> {
    let r = self.0;
    response_from_parts(ctx, r.body, r.status, r.status_text, r.url, r.headers)?.into_js(ctx)
  }
}
