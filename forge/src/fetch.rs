//! Engine-free `fetch` core.
//!
//! Sends an HTTP request with reqwest and reads the response into a plain
//! `ResponseData`. Names no scripting-engine types: the marshalling layer
//! (`plugins/fetch.rs`) decodes the JS request, builds the request body, and
//! turns `ResponseData` into a JS `Response`. Streamed request bodies are fed
//! through `channel_request_body`, whose sender the marshalling layer drives
//! from a JS async-iterable.

use bytes::Bytes;
use futures_core::Stream;
use serde::{Deserialize, Serialize};
use std::io;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll};
use tokio::sync::mpsc;

use crate::cache::Cache;
use crate::stream::{to_byte_stream, ByteStream};

/// The retained parts of an HTTP response: status, resolved url, headers, and a
/// lazily-drained body stream.
pub struct ResponseData {
  pub status: u16,
  pub status_text: String,
  pub url: String,
  pub headers: Vec<(String, String)>,
  pub body: ByteStream,
}

/// Bridges the mpsc receiver fed by the marshalling layer into a `futures`
/// stream so reqwest can send it as a streamed (chunked) request body. The
/// mirror of forge/http's `ChannelBody`, but for `futures::Stream` rather than
/// `hyper::Body`.
struct ChunkStream {
  rx: mpsc::Receiver<Bytes>,
}

impl Stream for ChunkStream {
  type Item = Result<Bytes, io::Error>;

  fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
    self.rx.poll_recv(cx).map(|chunk| chunk.map(Ok))
  }
}

/// Build a streamed request body and the sender that feeds it. The producer
/// (the marshalling layer's pump over a JS async-iterable) sends `Bytes` frames;
/// dropping the sender ends the body. Mirrors forge/http's `channel_body`.
pub fn channel_request_body() -> (mpsc::Sender<Bytes>, reqwest::Body) {
  let (tx, rx) = mpsc::channel::<Bytes>(16);
  (tx, reqwest::Body::wrap_stream(ChunkStream { rx }))
}

fn headers_to_pairs(headers: &reqwest::header::HeaderMap) -> Vec<(String, String)> {
  headers
    .iter()
    .filter_map(|(name, value)| value.to_str().ok().map(|v| (name.as_str().to_string(), v.to_string())))
    .collect()
}

/// Send an HTTP request and read the response into a `ResponseData`. The `body`
/// may be buffered (`reqwest::Body::from(bytes)`) or streamed
/// (`channel_request_body`); `None` sends no body.
pub async fn do_fetch(
  client: Rc<reqwest::Client>,
  method: &str,
  url: &str,
  headers: Vec<(String, String)>,
  body: Option<reqwest::Body>,
) -> Result<ResponseData, String> {
  let mut req = match method {
    "GET" => client.get(url),
    "POST" => client.post(url),
    "PUT" => client.put(url),
    "DELETE" => client.delete(url),
    "PATCH" => client.patch(url),
    "HEAD" => client.head(url),
    _ => client.request(method.parse().map_err(|_| format!("invalid HTTP method: {method}"))?, url),
  };

  for (key, val) in &headers {
    req = req.header(key.as_str(), val.as_str());
  }

  if let Some(body) = body {
    req = req.body(body);
  }

  let resp = req.send().await.map_err(|e| e.to_string())?;
  let status = resp.status();
  let resp_url = resp.url().to_string();
  let resp_headers = headers_to_pairs(resp.headers());
  // Streamed by default: the body is read lazily as JS consumes it (text/bytes/
  // json drain it; response.body iterates it), rather than buffered up front.
  let body: ByteStream = to_byte_stream(resp.bytes_stream());

  Ok(ResponseData {
    status: status.as_u16(),
    // The canonical reason phrase for the code. reqwest/http does not retain the
    // wire reason phrase, so this is derived from the status (covers all standard
    // codes; empty for non-standard ones).
    status_text: status.canonical_reason().unwrap_or("").to_string(),
    url: resp_url,
    headers: resp_headers,
    body,
  })
}

/// Disk-cache policy for `do_fetch_cached`. The uncached default is the
/// caller using plain `do_fetch` instead.
#[derive(Clone, Copy, PartialEq)]
pub enum CacheMode {
  /// Serve from disk if present, otherwise fetch and store. No freshness
  /// model: the entry lives until evicted.
  ForceCache,
  /// Fetch fresh and overwrite the stored entry.
  Reload,
}

/// The response snapshot stored as an entry's metadata blob (the body follows
/// it in the entry file). `url` is the resolved url, which can differ from
/// the request url the entry is keyed by (redirects).
#[derive(Serialize, Deserialize)]
struct CacheMeta {
  status: u16,
  status_text: String,
  url: String,
  headers: Vec<(String, String)>,
}

/// `do_fetch` with an explicit disk-cache policy. Only GET requests with 2xx
/// responses are cached, keyed by the request url; anything else degrades to
/// a plain `do_fetch`. Stored bodies write through as the consumer drains the
/// response and commit only on clean completion (see `Cache::store`).
pub async fn do_fetch_cached(
  client: Rc<reqwest::Client>,
  method: &str,
  url: &str,
  headers: Vec<(String, String)>,
  body: Option<reqwest::Body>,
  cache: Rc<Cache>,
  mode: CacheMode,
) -> Result<ResponseData, String> {
  if method != "GET" {
    return do_fetch(client, method, url, headers, body).await;
  }
  if mode == CacheMode::ForceCache {
    if let Some((meta, cached_body)) = cache.lookup(url).await {
      // A meta blob that does not parse is a corrupt or foreign entry:
      // fall through to the network, which overwrites it.
      if let Ok(m) = serde_json::from_slice::<CacheMeta>(&meta) {
        return Ok(ResponseData {
          status: m.status,
          status_text: m.status_text,
          url: m.url,
          headers: m.headers,
          body: cached_body,
        });
      }
    }
  }
  let resp = do_fetch(client, method, url, headers, body).await?;
  if !(200..300).contains(&resp.status) {
    return Ok(resp);
  }
  let ResponseData { status, status_text, url: resolved_url, headers: resp_headers, body: resp_body } = resp;
  let meta = CacheMeta {
    status,
    status_text: status_text.clone(),
    url: resolved_url.clone(),
    headers: resp_headers.clone(),
  };
  let meta = serde_json::to_vec(&meta).map_err(|e| e.to_string())?;
  let body = cache.store(url, meta, resp_body);
  Ok(ResponseData { status, status_text, url: resolved_url, headers: resp_headers, body })
}
