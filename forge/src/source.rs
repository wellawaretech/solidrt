//! A streamed byte source: a producer (HTTP today) pumped on the forge I/O
//! runtime into a bounded ring that a synchronous consumer reads and seeks
//! from its own thread.
//!
//! The [`Reader`] is `Read + Seek + Send`, so it is a
//! [`SeekableReader`](crate::seek::SeekableReader) any existing consumer takes
//! (the WebM demuxer, an audio decoder). Seeks are local bookkeeping where
//! they can be: a target inside the ring, or a short forward skip, is
//! discarded locally; anything else restarts the producer at the target under
//! a new generation, and the producer re-checks the generation under the lock
//! before every push, so no chunk from before the restart lands after it. A
//! source that cannot seek (an HTTP 200, a chunked feed) refuses a restart
//! with `Unsupported`; a forward skip on it is always read through.
//!
//! Producers run on one lazily started I/O runtime: a current_thread runtime
//! on its own thread, alive for the process. Not the host's runtime: the flux
//! binaries run current_thread on the JS thread, and a producer awaiting a
//! socket must not share it. Because the runtime is never dropped, a
//! cancelled name lookup (getaddrinfo on tokio's blocking pool) cannot hang a
//! close.
//!
//! Reconnects belong to the pump: on a seekable source any transport error,
//! early end or stall reopens the producer at the offset reached, under a
//! backoff and an outage budget; the producer's job is to refuse a reopened
//! source that is not the one it first saw. An unseekable source never
//! reconnects, since a new request restarts it on a new timeline. A clean end
//! and an error stay distinct: a truncated body is an error, never the end.
//!
//! This source bypasses the engine's `fetch`: no disk cache, no dev-server
//! proxying. The device reaches the host directly.

use bytes::{Buf, Bytes};
use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::io::{self, ErrorKind, Read, Seek, SeekFrom};
use std::pin::Pin;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::{Duration, Instant};
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

use crate::stream::{to_byte_stream, ByteStream};

/// Bytes the ring holds ahead of the consumer before the producer waits. A
/// transfer buffer, not the playback buffer: read-ahead is the consumer's.
pub const STREAM_RING_BYTES: usize = 1024 * 1024;
/// A forward seek on a seekable source that lands within this many bytes past
/// what is already in flight is read through and discarded rather than
/// restarting the producer: a restart costs a round trip and a connection.
pub const STREAM_SKIP_BY_READ_BYTES: u64 = 512 * 1024;
/// A seekable source delivering no bytes for this long is reconnected.
const STREAM_STALL_MS: u64 = 8_000;
/// First reconnect delay; doubles per failed attempt up to the cap.
const RECONNECT_BACKOFF_MS: u64 = 250;
const RECONNECT_BACKOFF_MAX_MS: u64 = 4_000;
/// How long reconnects may keep failing, with no byte delivered in between,
/// before the source errors.
const RECONNECT_OUTAGE_MS: u64 = 30_000;

/// What the source knows about itself once its first open has answered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Facts {
  /// The total length in bytes, when the source announces one.
  pub len: Option<u64>,
  /// Whether the producer can be reopened at an offset. False means every
  /// open starts a new timeline at 0: no restart, no reconnect.
  pub seekable: bool,
}

/// A producer's answer to an open: the body from the requested offset on,
/// with what the response said about length and seekability.
pub struct Opened {
  pub body: ByteStream,
  pub len: Option<u64>,
  pub seekable: bool,
}

pub type OpenFuture<'a> = Pin<Box<dyn Future<Output = io::Result<Opened>> + Send + 'a>>;

/// Something that can be opened at a byte offset. Run on the I/O runtime.
pub trait Producer: Send + 'static {
  /// Open the source at `offset`: first at 0, then at every restart (a seek,
  /// a reconnect). A producer that has opened once must fail a later open
  /// whose source is not the one it first saw, rather than deliver the new
  /// bytes as if they continued the old; that failure is `InvalidData`, the
  /// one error kind the pump never retries. Any other error on a seekable
  /// source is retried under the reconnect policy.
  fn open(&mut self, offset: u64) -> OpenFuture<'_>;
}

// --- The shared ring ---

enum End {
  Clean,
  Error(String),
}

/// Unread bytes from `head` on, in producer order. `next` is where the
/// producer's next chunk lands (`head + buffered`); the consumer's own
/// position may run ahead of `head` (a skip by read), in which case reads
/// discard up to it as bytes arrive.
struct Ring {
  generation: u64,
  chunks: VecDeque<Bytes>,
  head: u64,
  buffered: usize,
  next: u64,
  end: Option<End>,
  /// A restart the pump has not picked up yet.
  restart: Option<u64>,
  facts: Option<Facts>,
  interrupt: bool,
  closed: bool,
}

impl Ring {
  /// Drop buffered bytes before `pos`.
  fn discard_before(&mut self, pos: u64) {
    while self.head < pos {
      let Some(front) = self.chunks.front_mut() else { break };
      let skip = ((pos - self.head) as usize).min(front.len());
      front.advance(skip);
      self.head += skip as u64;
      self.buffered -= skip;
      if front.is_empty() {
        self.chunks.pop_front();
      }
    }
  }

  /// Copy buffered bytes out from `head`.
  fn take(&mut self, buf: &mut [u8]) -> usize {
    let mut n = 0;
    while n < buf.len() {
      let Some(front) = self.chunks.front_mut() else { break };
      let len = front.len().min(buf.len() - n);
      buf[n..n + len].copy_from_slice(&front[..len]);
      front.advance(len);
      n += len;
      if front.is_empty() {
        self.chunks.pop_front();
      }
    }
    self.head += n as u64;
    self.buffered -= n;
    n
  }

  /// Begin a new generation at `offset`, empty. With `restart` the pump
  /// reopens the producer there; without it the pump goes idle (the target
  /// is at or past the end, so there is nothing to fetch).
  fn new_generation(&mut self, offset: u64, restart: bool) {
    self.generation += 1;
    self.chunks.clear();
    self.buffered = 0;
    self.head = offset;
    self.next = offset;
    self.end = if restart { None } else { Some(End::Clean) };
    self.restart = restart.then_some(offset);
  }
}

struct Shared {
  ring: Mutex<Ring>,
  /// Consumer side: bytes, an end, facts, an interrupt or a close arrived.
  data: Condvar,
  /// Producer side: room in the ring, a generation change or a close.
  space: Notify,
  /// Pump side: a restart was requested (or a generation ended without one).
  restart: Notify,
  cancel: CancellationToken,
  ring_bytes: usize,
}

impl Shared {
  fn lock(&self) -> MutexGuard<'_, Ring> {
    self.ring.lock().unwrap_or_else(PoisonError::into_inner)
  }

  fn wait<'a>(&self, ring: MutexGuard<'a, Ring>) -> MutexGuard<'a, Ring> {
    self.data.wait(ring).unwrap_or_else(PoisonError::into_inner)
  }

  /// End the generation, if it is still the current one.
  fn finish(&self, generation: u64, end: End) {
    let mut ring = self.lock();
    if ring.generation == generation {
      ring.end = Some(end);
      self.data.notify_all();
    }
  }
}

fn closed_error() -> io::Error {
  io::Error::other("source closed")
}

// --- The consumer side ---

/// The synchronous side of a streamed source: `Read + Seek + Send`.
/// Dropping it closes the source.
pub struct Reader {
  shared: Arc<Shared>,
  pos: u64,
}

/// A clonable handle for the threads that do not own the reader: interrupt a
/// blocked read, or close the source.
#[derive(Clone)]
pub struct Handle {
  shared: Arc<Shared>,
}

impl Reader {
  /// Start pumping `producer` at offset 0 into a ring of `ring_bytes`
  /// (normally [`STREAM_RING_BYTES`]). Returns at once; [`facts`](Self::facts)
  /// or the first read waits for the open to answer.
  pub fn open(producer: impl Producer, ring_bytes: usize) -> Reader {
    let shared = Arc::new(Shared {
      ring: Mutex::new(Ring {
        generation: 0,
        chunks: VecDeque::new(),
        head: 0,
        buffered: 0,
        next: 0,
        end: None,
        restart: Some(0),
        facts: None,
        interrupt: false,
        closed: false,
      }),
      data: Condvar::new(),
      space: Notify::new(),
      restart: Notify::new(),
      cancel: CancellationToken::new(),
      ring_bytes,
    });
    io_runtime().spawn(pump(shared.clone(), producer));
    Reader { shared, pos: 0 }
  }

  pub fn handle(&self) -> Handle {
    Handle { shared: self.shared.clone() }
  }

  /// The source's length and seekability. Blocks until the first open has
  /// answered; errs when it failed or the source was closed. The error is
  /// sticky: reads repeat it.
  pub fn facts(&self) -> io::Result<Facts> {
    let ring = self.shared.lock();
    self.facts_locked(ring).map(|(facts, _)| facts)
  }

  fn facts_locked<'a>(&self, mut ring: MutexGuard<'a, Ring>) -> io::Result<(Facts, MutexGuard<'a, Ring>)> {
    loop {
      if let Some(facts) = ring.facts {
        return Ok((facts, ring));
      }
      if ring.closed {
        return Err(closed_error());
      }
      if let Some(End::Error(e)) = &ring.end {
        return Err(io::Error::other(e.clone()));
      }
      ring = self.shared.wait(ring);
    }
  }
}

impl Read for Reader {
  /// Blocks until bytes are buffered. `Ok(0)` only at the clean end of the
  /// source; a truncated or failed body is an error, repeated on every read.
  /// `WouldBlock` after an [`interrupt`](Handle::interrupt), with no byte
  /// consumed.
  fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
    if buf.is_empty() {
      return Ok(0);
    }
    let mut ring = self.shared.lock();
    loop {
      if ring.closed {
        return Err(closed_error());
      }
      if ring.interrupt {
        ring.interrupt = false;
        return Err(io::Error::new(ErrorKind::WouldBlock, "read interrupted"));
      }
      ring.discard_before(self.pos);
      if ring.buffered > 0 {
        let n = ring.take(buf);
        self.pos += n as u64;
        self.shared.space.notify_one();
        return Ok(n);
      }
      match &ring.end {
        Some(End::Clean) => return Ok(0),
        Some(End::Error(e)) => return Err(io::Error::other(e.clone())),
        None => ring = self.shared.wait(ring),
      }
    }
  }
}

impl Seek for Reader {
  /// `Current(0)` is a no-op (std and SDL call it to read the position).
  /// `End` needs a known length. A target still in the ring, or at or past a
  /// known end, is local; so is a forward skip within
  /// [`STREAM_SKIP_BY_READ_BYTES`] of what is in flight, or any forward skip
  /// on an unseekable source (read through and discarded). Anything else
  /// restarts the producer at the target; on an unseekable source that is
  /// `Unsupported`.
  fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
    let target = match from {
      SeekFrom::Current(0) => return Ok(self.pos),
      SeekFrom::Start(n) => Some(n),
      SeekFrom::Current(d) => self.pos.checked_add_signed(d),
      SeekFrom::End(_) => None,
    };
    let ring = self.shared.lock();
    let (facts, mut ring) = self.facts_locked(ring)?;
    let target = match from {
      SeekFrom::End(d) => {
        let len = facts.len.ok_or_else(|| {
          io::Error::new(ErrorKind::Unsupported, "seek from the end of a source with no known length")
        })?;
        len.checked_add_signed(d)
      }
      _ => target,
    }
    .ok_or_else(|| io::Error::new(ErrorKind::InvalidInput, "seek before the start"))?;
    if target == self.pos {
      return Ok(target);
    }
    if ring.closed {
      return Err(closed_error());
    }
    let in_ring = target >= ring.head && target <= ring.next;
    let past_end = facts.len.is_some_and(|len| target >= len);
    let short_skip = target > ring.next && target - ring.next < STREAM_SKIP_BY_READ_BYTES;
    let forward = target > self.pos;
    if past_end {
      // Nothing to fetch: reads answer the end until the next seek.
      ring.new_generation(target, false);
    } else if in_ring || (forward && (short_skip || !facts.seekable)) {
      // Local: reads discard up to the new position as bytes arrive.
      self.pos = target;
      ring.discard_before(target);
      self.shared.space.notify_one();
      return Ok(target);
    } else if !facts.seekable {
      return Err(io::Error::new(ErrorKind::Unsupported, "source cannot seek backwards"));
    } else {
      ring.new_generation(target, true);
    }
    self.pos = target;
    self.shared.restart.notify_one();
    self.shared.space.notify_one();
    Ok(target)
  }
}

impl Drop for Reader {
  fn drop(&mut self) {
    self.handle().close();
  }
}

impl Handle {
  /// Make the next read (the one blocked now, or the next call) return
  /// `WouldBlock` without consuming bytes, so the reading thread can take a
  /// command while the body keeps flowing. `WouldBlock` rather than
  /// `Interrupted`: std's `read_exact` swallows `Interrupted` and blocks
  /// again.
  pub fn interrupt(&self) {
    let mut ring = self.shared.lock();
    ring.interrupt = true;
    self.shared.data.notify_all();
  }

  /// Cancel the producer and fail every blocked or later call. Returns at
  /// once; the pump task winds down on the I/O runtime.
  pub fn close(&self) {
    let mut ring = self.shared.lock();
    ring.closed = true;
    self.shared.cancel.cancel();
    self.shared.data.notify_all();
    self.shared.space.notify_one();
    self.shared.restart.notify_one();
  }
}

// --- The pump ---

/// The forge I/O runtime: started on first use, on its own thread, never
/// dropped.
fn io_runtime() -> &'static tokio::runtime::Handle {
  static HANDLE: OnceLock<tokio::runtime::Handle> = OnceLock::new();
  HANDLE.get_or_init(|| {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::Builder::new()
      .name("forge-io".into())
      .spawn(move || {
        let runtime =
          tokio::runtime::Builder::new_current_thread().enable_all().build().expect("build the forge I/O runtime");
        let _ = tx.send(runtime.handle().clone());
        runtime.block_on(std::future::pending::<()>());
      })
      .expect("spawn the forge I/O thread");
    rx.recv().expect("the forge I/O runtime reports its handle")
  })
}

async fn pump(shared: Arc<Shared>, mut producer: impl Producer) {
  loop {
    let (generation, offset) = loop {
      {
        let mut ring = shared.lock();
        if ring.closed {
          return;
        }
        if let Some(offset) = ring.restart.take() {
          break (ring.generation, offset);
        }
      }
      shared.restart.notified().await;
    };
    run_generation(&shared, &mut producer, generation, offset).await;
  }
}

enum Wake<T> {
  Done(T),
  /// The generation changed under us, or the source closed.
  Gone,
}

/// Await `fut` unless the generation changes or the source closes first.
async fn wait<T>(shared: &Shared, generation: u64, fut: impl Future<Output = T>) -> Wake<T> {
  tokio::pin!(fut);
  loop {
    tokio::select! {
      r = &mut fut => return Wake::Done(r),
      _ = shared.cancel.cancelled() => return Wake::Gone,
      _ = shared.restart.notified() => {
        let gone = shared.lock().generation != generation;
        if gone {
          return Wake::Gone;
        }
      }
    }
  }
}

/// Wait for room, then push. False when the generation changed or the
/// source closed while the chunk was held.
async fn push(shared: &Shared, generation: u64, chunk: Bytes) -> bool {
  loop {
    {
      let mut ring = shared.lock();
      if ring.generation != generation || ring.closed {
        return false;
      }
      if ring.buffered == 0 || ring.buffered + chunk.len() <= shared.ring_bytes {
        ring.next += chunk.len() as u64;
        ring.buffered += chunk.len();
        ring.chunks.push_back(chunk);
        shared.data.notify_all();
        return true;
      }
    }
    shared.space.notified().await;
  }
}

/// The reconnect policy's state for one generation.
struct Retry {
  backoff: Duration,
  outage_since: Option<Instant>,
}

impl Retry {
  fn new() -> Retry {
    Retry { backoff: Duration::from_millis(RECONNECT_BACKOFF_MS), outage_since: None }
  }

  fn delivered(&mut self) {
    *self = Retry::new();
  }

  /// The delay before the next attempt, or None when the outage budget is
  /// spent.
  fn next_delay(&mut self) -> Option<Duration> {
    let since = *self.outage_since.get_or_insert_with(Instant::now);
    if since.elapsed() >= Duration::from_millis(RECONNECT_OUTAGE_MS) {
      return None;
    }
    let delay = self.backoff;
    self.backoff = (self.backoff * 2).min(Duration::from_millis(RECONNECT_BACKOFF_MAX_MS));
    Some(delay)
  }
}

/// One generation: open at `offset` and stream until the end, a failure the
/// reconnect policy does not cover, a restart or a close.
async fn run_generation(shared: &Shared, producer: &mut impl Producer, generation: u64, mut offset: u64) {
  let mut retry = Retry::new();
  loop {
    let opened = match wait(shared, generation, producer.open(offset)).await {
      Wake::Done(opened) => opened,
      Wake::Gone => return,
    };
    // The first answer fixes the facts; a reopen keeps them (the producer
    // vouched the source is the same).
    let (opened, facts) = {
      let mut ring = shared.lock();
      if ring.generation != generation || ring.closed {
        return;
      }
      match opened {
        Ok(opened) => {
          let facts = *ring.facts.get_or_insert(Facts { len: opened.len, seekable: opened.seekable });
          shared.data.notify_all();
          (Ok(opened.body), facts)
        }
        Err(e) => (Err(e), ring.facts.unwrap_or(Facts { len: None, seekable: false })),
      }
    };
    let mut body = match opened {
      Ok(body) => body,
      Err(e) => {
        let retryable = facts.seekable && e.kind() != ErrorKind::InvalidData;
        if !retryable {
          shared.finish(generation, End::Error(e.to_string()));
          return;
        }
        if !reconnect(shared, generation, &mut retry, e.to_string()).await {
          return;
        }
        continue;
      }
    };
    let trouble = loop {
      let next = next_chunk(&mut body, facts.seekable);
      match wait(shared, generation, next).await {
        Wake::Gone => return,
        Wake::Done(Ok(Some(Ok(chunk)))) => {
          let len = chunk.len() as u64;
          if !push(shared, generation, chunk).await {
            return;
          }
          offset += len;
          retry.delivered();
        }
        Wake::Done(Ok(Some(Err(e)))) => break e.to_string(),
        Wake::Done(Err(_stalled)) => break format!("no bytes for {STREAM_STALL_MS} ms"),
        Wake::Done(Ok(None)) => match facts.len {
          Some(len) if offset < len => break format!("ended at {offset} of {len} bytes"),
          Some(len) if offset > len => {
            shared.finish(generation, End::Error(format!("source changed: {offset} bytes past the announced {len}")));
            return;
          }
          _ => {
            shared.finish(generation, End::Clean);
            return;
          }
        },
      }
    };
    drop(body);
    if !facts.seekable {
      shared.finish(generation, End::Error(trouble));
      return;
    }
    if !reconnect(shared, generation, &mut retry, trouble).await {
      return;
    }
  }
}

/// The body's next chunk; on a seekable source, within the stall limit.
async fn next_chunk(
  body: &mut ByteStream,
  seekable: bool,
) -> Result<Option<io::Result<Bytes>>, tokio::time::error::Elapsed> {
  let next = std::future::poll_fn(|cx| body.as_mut().poll_next(cx));
  if seekable {
    tokio::time::timeout(Duration::from_millis(STREAM_STALL_MS), next).await
  } else {
    Ok(next.await)
  }
}

/// Sleep out the backoff before another attempt. False when the outage budget
/// is spent (the generation is then failed) or the generation is gone.
async fn reconnect(shared: &Shared, generation: u64, retry: &mut Retry, trouble: String) -> bool {
  let Some(delay) = retry.next_delay() else {
    shared.finish(generation, End::Error(format!("{trouble} (gave up after {RECONNECT_OUTAGE_MS} ms)")));
    return false;
  };
  log::info!("[forge::source] {trouble}; reconnecting in {} ms", delay.as_millis());
  matches!(wait(shared, generation, tokio::time::sleep(delay)).await, Wake::Done(()))
}

// --- The HTTP producer ---

/// A file served over HTTP. `GET` with `Range: bytes=N-`: a 206 is seekable,
/// with the length from `Content-Range` (`a-b/*` leaves it unknown); a 200 is
/// unseekable, with the length from `Content-Length` when there is one. No
/// HEAD probe (Bun sends Accept-Ranges only on 206 and 416). A reopen carries
/// `If-Range` with the first response's validator and is refused when its
/// range does not start at the offset with the first response's total.
pub struct HttpSource {
  url: String,
  client: reqwest::Client,
  identity: Option<Identity>,
}

/// What the first response said, for the reopens to match.
struct Identity {
  total: Option<u64>,
  /// The ETag, else Last-Modified: the `If-Range` value.
  validator: Option<String>,
}

impl HttpSource {
  /// One reqwest client per user agent, shared by every source.
  pub fn new(url: &str, user_agent: &str) -> io::Result<HttpSource> {
    static CLIENTS: OnceLock<Mutex<HashMap<String, reqwest::Client>>> = OnceLock::new();
    let mut clients = CLIENTS.get_or_init(Default::default).lock().unwrap_or_else(PoisonError::into_inner);
    let client = match clients.get(user_agent) {
      Some(client) => client.clone(),
      None => {
        let client = reqwest::Client::builder().user_agent(user_agent).build().map_err(io::Error::other)?;
        clients.insert(user_agent.to_string(), client.clone());
        client
      }
    };
    Ok(HttpSource { url: url.to_string(), client, identity: None })
  }

  async fn open_at(&mut self, offset: u64) -> io::Result<Opened> {
    use reqwest::header::{CONTENT_LENGTH, CONTENT_RANGE, ETAG, IF_RANGE, LAST_MODIFIED, RANGE};
    let mut request = self.client.get(&self.url).header(RANGE, format!("bytes={offset}-"));
    if let Some(validator) = self.identity.as_ref().and_then(|id| id.validator.as_deref()) {
      request = request.header(IF_RANGE, validator);
    }
    let response = request.send().await.map_err(io::Error::other)?;
    let status = response.status().as_u16();
    let header = |name| response.headers().get(name).and_then(|v| v.to_str().ok());
    let (start, len, seekable) = match status {
      206 => {
        let range = header(CONTENT_RANGE).ok_or_else(|| io::Error::other("206 without content-range"))?;
        let (start, total) =
          parse_content_range(range).ok_or_else(|| io::Error::other(format!("bad content-range {range:?}")))?;
        (start, total, true)
      }
      200 => (0, header(CONTENT_LENGTH).and_then(|v| v.parse().ok()), false),
      416 => return Err(final_error(format!("HTTP 416: offset {offset} is past the end of {}", self.url))),
      _ => return Err(final_error(format!("HTTP {status} for {}", self.url))),
    };
    if start != offset {
      return Err(final_error(format!("source changed: got offset {start}, asked for {offset}")));
    }
    match &self.identity {
      Some(id) if id.total != len => {
        return Err(final_error(format!("source changed: length {len:?}, was {:?}", id.total)));
      }
      Some(_) => {}
      None => {
        let validator = header(ETAG).or_else(|| header(LAST_MODIFIED)).map(str::to_string);
        self.identity = Some(Identity { total: len, validator });
      }
    }
    Ok(Opened { body: to_byte_stream(response.bytes_stream()), len, seekable })
  }
}

impl Producer for HttpSource {
  fn open(&mut self, offset: u64) -> OpenFuture<'_> {
    Box::pin(self.open_at(offset))
  }
}

/// A failure no reconnect can mend: the source is not what was asked for.
fn final_error(message: String) -> io::Error {
  io::Error::new(ErrorKind::InvalidData, message)
}

/// `bytes a-b/total` or `bytes a-b/*` -> (a, total).
fn parse_content_range(value: &str) -> Option<(u64, Option<u64>)> {
  let spec = value.trim().strip_prefix("bytes")?.trim_start();
  let (range, total) = spec.split_once('/')?;
  let (start, _end) = range.split_once('-')?;
  let start = start.trim().parse().ok()?;
  let total = match total.trim() {
    "*" => None,
    n => Some(n.parse().ok()?),
  };
  Some((start, total))
}
