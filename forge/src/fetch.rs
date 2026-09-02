//! Engine-free `fetch` core.
//!
//! Sends an HTTP request with reqwest and reads the response into a plain
//! `ResponseData`. Names no scripting-engine types, and no reqwest types cross
//! its boundary either: the client is `Client`, an outgoing body is
//! `RequestBody`. The marshalling layer (flux `standards_plugins/fetch.rs`)
//! decodes the JS request, builds the request body, and turns `ResponseData`
//! into a JS `Response`. Streamed request bodies are fed through
//! `channel_request_body`, whose sender the marshalling layer drives from a
//! JS async-iterable.

use bytes::Bytes;
use futures_core::Stream;
use serde::{Deserialize, Serialize};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::io;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio::time::Instant;

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

/// The HTTP client `fetch` sends through: one per engine, carrying the host's
/// `User-Agent`. Cheap to clone (the reqwest client is a shared handle), so
/// callers clone it into each request future.
#[derive(Clone)]
pub struct Client {
  inner: reqwest::Client,
}

impl Client {
  pub fn new(user_agent: &str) -> Result<Client, String> {
    reqwest::Client::builder().user_agent(user_agent).build().map(|inner| Client { inner }).map_err(|e| e.to_string())
  }
}

/// An outgoing request body: buffered bytes (`bytes`), or the streamed side
/// of `channel_request_body`.
pub struct RequestBody(reqwest::Body);

impl RequestBody {
  pub fn bytes(bytes: Vec<u8>) -> RequestBody {
    RequestBody(reqwest::Body::from(bytes))
  }
}

/// Bridges the mpsc receiver fed by the marshalling layer into a `futures`
/// stream so reqwest can send it as a streamed (chunked) request body. The
/// mirror of forge/http's `ChannelBody`, but for `futures::Stream` rather than
/// `hyper::Body`.
struct ChunkStream {
  rx: mpsc::Receiver<Vec<u8>>,
}

impl Stream for ChunkStream {
  type Item = Result<Bytes, io::Error>;

  fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
    self.rx.poll_recv(cx).map(|chunk| chunk.map(|c| Ok(Bytes::from(c))))
  }
}

/// Build a streamed request body and the sender that feeds it. The producer
/// (the marshalling layer's pump over a JS async-iterable) sends chunks;
/// dropping the sender ends the body. Mirrors forge/http's `channel_body`.
pub fn channel_request_body() -> (mpsc::Sender<Vec<u8>>, RequestBody) {
  let (tx, rx) = mpsc::channel::<Vec<u8>>(16);
  (tx, RequestBody(reqwest::Body::wrap_stream(ChunkStream { rx })))
}

fn headers_to_pairs(headers: &reqwest::header::HeaderMap) -> Vec<(String, String)> {
  headers
    .iter()
    .filter_map(|(name, value)| value.to_str().ok().map(|v| (name.as_str().to_string(), v.to_string())))
    .collect()
}

/// Send an HTTP request and read the response into a `ResponseData`. The `body`
/// may be buffered (`RequestBody::bytes`) or streamed
/// (`channel_request_body`); `None` sends no body.
pub async fn do_fetch(
  client: &Client,
  method: &str,
  url: &str,
  headers: Vec<(String, String)>,
  body: Option<RequestBody>,
) -> Result<ResponseData, String> {
  let client = &client.inner;
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
    req = req.body(body.0);
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

/// Per-host concurrency cap for cached (asset-mode) fetches. Uncoordinated
/// fetch floods (one per mounted image) slow every transfer and get clients
/// rate-limited; disk hits bypass the limit, and plain `do_fetch` traffic is
/// deliberately not throttled (API calls, long-polls, and streams must never
/// queue behind a politeness knob).
///
/// A permit is an async RAII value: waiters are pending futures (nothing
/// blocks), and the permit rides the response body stream so it releases when
/// the body completes or its consumer drops it.
///
/// Concurrency alone does not bound the request *rate* (six fast transfers
/// recycling is still tens of requests per second), so a host also carries a
/// cooldown: a 429 pauses the whole host (see `cooldown`), and `acquire`
/// waits it out before letting a request start.
pub struct HostLimits {
  max_per_host: usize,
  hosts: RefCell<HashMap<String, Rc<HostState>>>,
}

struct HostState {
  semaphore: Arc<Semaphore>,
  cooldown_until: Cell<Option<Instant>>,
}

impl HostLimits {
  pub fn new(max_per_host: usize) -> Self {
    Self { max_per_host, hosts: RefCell::new(HashMap::new()) }
  }

  fn state(&self, host: &str) -> Rc<HostState> {
    self
      .hosts
      .borrow_mut()
      .entry(host.to_string())
      .or_insert_with(|| {
        Rc::new(HostState { semaphore: Arc::new(Semaphore::new(self.max_per_host)), cooldown_until: Cell::new(None) })
      })
      .clone()
  }

  /// Wait for a slot on `host`, then for any active cooldown. The returned
  /// permit frees the slot on drop. The permit is held through the cooldown
  /// sleep on purpose: a 429'd host drains to idle and stays paused, instead
  /// of freed slots re-flooding it the moment their transfers finish.
  pub async fn acquire(&self, host: &str) -> OwnedSemaphorePermit {
    let state = self.state(host);
    let permit = state.semaphore.clone().acquire_owned().await.expect("host semaphore never closes");
    loop {
      match state.cooldown_until.get() {
        Some(until) if until > Instant::now() => tokio::time::sleep_until(until).await,
        _ => return permit,
      }
    }
  }

  /// Pause new requests to `host` for `delay`. Extends, never shortens, an
  /// active cooldown: concurrent 429s each report a delay and the furthest
  /// one wins.
  pub fn cooldown(&self, host: &str, delay: Duration) {
    let state = self.state(host);
    let until = Instant::now() + delay;
    if state.cooldown_until.get().is_none_or(|current| until > current) {
      state.cooldown_until.set(Some(until));
    }
  }
}

/// The per-host key: host plus effective port, so `http://x` and `https://x`
/// count as the same endpoint family only when their ports match.
fn host_key(url: &str) -> Option<String> {
  let parsed = reqwest::Url::parse(url).ok()?;
  let host = parsed.host_str()?;
  match parsed.port_or_known_default() {
    Some(port) => Some(format!("{host}:{port}")),
    None => Some(host.to_string()),
  }
}

/// Carries the host permit for the lifetime of the response body.
struct LimitedStream {
  inner: ByteStream,
  _permit: OwnedSemaphorePermit,
}

impl Stream for LimitedStream {
  type Item = Result<Bytes, io::Error>;

  fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
    self.get_mut().inner.as_mut().poll_next(cx)
  }
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

/// What browsing tooling reads from a fetch-cache entry's meta blob: the
/// (resolved) url and the response content type (lowercased, parameters like
/// `; charset=...` stripped).
pub struct CachedMeta {
  pub url: String,
  pub content_type: Option<String>,
}

/// Decode a scanned fetch-cache entry's meta blob (see `cache::scan`). None
/// for a blob some other consumer wrote.
pub fn cached_meta(meta: &[u8]) -> Option<CachedMeta> {
  let m = serde_json::from_slice::<CacheMeta>(meta).ok()?;
  let content_type = m
    .headers
    .iter()
    .find(|(name, _)| name.eq_ignore_ascii_case("content-type"))
    .map(|(_, value)| value.split(';').next().unwrap_or("").trim().to_ascii_lowercase())
    .filter(|v| !v.is_empty());
  Some(CachedMeta { url: m.url, content_type })
}

/// Bounded retries after a 429'd request (4 attempts total).
const RETRY_LIMIT: u32 = 3;

/// First-attempt backoff ceiling when the 429 carries no Retry-After; doubles
/// per attempt (full jitter: the delay is uniform in [0, ceiling]).
const BACKOFF_BASE: Duration = Duration::from_millis(500);

/// A Retry-After beyond this means "come back much later", not "brief pause":
/// give up and hand the 429 to the caller instead of sitting on a long sleep.
const RETRY_AFTER_MAX: Duration = Duration::from_secs(60);

/// The Retry-After header as a delay. Only the delta-seconds form is parsed;
/// the HTTP-date form degrades to the jittered backoff like an absent header.
fn retry_after(headers: &[(String, String)]) -> Option<Duration> {
  let (_, value) = headers.iter().find(|(name, _)| name.eq_ignore_ascii_case("retry-after"))?;
  value.trim().parse::<u64>().ok().map(Duration::from_secs)
}

/// Full-jitter exponential backoff: uniform in [0, base * 2^attempt], so
/// parallel lanes that 429'd together do not retry in lockstep.
fn jittered_backoff(attempt: u32) -> Duration {
  Duration::from_millis(fastrand::u64(0..=(BACKOFF_BASE.as_millis() as u64) << attempt))
}

/// `do_fetch` with an explicit disk-cache policy. Only GET requests with 2xx
/// responses are cached, keyed by the request url; anything else degrades to
/// a plain `do_fetch`. Stored bodies write through as the consumer drains the
/// response and commit only on clean completion (see `Cache::store`). Cache
/// misses queue on the per-host limit; disk hits bypass it.
///
/// A 429 response backs off reactively: the host goes on cooldown (Retry-After
/// when sent, jittered exponential otherwise) and the request retries up to
/// `RETRY_LIMIT` times. Plain `do_fetch` traffic deliberately has none of
/// this; a caller that wants backoff on API calls implements its own policy.
pub async fn do_fetch_cached(
  client: &Client,
  method: &str,
  url: &str,
  headers: Vec<(String, String)>,
  body: Option<RequestBody>,
  cache: Rc<Cache>,
  mode: CacheMode,
  limits: Rc<HostLimits>,
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
  let host = host_key(url);
  let permit = match &host {
    Some(host) => Some(limits.acquire(host).await),
    // An unparsable url: skip the limit and let do_fetch produce the error.
    None => None,
  };
  // A request body cannot be replayed (it may be a stream), so only body-less
  // requests retry; a GET with a body is a fringe case.
  let can_retry = body.is_none();
  let mut attempt: u32 = 0;
  let mut resp = do_fetch(client, method, url, headers.clone(), body).await?;
  while can_retry && resp.status == 429 && attempt < RETRY_LIMIT {
    let delay = match retry_after(&resp.headers) {
      Some(after) if after > RETRY_AFTER_MAX => break,
      Some(after) => after,
      None => jittered_backoff(attempt),
    };
    if let Some(host) = &host {
      limits.cooldown(host, delay);
    }
    // Sleep it out while holding the permit (see `acquire`): the other lanes
    // queue behind the cooldown instead of taking over the freed slot.
    tokio::time::sleep(delay).await;
    attempt += 1;
    resp = do_fetch(client, method, url, headers.clone(), None).await?;
  }
  let ResponseData { status, status_text, url: resolved_url, headers: resp_headers, body: resp_body } = resp;
  let resp_body = match permit {
    Some(permit) => Box::pin(LimitedStream { inner: resp_body, _permit: permit }) as ByteStream,
    None => resp_body,
  };
  if !(200..300).contains(&status) {
    return Ok(ResponseData { status, status_text, url: resolved_url, headers: resp_headers, body: resp_body });
  }
  let meta =
    CacheMeta { status, status_text: status_text.clone(), url: resolved_url.clone(), headers: resp_headers.clone() };
  let meta = serde_json::to_vec(&meta).map_err(|e| e.to_string())?;
  let body = cache.store(url, meta, resp_body);
  Ok(ResponseData { status, status_text, url: resolved_url, headers: resp_headers, body })
}
