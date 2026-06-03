use bytes::Bytes;
use futures_core::Stream;
use rquickjs::{function::MutFn, promise::Promised, Ctx, Function, IntoJs, Object, TypedArray, Value};
use std::io;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll};
use tokio::sync::mpsc;

use crate::logger::CtxLogger;
use crate::pending::PendingOps;
use crate::plugins::body::{is_async_iterable, pump_async_iterable};
use crate::plugins::http::{reqwest_err, HttpClient};
use crate::plugins::response::response_from_parts;

/// Bridges the mpsc receiver fed by `pump_async_iterable` into a `futures` stream
/// so reqwest can send it as a streamed (chunked) request body. The mirror of
/// serve.rs's `ChannelBody`, but for `futures::Stream` rather than `hyper::Body`.
struct ChunkStream {
  rx: mpsc::Receiver<Bytes>,
}

impl Stream for ChunkStream {
  type Item = Result<Bytes, io::Error>;

  fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
    self.rx.poll_recv(cx).map(|chunk| chunk.map(Ok))
  }
}

pub struct ResponseData {
  pub status: u16,
  pub status_text: String,
  pub url: String,
  pub headers: Vec<(String, String)>,
  pub body: Vec<u8>,
}

fn headers_to_pairs(headers: &reqwest::header::HeaderMap) -> Vec<(String, String)> {
  headers
    .iter()
    .filter_map(|(name, value)| value.to_str().ok().map(|v| (name.as_str().to_string(), v.to_string())))
    .collect()
}

pub(crate) fn init_fetch(ctx: &Ctx<'_>) {
  let globals = ctx.globals();

  let fetch_fn = Function::new(
    ctx.clone(),
    MutFn::from(
      |ctx: Ctx<'_>, url: String, opts: rquickjs::function::Opt<Object<'_>>| -> rquickjs::Result<Promised<_>> {
        let client = ctx.userdata::<HttpClient>().expect("http client").0.clone();
        let pending = ctx.userdata::<PendingOps>().expect("pending ops").clone();

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
              let (tx, rx) = mpsc::channel::<Bytes>(16);
              let pump_ctx = stream_ctx.clone();
              stream_ctx.spawn(async move {
                pump_async_iterable(pump_ctx, iterable, tx, logger).await;
              });
              Some(reqwest::Body::wrap_stream(ChunkStream { rx }))
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
            for key in h.keys::<String>() {
              if let Ok(key) = key {
                if let Ok(Some(val)) = h.get::<_, Option<String>>(&key) {
                  out.push((key, val));
                }
              }
            }
            out
          })
          .unwrap_or_default();

        Ok(Promised(async move {
          pending.hold();
          let r = do_fetch(client, &method, &url, headers, body).await;
          pending.release();
          r
        }))
      },
    ),
  )
  .expect("create fetch function");

  globals.set("fetch", fetch_fn).expect("set fetch global");
}

/// Send an HTTP request and read the response into a `ResponseData`. The `body`
/// may be buffered (`reqwest::Body::from(bytes)`) or streamed
/// (`reqwest::Body::wrap_stream(..)`); `None` sends no body.
pub async fn do_fetch(
  client: Rc<reqwest::Client>,
  method: &str,
  url: &str,
  headers: Vec<(String, String)>,
  body: Option<reqwest::Body>,
) -> rquickjs::Result<ResponseData> {
  let mut req = match method {
    "GET" => client.get(url),
    "POST" => client.post(url),
    "PUT" => client.put(url),
    "DELETE" => client.delete(url),
    "PATCH" => client.patch(url),
    "HEAD" => client.head(url),
    _ => client.request(
      method.parse().map_err(|_| {
        rquickjs::Error::Io(io::Error::new(io::ErrorKind::InvalidInput, format!("invalid HTTP method: {}", method)))
      })?,
      url,
    ),
  };

  for (key, val) in &headers {
    req = req.header(key.as_str(), val.as_str());
  }

  if let Some(body) = body {
    req = req.body(body);
  }

  let resp = req.send().await.map_err(reqwest_err)?;
  let status = resp.status();
  let resp_url = resp.url().to_string();
  let resp_headers = headers_to_pairs(resp.headers());
  let resp_body = resp.bytes().await.map_err(reqwest_err)?;

  Ok(ResponseData {
    status: status.as_u16(),
    // The canonical reason phrase for the code. reqwest/http does not retain the
    // wire reason phrase, so this is derived from the status (covers all standard
    // codes; empty for non-standard ones).
    status_text: status.canonical_reason().unwrap_or("").to_string(),
    url: resp_url,
    headers: resp_headers,
    body: resp_body.to_vec(),
  })
}

impl<'js> IntoJs<'js> for ResponseData {
  fn into_js(self, ctx: &Ctx<'js>) -> rquickjs::Result<Value<'js>> {
    response_from_parts(ctx, self.body, self.status, self.status_text, self.url, self.headers)?.into_js(ctx)
  }
}
