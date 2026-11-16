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
use std::collections::HashMap;
use std::io;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
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
  /// The response came over HTTP/2 (false for a disk hit). `HostLimits`
  /// records it per host.
  pub http2: bool,
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

  /// A client on a TLS config of `forge::tls` (extra roots, a client
  /// certificate, a pin) instead of reqwest's own. reqwest sends a
  /// preconfigured config as it is, so the ALPN list `new` would offer is
  /// set here: HTTP/2 first, then HTTP/1.1.
  pub fn with_tls(user_agent: &str, config: &crate::tls::ClientConfig) -> Result<Client, String> {
    let mut config = config.clone();
    config.alpn_protocols = FETCH_ALPN.iter().map(|p| p.to_vec()).collect();
    reqwest::Client::builder()
      .user_agent(user_agent)
      .use_preconfigured_tls(config)
      .build()
      .map(|inner| Client { inner })
      .map_err(|e| e.to_string())
  }
}

/// The protocols a fetch offers by ALPN, as reqwest's own TLS config does.
const FETCH_ALPN: [&[u8]; 2] = [b"h2", b"http/1.1"];

/// A failed request as one message: reqwest's own ("error sending request
/// for url (..)") says nothing of the cause, so the sources follow it, each
/// once (`tls::error_chain_message`, which renders a TLS error of our
/// verifiers).
fn error_message(e: &reqwest::Error) -> String {
  crate::tls::error_chain_message(e)
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

/// Send one HTTP request and read the response into a `ResponseData`, with
/// no per-host limit and no retry: the dev-server proxy, which funnels every
/// host into one, sends through this. Everything else sends through
/// `do_fetch`. The `body` may be buffered (`RequestBody::bytes`) or streamed
/// (`channel_request_body`); `None` sends no body.
pub async fn send(
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

  let resp = req.send().await.map_err(|e| error_message(&e))?;
  let status = resp.status();
  let resp_url = resp.url().to_string();
  let resp_headers = headers_to_pairs(resp.headers());
  let http2 = resp.version() == reqwest::Version::HTTP_2;
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
    http2,
  })
}

/// Send an HTTP request through the per-host limits: take a slot on the host
/// (see `HostLimits::acquire`), send, and record the response's HTTP version
/// for the host. A 429 puts the host on cooldown (Retry-After when sent,
/// jittered exponential otherwise); a body-less `GET` or `HEAD` then retries
/// up to `RETRY_LIMIT` times, and any other request hands the 429 to the
/// caller (a body cannot be replayed, and repeating another method is the
/// caller's decision). The slot rides the response body and frees when the
/// body ends or its consumer drops it. With `slot` false the request takes
/// no slot but still waits out the host's cooldown: the cooldown spares the
/// server, the slot spares the app's other requests.
pub async fn do_fetch(
  client: &Client,
  limits: &HostLimits,
  slot: bool,
  method: &str,
  url: &str,
  headers: Vec<(String, String)>,
  body: Option<RequestBody>,
) -> Result<ResponseData, String> {
  let host = host_key(url);
  let permit = match &host {
    Some(host) if slot => limits.acquire(host).await,
    Some(host) => {
      limits.cooled(host).await;
      None
    }
    // An unparsable url: skip the limit and let send produce the error.
    None => None,
  };
  let can_retry = body.is_none() && (method == "GET" || method == "HEAD");
  let mut attempt: u32 = 0;
  let mut resp = send(client, method, url, headers.clone(), body).await?;
  loop {
    if let Some(host) = &host {
      limits.record(host, resp.http2);
    }
    if resp.status != 429 {
      break;
    }
    let delay = match retry_after(&resp.headers) {
      // "Come back much later": no host-wide pause, the caller gets the 429.
      Some(after) if after > RETRY_AFTER_MAX => break,
      Some(after) => after,
      None => jittered_backoff(attempt),
    };
    if let Some(host) = &host {
      limits.cooldown(host, delay);
    }
    if !can_retry || attempt == RETRY_LIMIT {
      break;
    }
    // Sleep it out while holding the slot, when this request has one (see
    // `acquire`): the other requests queue behind the cooldown instead of
    // taking over the freed slot.
    tokio::time::sleep(delay).await;
    attempt += 1;
    resp = send(client, method, url, headers.clone(), None).await?;
  }
  if let Some(permit) = permit {
    resp.body = with_permit(resp.body, permit);
  }
  Ok(resp)
}

/// Per-host concurrency cap for every `do_fetch`, the browser model for
/// HTTP/1.1: uncoordinated fetch floods (one per mounted image) slow every
/// transfer, open a connection each and get clients rate-limited. Disk hits
/// bypass the limit. A host whose last response came over HTTP/2 skips the
/// cap: its requests share one connection, and the server's stream limit is
/// the cap. A host is capped until its first response says otherwise.
///
/// A slot is an open request to a host, whatever opened it: a media source
/// (`source::HttpSource`) takes one as a fetch does.
///
/// A permit is an async RAII value: waiters are pending futures (nothing
/// blocks), and the permit rides the response body stream so it releases when
/// the body completes or its consumer drops it.
///
/// Concurrency alone does not bound the request *rate* (six fast transfers
/// recycling is still tens of requests per second), so a host also carries a
/// cooldown: a 429 pauses the whole host (see `cooldown`), and `acquire`
/// waits it out before letting a request start.
///
/// The cap is the app's to change (`set_limit`): a default for every host,
/// and an override per host beside it.
///
/// One per app, shared by every engine (the root and its isolates) and every
/// client, so a server sees one app, not one per thread or per client. It is
/// `Send + Sync`: its locks guard plain state and are never held across an
/// await.
pub struct HostLimits {
  hosts: Mutex<Hosts>,
}

/// The default cap and the hosts seen so far. A host's state is made on
/// first sight with the default of that moment.
struct Hosts {
  /// The cap of a host with no override; `None` is unlimited.
  default: Option<usize>,
  map: HashMap<String, Arc<HostState>>,
}

struct HostState {
  slots: Mutex<Slots>,
  cooldown_until: Mutex<Option<Instant>>,
  /// The host's last response came over HTTP/2.
  multiplexed: AtomicBool,
}

/// A host's cap.
struct Slots {
  /// Set by `set_limit` for this host: a new default leaves the host alone.
  overridden: bool,
  /// The slots of a finite cap; `None` while the host is unlimited.
  pool: Option<Arc<Pool>>,
}

/// The slots of a finite cap. A raised cap adds permits at once. A lowered
/// one forgets what is free at once (`forget_permits`) and owes the rest: a
/// permit that comes back to a pool that owes is forgotten by the acquire
/// that takes it, which then waits on. So the slots in use never exceed the
/// new cap once the requests open before the change have ended.
struct Pool {
  semaphore: Arc<Semaphore>,
  ledger: Mutex<Ledger>,
}

struct Ledger {
  cap: usize,
  /// Permits still to forget as they come back.
  owed: usize,
}

impl Pool {
  fn new(cap: usize) -> Arc<Pool> {
    Arc::new(Pool { semaphore: Arc::new(Semaphore::new(cap)), ledger: Mutex::new(Ledger { cap, owed: 0 }) })
  }

  fn ledger(&self) -> std::sync::MutexGuard<'_, Ledger> {
    self.ledger.lock().expect("host slots ledger poisoned")
  }

  fn resize(&self, cap: usize) {
    let mut ledger = self.ledger();
    if cap > ledger.cap {
      let more = cap - ledger.cap;
      let repaid = more.min(ledger.owed);
      ledger.owed -= repaid;
      self.semaphore.add_permits(more - repaid);
    } else {
      let fewer = ledger.cap - cap;
      let forgotten = self.semaphore.forget_permits(fewer);
      ledger.owed += fewer - forgotten;
    }
    ledger.cap = cap;
  }

  /// A permit, once one is free and nothing is owed. `None` when the pool
  /// closed while this waited: its host went unlimited.
  async fn acquire(&self) -> Option<OwnedSemaphorePermit> {
    loop {
      let permit = self.semaphore.clone().acquire_owned().await.ok()?;
      let owes = {
        let mut ledger = self.ledger();
        let owes = ledger.owed > 0;
        if owes {
          ledger.owed -= 1;
        }
        owes
      };
      if !owes {
        return Some(permit);
      }
      permit.forget();
    }
  }
}

impl Slots {
  fn new(limit: Option<usize>) -> Slots {
    Slots { overridden: false, pool: limit.map(Pool::new) }
  }

  fn limit(&self) -> Option<usize> {
    self.pool.as_ref().map(|pool| pool.ledger().cap)
  }

  /// Change the cap. Unlimited closes the pool, which frees its waiters; a
  /// finite cap after unlimited starts a fresh pool, which does not count
  /// the requests already open.
  fn set(&mut self, limit: Option<usize>) {
    self.pool = match (self.pool.take(), limit) {
      (Some(pool), Some(cap)) => {
        pool.resize(cap);
        Some(pool)
      }
      (Some(pool), None) => {
        pool.semaphore.close();
        None
      }
      (None, limit) => limit.map(Pool::new),
    };
  }
}

impl HostState {
  fn new(limit: Option<usize>) -> HostState {
    HostState {
      slots: Mutex::new(Slots::new(limit)),
      cooldown_until: Mutex::new(None),
      multiplexed: AtomicBool::new(false),
    }
  }

  fn slots(&self) -> std::sync::MutexGuard<'_, Slots> {
    self.slots.lock().expect("host slots lock poisoned")
  }

  fn cooldown_until(&self) -> Option<Instant> {
    *self.cooldown_until.lock().expect("host cooldown lock poisoned")
  }

  /// Wait out an active cooldown, however often it is extended meanwhile.
  async fn cooled(&self) {
    loop {
      match self.cooldown_until() {
        Some(until) if until > Instant::now() => tokio::time::sleep_until(until).await,
        _ => return,
      }
    }
  }
}

/// The largest finite per-host cap: the semaphore's own maximum. A caller
/// that takes caps from outside checks against it (`set_limit` clamps).
pub const MAX_HOST_LIMIT: usize = Semaphore::MAX_PERMITS;

/// A cap as the semaphore can hold it: at least one slot (a cap of 0 would
/// stall the host for good), at most `MAX_HOST_LIMIT`.
fn valid_cap(limit: Option<usize>) -> Option<usize> {
  limit.map(|cap| cap.clamp(1, MAX_HOST_LIMIT))
}

impl HostLimits {
  pub fn new(max_per_host: usize) -> Self {
    Self { hosts: Mutex::new(Hosts { default: valid_cap(Some(max_per_host)), map: HashMap::new() }) }
  }

  fn hosts(&self) -> std::sync::MutexGuard<'_, Hosts> {
    self.hosts.lock().expect("host limits lock poisoned")
  }

  /// The state of `host`, cloned out of the map: the map lock is released
  /// before the caller awaits anything.
  fn state(&self, host: &str) -> Arc<HostState> {
    let mut hosts = self.hosts();
    let default = hosts.default;
    hosts.map.entry(host.to_string()).or_insert_with(|| Arc::new(HostState::new(default))).clone()
  }

  /// The cap of `host` (a `host_key`), or the default without one. `None`
  /// is unlimited. A host not seen yet reads the default.
  pub fn limit(&self, host: Option<&str>) -> Option<usize> {
    let hosts = self.hosts();
    match host.and_then(|host| hosts.map.get(host)) {
      Some(state) => state.slots().limit(),
      None => hosts.default,
    }
  }

  /// Set the cap of `host` (a `host_key`), an override that a later default
  /// leaves alone; or, without a host, the default: the cap of every host
  /// with no override, seen so far or later. `None` is unlimited: `acquire`
  /// takes no slot on such a host, as on an HTTP/2 one, and its waiters go
  /// at once. A raised cap frees waiters at once. A lowered cap takes effect
  /// as slots free up: requests already open keep theirs, and new ones wait
  /// until the open ones are within the new cap. A finite cap on a host that
  /// was unlimited starts afresh: the requests open since do not count. A
  /// cap of 0 is taken as 1.
  pub fn set_limit(&self, host: Option<&str>, limit: Option<usize>) {
    let limit = valid_cap(limit);
    let mut hosts = self.hosts();
    match host {
      Some(host) => {
        let default = hosts.default;
        let state = hosts.map.entry(host.to_string()).or_insert_with(|| Arc::new(HostState::new(default)));
        let mut slots = state.slots();
        slots.overridden = true;
        slots.set(limit);
      }
      None => {
        hosts.default = limit;
        for state in hosts.map.values() {
          let mut slots = state.slots();
          if !slots.overridden {
            slots.set(limit);
          }
        }
      }
    }
  }

  /// Wait for a slot on `host`, then for any active cooldown. The returned
  /// permit frees the slot on drop; an HTTP/2 or unlimited host takes no
  /// slot (`None`) but still waits out the cooldown. The permit is held
  /// through the cooldown sleep on purpose: a 429'd host drains to idle and
  /// stays paused, instead of freed slots re-flooding it the moment their
  /// transfers finish.
  pub async fn acquire(&self, host: &str) -> Option<OwnedSemaphorePermit> {
    let state = self.state(host);
    let pool = match state.multiplexed.load(Ordering::Relaxed) {
      true => None,
      false => state.slots().pool.clone(),
    };
    let permit = match pool {
      Some(pool) => pool.acquire().await,
      None => None,
    };
    state.cooled().await;
    permit
  }

  /// Wait out any active cooldown on `host` without taking a slot: a request
  /// that opted out of the cap still spares a server that asked for a pause.
  pub async fn cooled(&self, host: &str) {
    self.state(host).cooled().await;
  }

  /// Record the HTTP version of a response from `host`. The last response
  /// wins: a host that moves back to HTTP/1.1 is capped from its next
  /// request on. A burst that starts before the first response records
  /// HTTP/2 is capped once more, which is harmless.
  pub fn record(&self, host: &str, http2: bool) {
    self.state(host).multiplexed.store(http2, Ordering::Relaxed);
  }

  /// Pause new requests to `host` for `delay`. Extends, never shortens, an
  /// active cooldown: concurrent 429s each report a delay and the furthest
  /// one wins.
  pub fn cooldown(&self, host: &str, delay: Duration) {
    let state = self.state(host);
    let until = Instant::now() + delay;
    let mut current = state.cooldown_until.lock().expect("host cooldown lock poisoned");
    if current.is_none_or(|current| until > current) {
      *current = Some(until);
    }
  }
}

/// The per-host key: host plus effective port, so `http://x` and `https://x`
/// count as the same endpoint family only when their ports match. `None`
/// for a value that is no URL with a host.
pub fn host_key(url: &str) -> Option<String> {
  let parsed = reqwest::Url::parse(url).ok()?;
  let host = parsed.host_str()?;
  match parsed.port_or_known_default() {
    Some(port) => Some(format!("{host}:{port}")),
    None => Some(host.to_string()),
  }
}

/// The per-host key an app names: a URL (its `host_key`), or a
/// `host:port` key itself, its host normalized as a URL's is (lowercased).
/// A key names its port, since a bare host could be either scheme's.
/// `None` for anything else.
pub fn parse_host_key(value: &str) -> Option<String> {
  if value.contains("://") {
    return host_key(value);
  }
  let (_, port) = value.rsplit_once(':')?;
  let port: u16 = port.parse().ok()?;
  let parsed = reqwest::Url::parse(&format!("http://{value}")).ok()?;
  // Nothing beside the host and port: "user@host:80" or "host:80/x" is no key.
  let bare = parsed.username().is_empty()
    && parsed.password().is_none()
    && parsed.path() == "/"
    && parsed.query().is_none()
    && parsed.fragment().is_none();
  match bare {
    true => Some(format!("{}:{port}", parsed.host_str()?)),
    false => None,
  }
}

/// `body` holding `permit` for as long as it lives: the slot frees when the
/// body ends or its consumer drops it.
pub fn with_permit(body: ByteStream, permit: OwnedSemaphorePermit) -> ByteStream {
  Box::pin(LimitedStream { inner: body, _permit: permit })
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
/// response and commit only on clean completion (see `Cache::store`). A disk
/// hit takes no slot on the per-host limit; a miss goes through `do_fetch`
/// with its limit (taking a slot unless `slot` is false) and 429 retry.
#[allow(clippy::too_many_arguments)]
pub async fn do_fetch_cached(
  client: &Client,
  limits: &HostLimits,
  slot: bool,
  method: &str,
  url: &str,
  headers: Vec<(String, String)>,
  body: Option<RequestBody>,
  cache: Rc<Cache>,
  mode: CacheMode,
) -> Result<ResponseData, String> {
  if method != "GET" {
    return do_fetch(client, limits, slot, method, url, headers, body).await;
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
          http2: false,
        });
      }
    }
  }
  let resp = do_fetch(client, limits, slot, method, url, headers, body).await?;
  if !(200..300).contains(&resp.status) {
    return Ok(resp);
  }
  let ResponseData { status, status_text, url: resolved_url, headers: resp_headers, body: resp_body, http2 } = resp;
  let meta =
    CacheMeta { status, status_text: status_text.clone(), url: resolved_url.clone(), headers: resp_headers.clone() };
  let meta = serde_json::to_vec(&meta).map_err(|e| e.to_string())?;
  let body = cache.store(url, meta, resp_body);
  Ok(ResponseData { status, status_text, url: resolved_url, headers: resp_headers, body, http2 })
}
