use rquickjs::{
  function::MutFn, Class, Ctx, Exception, Function, IntoJs, JsLifetime, Object, Promise, TypedArray, Value,
};
use std::path::PathBuf;
use std::rc::Rc;

use crate::logger::CtxLogger;
use crate::pending::PendingOps;
use crate::plugins::marshal::{with_pending, OptArg};
use crate::standards_plugins::abort::AbortSignal;
use crate::standards_plugins::body::{is_async_iterable, pump_async_iterable};
use crate::standards_plugins::headers::header_pairs_from_init;
use crate::standards_plugins::http::HttpClient;
use crate::standards_plugins::response::response_from_parts;
use forge::cache::Cache;
use forge::fetch::{
  channel_request_body, do_fetch, do_fetch_cached, CacheMode, HostLimits, RequestBody, ResponseData,
};

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
    MutFn::from(fetch_builder(
      move |ctx, url: String, opts| {
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

        let body: Option<RequestBody> = match opts.0.as_ref().and_then(|o| o.get::<_, Value>("body").ok()) {
          Some(val) => request_body_from_value(val)?,
          None => None,
        };

        let headers: Vec<(String, String)> = match opts.0.as_ref().and_then(|o| o.get::<_, Value>("headers").ok()) {
          Some(val) => header_pairs_from_init(&val)?,
          None => Vec::new(),
        };

        let signal: Option<Class<AbortSignal>> = match opts.0.as_ref() {
          Some(o) => o
            .get::<_, Option<Class<AbortSignal>>>("signal")
            .map_err(|_| Exception::throw_type(&ctx, "signal must be an AbortSignal"))?,
          None => None,
        };

        let net = async move {
          match (cache, cache_mode) {
            (Some(cache), Some(mode)) => {
              do_fetch_cached(&client, &method, &url, headers, body, cache, mode, limits).await
            }
            _ => do_fetch(&client, &method, &url, headers, body).await,
          }
        };

        let Some(sig) = signal else {
          return with_pending(&ctx, async move { net.await.map(JsResponseData) }).into_js(&ctx);
        };

        // Aborting must reject with the signal's own reason (a JS value), so
        // this path settles the promise from a local task that holds the
        // signal, instead of a `Promised` future (which cannot hold `'js`
        // values). Abort wins the race and drops the request mid-flight; an
        // already-aborted signal rejects without sending anything.
        let (promise, resolve, reject) = Promise::new(&ctx)?;
        let mut abort_rx = sig.borrow().subscribe();
        let pending = ctx.userdata::<PendingOps>().expect("pending ops").clone();
        pending.hold();
        let task_ctx = ctx.clone();
        ctx.spawn(async move {
          tokio::pin!(net);
          tokio::select! {
            biased;
            res = &mut abort_rx => {
              if res.is_ok() {
                let reason = sig.borrow().reason(task_ctx.clone());
                let _ = reject.call::<_, ()>((reason,));
              } else {
                // The sender cannot drop while this task holds the signal;
                // finish the request if it somehow does.
                settle_fetch(&task_ctx, &resolve, &reject, net.await);
              }
            }
            r = &mut net => settle_fetch(&task_ctx, &resolve, &reject, r),
          }
          pending.release();
        });
        Ok(promise.into_value())
      },
    )),
  )
  .expect("create fetch function");

  globals.set("fetch", fetch_fn).expect("set fetch global");
}

/// Forces the HRTB a capturing closure returning a `'js`-bound value needs
/// (see "Ctx and the `'js` lifetime" in flux/CLAUDE.md).
fn fetch_builder<F>(f: F) -> F
where
  F: for<'js> FnMut(Ctx<'js>, String, OptArg<Object<'js>>) -> rquickjs::Result<Value<'js>>,
{
  f
}

/// Settle a manually-built fetch promise: a response resolves it, an error
/// message rejects it with an `Error`.
fn settle_fetch<'js>(ctx: &Ctx<'js>, resolve: &Function<'js>, reject: &Function<'js>, r: Result<ResponseData, String>) {
  let outcome = match r {
    Ok(data) => JsResponseData(data).into_js(ctx).map(Ok),
    Err(msg) => Exception::from_message(ctx.clone(), &msg).map(|e| Err(e.into_value())),
  };
  match outcome {
    Ok(Ok(v)) => {
      let _ = resolve.call::<_, ()>((v,));
    }
    Ok(Err(e)) => {
      let _ = reject.call::<_, ()>((e,));
    }
    Err(e) => ctx.logger().warn(&format!("[flux] fetch: could not build result: {e}")),
  }
}

/// Marshal a fetch request body value into a reqwest Body. Buffered bodies
/// (string, Uint8Array) are checked first so they pay no eval. An async-iterable
/// body is streamed: a task drives it into a channel that reqwest sends as a
/// chunked body (see `pump_async_iterable`). Null/undefined mean no body; any
/// other value throws. Public because the lattice dev-server proxy's fetch
/// marshals its body the same way.
pub fn request_body_from_value<'js>(val: Value<'js>) -> rquickjs::Result<Option<RequestBody>> {
  if val.is_null() || val.is_undefined() {
    return Ok(None);
  }
  if let Some(s) = val.as_string() {
    return Ok(Some(RequestBody::bytes(s.to_string()?.into_bytes())));
  }
  if let Ok(ta) = TypedArray::<u8>::from_value(val.clone()) {
    return Ok(Some(RequestBody::bytes(ta.as_bytes().map(|b| b.to_vec()).unwrap_or_default())));
  }
  if is_async_iterable(val.ctx(), &val)? {
    // Use the value's own context so the result's lifetime unifies with it.
    let stream_ctx = val.ctx().clone();
    let logger = stream_ctx.logger();
    let iterable = val.into_object().expect("async iterable is an object");
    let (tx, body) = channel_request_body();
    let pump_ctx = stream_ctx.clone();
    stream_ctx.spawn(async move {
      pump_async_iterable(pump_ctx, iterable, tx, logger).await;
    });
    return Ok(Some(body));
  }
  Err(Exception::throw_message(
    val.ctx(),
    "Fetch body must be a string, Uint8Array, an async iterable, null, or undefined",
  ))
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
