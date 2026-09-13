use bytes::Bytes;
use futures_core::Stream;
use std::collections::VecDeque;
use std::io::{self, ErrorKind, Read, Seek, SeekFrom};
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::task::{Context, Poll};
use std::thread;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{mpsc, Notify};

use crate::source::{Facts, HttpSource, OpenFuture, Opened, Producer, Reader, STREAM_RING_BYTES};
use crate::stream::{from_bytes, ByteStream};

/// A ring small enough that a few chunks fill it.
const SMALL_RING: usize = 32;
/// How long a close or an interrupt may take to unblock a call.
const PROMPT: Duration = Duration::from_secs(2);
/// Long enough for the pump to reach a blocking point.
const SETTLE: Duration = Duration::from_millis(200);

fn file(n: usize) -> Vec<u8> {
  (0..n).map(|i| (i * 7 + 3) as u8).collect()
}

pub(super) fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
  m.lock().unwrap_or_else(PoisonError::into_inner)
}

// --- A scripted producer ---

struct ChanStream(mpsc::UnboundedReceiver<io::Result<Bytes>>);

impl Stream for ChanStream {
  type Item = io::Result<Bytes>;
  fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
    self.0.poll_recv(cx)
  }
}

type Feed = mpsc::UnboundedSender<io::Result<Bytes>>;

/// A body fed by hand: chunks, then the end (drop the feed) or an error.
fn channel_body() -> (Feed, ByteStream) {
  let (tx, rx) = mpsc::unbounded_channel();
  (tx, Box::pin(ChanStream(rx)))
}

fn feed(tx: &Feed, bytes: &[u8]) {
  tx.send(Ok(Bytes::copy_from_slice(bytes))).expect("the body is still open");
}

/// Answers each open from a script, recording the offsets asked for.
struct Scripted {
  answers: Mutex<VecDeque<io::Result<Opened>>>,
  opens: Arc<Mutex<Vec<u64>>>,
}

impl Producer for Scripted {
  fn open(&mut self, offset: u64) -> OpenFuture<'_> {
    lock(&self.opens).push(offset);
    let answer = lock(&self.answers).pop_front().unwrap_or_else(|| Err(io::Error::other("script exhausted")));
    Box::pin(async move { answer })
  }
}

fn scripted(answers: Vec<io::Result<Opened>>) -> (Scripted, Arc<Mutex<Vec<u64>>>) {
  let opens = Arc::new(Mutex::new(Vec::new()));
  (Scripted { answers: Mutex::new(answers.into()), opens: opens.clone() }, opens)
}

fn opened(body: ByteStream, len: Option<u64>, seekable: bool) -> io::Result<Opened> {
  Ok(Opened { body, len, seekable })
}

// --- A scripted HTTP server ---

pub(super) struct Req {
  pub(super) headers: Vec<(String, String)>,
}

impl Req {
  pub(super) fn header(&self, name: &str) -> Option<&str> {
    self.headers.iter().find(|(n, _)| n == name).map(|(_, v)| v.as_str())
  }

  pub(super) fn range_start(&self) -> usize {
    self
      .header("range")
      .and_then(|r| r.strip_prefix("bytes="))
      .and_then(|r| r.split('-').next())
      .and_then(|n| n.parse().ok())
      .unwrap_or(0)
  }
}

pub(super) enum Step {
  Send(Vec<u8>),
  Wait(Arc<Notify>),
}

/// The status line and headers, then the steps; the socket closes after the
/// last step (which is the truncation when the body is not complete).
pub(super) struct Script {
  pub(super) head: String,
  pub(super) steps: Vec<Step>,
}

pub(super) fn head(status: &str, headers: &[(&str, String)]) -> String {
  let mut s = format!("HTTP/1.1 {status}\r\nconnection: close\r\n");
  for (name, value) in headers {
    s += &format!("{name}: {value}\r\n");
  }
  s + "\r\n"
}

async fn read_request(sock: &mut tokio::net::TcpStream) -> Req {
  let mut raw = Vec::new();
  let mut buf = [0u8; 1024];
  while !raw.windows(4).any(|w| w == b"\r\n\r\n") {
    let Ok(n) = sock.read(&mut buf).await else { break };
    if n == 0 {
      break;
    }
    raw.extend_from_slice(&buf[..n]);
  }
  let text = String::from_utf8_lossy(&raw);
  let headers = text
    .lines()
    .skip(1)
    .take_while(|l| !l.is_empty())
    .filter_map(|l| l.split_once(':'))
    .map(|(n, v)| (n.trim().to_ascii_lowercase(), v.trim().to_string()))
    .collect();
  Req { headers }
}

/// Serve `handler` on a runtime of its own, one task per connection,
/// recording every request.
pub(super) fn serve(handler: impl Fn(&Req) -> Script + Send + Sync + 'static) -> (SocketAddr, Arc<Mutex<Vec<Req>>>) {
  let requests = Arc::new(Mutex::new(Vec::new()));
  let seen = requests.clone();
  let handler = Arc::new(handler);
  let (tx, rx) = std::sync::mpsc::channel();
  thread::spawn(move || {
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().expect("test runtime");
    runtime.block_on(async move {
      let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind the test server");
      tx.send(listener.local_addr().expect("test server addr")).expect("report the addr");
      loop {
        let Ok((mut sock, _)) = listener.accept().await else { return };
        let (handler, seen) = (handler.clone(), seen.clone());
        tokio::spawn(async move {
          let req = read_request(&mut sock).await;
          let script = handler(&req);
          lock(&seen).push(req);
          let _ = sock.write_all(script.head.as_bytes()).await;
          for step in script.steps {
            match step {
              Step::Send(bytes) => {
                let _ = sock.write_all(&bytes).await;
              }
              Step::Wait(notify) => notify.notified().await,
            }
          }
        });
      }
    });
  });
  (rx.recv().expect("the test server reports its addr"), requests)
}

/// A file served with Range support: a 206 from the requested offset.
pub(super) fn ranged(data: Arc<Vec<u8>>, etag: &'static str) -> impl Fn(&Req) -> Script + Send + Sync + 'static {
  move |req| {
    let start = req.range_start();
    let body = data[start..].to_vec();
    Script {
      head: head(
        "206 Partial Content",
        &[
          ("content-range", format!("bytes {start}-{}/{}", data.len() - 1, data.len())),
          ("content-length", body.len().to_string()),
          ("etag", etag.to_string()),
        ],
      ),
      steps: vec![Step::Send(body)],
    }
  }
}

pub(super) fn chunk(bytes: &[u8]) -> Vec<u8> {
  let mut out = format!("{:x}\r\n", bytes.len()).into_bytes();
  out.extend_from_slice(bytes);
  out.extend_from_slice(b"\r\n");
  out
}

fn http_reader(addr: SocketAddr, ring: usize) -> Reader {
  let source = HttpSource::new(&format!("http://{addr}/clip.webm"), "forge-test").expect("build the source");
  Reader::open(source, ring)
}

/// Read until the end or an error, keeping what arrived before it.
fn read_all(reader: &mut Reader) -> (Vec<u8>, io::Result<()>) {
  let mut out = Vec::new();
  let mut buf = [0u8; 4096];
  loop {
    match reader.read(&mut buf) {
      Ok(0) => return (out, Ok(())),
      Ok(n) => out.extend_from_slice(&buf[..n]),
      Err(e) => return (out, Err(e)),
    }
  }
}

// --- HTTP ---

#[test]
fn http_range_source_reads_and_seeks() {
  let data = Arc::new(file(1_200_000));
  let (addr, requests) = serve(ranged(data.clone(), "\"v1\""));
  let mut reader = http_reader(addr, STREAM_RING_BYTES);
  assert_eq!(reader.facts().expect("facts"), Facts { len: Some(1_200_000), seekable: true });

  let mut first = vec![0u8; 1000];
  reader.read_exact(&mut first).expect("first bytes");
  assert_eq!(first, data[..1000]);

  // Far forward: a restart at the target.
  assert_eq!(reader.seek(SeekFrom::Start(1_000_000)).expect("seek"), 1_000_000);
  let mut mid = vec![0u8; 100];
  reader.read_exact(&mut mid).expect("bytes after the seek");
  assert_eq!(mid, data[1_000_000..1_000_100]);

  // The position read, then a short forward skip (local) to the tail.
  assert_eq!(reader.seek(SeekFrom::Current(0)).expect("position"), 1_000_100);
  assert_eq!(reader.seek(SeekFrom::End(-10)).expect("seek from end"), 1_199_990);
  let (tail, end) = read_all(&mut reader);
  end.expect("clean end");
  assert_eq!(tail, data[1_199_990..]);

  // Backwards: a restart.
  assert_eq!(reader.seek(SeekFrom::Start(0)).expect("rewind"), 0);
  reader.read_exact(&mut first).expect("bytes after the rewind");
  assert_eq!(first, data[..1000]);

  let requests = lock(&requests);
  let ranges: Vec<_> = requests.iter().map(|r| r.header("range").unwrap_or("").to_string()).collect();
  assert_eq!(ranges, ["bytes=0-", "bytes=1000000-", "bytes=0-"]);
}

#[test]
fn http_200_is_unseekable() {
  let data = Arc::new(file(10_000));
  let served = data.clone();
  let (addr, requests) = serve(move |_| Script {
    head: head("200 OK", &[("content-length", served.len().to_string())]),
    steps: vec![Step::Send(served.to_vec())],
  });
  let mut reader = http_reader(addr, STREAM_RING_BYTES);
  assert_eq!(reader.facts().expect("facts"), Facts { len: Some(10_000), seekable: false });

  let mut buf = vec![0u8; 100];
  reader.read_exact(&mut buf).expect("first bytes");
  assert_eq!(reader.seek(SeekFrom::Current(0)).expect("position"), 100);
  let back = reader.seek(SeekFrom::Start(50)).expect_err("a restart on a 200");
  assert_eq!(back.kind(), ErrorKind::Unsupported);

  // Forward is read through.
  assert_eq!(reader.seek(SeekFrom::Start(5_000)).expect("forward skip"), 5_000);
  let mut buf = vec![0u8; 10];
  reader.read_exact(&mut buf).expect("bytes after the skip");
  assert_eq!(buf, data[5_000..5_010]);

  // At the end: nothing more, no request.
  assert_eq!(reader.seek(SeekFrom::End(0)).expect("seek to end"), 10_000);
  assert_eq!(reader.read(&mut buf).expect("read at the end"), 0);
  assert_eq!(lock(&requests).len(), 1);
}

#[test]
fn chunked_truncation_is_an_error() {
  let (addr, _) = serve(|_| Script {
    head: head("200 OK", &[("transfer-encoding", "chunked".to_string())]),
    steps: vec![Step::Send(chunk(b"hello")), Step::Send(chunk(b" world"))],
  });
  let mut reader = http_reader(addr, STREAM_RING_BYTES);
  assert_eq!(reader.facts().expect("facts"), Facts { len: None, seekable: false });
  let (got, end) = read_all(&mut reader);
  assert_eq!(got, b"hello world");
  let err = end.expect_err("a body cut before its last chunk is not the end");
  assert_ne!(err.kind(), ErrorKind::WouldBlock);
  // Sticky.
  reader.read(&mut [0u8; 1]).expect_err("the error repeats");
}

#[test]
fn early_end_on_an_unseekable_source_is_an_error() {
  let data = file(100);
  let (addr, _) = serve(move |_| Script {
    head: head("200 OK", &[("content-length", "100".to_string())]),
    steps: vec![Step::Send(data[..50].to_vec())],
  });
  let mut reader = http_reader(addr, STREAM_RING_BYTES);
  let (got, end) = read_all(&mut reader);
  assert_eq!(got.len(), 50);
  end.expect_err("50 of 100 bytes is not the end");
}

#[test]
fn reconnect_resumes_at_the_offset_with_if_range() {
  let data = Arc::new(file(20_000));
  let served = data.clone();
  let full = ranged(data.clone(), "\"v1\"");
  let hits = AtomicUsize::new(0);
  let (addr, requests) = serve(move |req| {
    if hits.fetch_add(1, Ordering::SeqCst) == 0 {
      // The whole range announced, 8000 bytes sent, then the socket closes.
      let mut script = full(req);
      script.steps = vec![Step::Send(served[..8_000].to_vec())];
      script
    } else {
      full(req)
    }
  });
  let mut reader = http_reader(addr, STREAM_RING_BYTES);
  let (got, end) = read_all(&mut reader);
  end.expect("resumed to a clean end");
  assert_eq!(got, *data);

  let requests = lock(&requests);
  assert_eq!(requests.len(), 2);
  assert_eq!(requests[1].header("range"), Some("bytes=8000-"));
  assert_eq!(requests[1].header("if-range"), Some("\"v1\""));
}

#[test]
fn reconnect_rejects_a_changed_source() {
  let data = Arc::new(file(20_000));
  let hits = AtomicUsize::new(0);
  let (addr, requests) = serve(move |req| {
    let start = req.range_start();
    if hits.fetch_add(1, Ordering::SeqCst) == 0 {
      Script {
        head: head(
          "206 Partial Content",
          &[("content-range", format!("bytes 0-19999/20000")), ("content-length", "20000".to_string())],
        ),
        steps: vec![Step::Send(data[..8_000].to_vec())],
      }
    } else {
      // A different file now: the total moved.
      Script {
        head: head(
          "206 Partial Content",
          &[("content-range", format!("bytes {start}-20000/20001")), ("content-length", "12001".to_string())],
        ),
        steps: vec![Step::Send(vec![0u8; 12_001])],
      }
    }
  });
  let mut reader = http_reader(addr, STREAM_RING_BYTES);
  let (got, end) = read_all(&mut reader);
  assert_eq!(got.len(), 8_000);
  let err = end.expect_err("a changed source is an error");
  assert!(err.to_string().contains("source changed"), "{err}");
  // Final: no third attempt.
  assert_eq!(lock(&requests).len(), 2);
}

#[test]
fn first_open_failure_is_sticky() {
  let (addr, _) =
    serve(|_| Script { head: head("404 Not Found", &[("content-length", "0".to_string())]), steps: vec![] });
  let mut reader = http_reader(addr, STREAM_RING_BYTES);
  let err = reader.facts().expect_err("a 404 fails the open");
  assert!(err.to_string().contains("HTTP 404"), "{err}");
  reader.read(&mut [0u8; 1]).expect_err("reads repeat it");
  reader.seek(SeekFrom::Start(1)).expect_err("seeks repeat it");
}

#[test]
fn close_returns_promptly_during_a_pending_open() {
  let never = Arc::new(Notify::new());
  let hold = never.clone();
  let (addr, _) = serve(move |_| Script { head: String::new(), steps: vec![Step::Wait(hold.clone())] });
  let reader = http_reader(addr, STREAM_RING_BYTES);
  let handle = reader.handle();
  let pending = thread::spawn(move || reader.facts());
  thread::sleep(SETTLE);
  let started = Instant::now();
  handle.close();
  let result = pending.join().expect("the facts thread");
  assert!(started.elapsed() < PROMPT, "close took {:?}", started.elapsed());
  result.expect_err("closed before the open answered");
}

// --- The ring ---

#[test]
fn seek_racing_a_held_chunk_lands_no_stale_bytes() {
  let (feed_a, body_a) = channel_body();
  let (feed_b, body_b) = channel_body();
  let (producer, opens) = scripted(vec![opened(body_a, Some(2_000_000), true), opened(body_b, Some(2_000_000), true)]);
  let mut reader = Reader::open(producer, SMALL_RING);
  // Four 16-byte chunks: one read, two in the ring, the fourth held by the
  // producer blocked on the full ring.
  for i in 1..=4u8 {
    feed(&feed_a, &[i; 16]);
  }
  let mut buf = [0u8; 16];
  reader.read_exact(&mut buf).expect("first chunk");
  assert_eq!(buf, [1u8; 16]);
  thread::sleep(SETTLE);

  assert_eq!(reader.seek(SeekFrom::Start(1_000_000)).expect("restart"), 1_000_000);
  feed(&feed_b, &[9; 16]);
  reader.read_exact(&mut buf).expect("bytes of the new generation");
  assert_eq!(buf, [9u8; 16]);
  assert_eq!(*lock(&opens), [0, 1_000_000]);
}

#[test]
fn interrupt_wakes_a_blocked_read_once() {
  let (feed_tx, body) = channel_body();
  let (producer, _) = scripted(vec![opened(body, None, false)]);
  let mut reader = Reader::open(producer, STREAM_RING_BYTES);
  let handle = reader.handle();
  let mut buf = [0u8; 3];
  feed(&feed_tx, b"abc");
  reader.read_exact(&mut buf).expect("first bytes");

  let blocked = thread::spawn(move || {
    let mut buf = [0u8; 3];
    let r = reader.read(&mut buf);
    (reader, r)
  });
  thread::sleep(SETTLE);
  let started = Instant::now();
  handle.interrupt();
  let (mut reader, r) = blocked.join().expect("the reading thread");
  assert!(started.elapsed() < PROMPT);
  assert_eq!(r.expect_err("woken without bytes").kind(), ErrorKind::WouldBlock);

  // The body kept flowing; only the one read was released.
  feed(&feed_tx, b"def");
  reader.read_exact(&mut buf).expect("bytes after the interrupt");
  assert_eq!(&buf, b"def");

  // An interrupt with bytes waiting is consumed by the next read.
  feed(&feed_tx, b"ghi");
  handle.interrupt();
  assert_eq!(reader.read(&mut buf).expect_err("the next read").kind(), ErrorKind::WouldBlock);
  reader.read_exact(&mut buf).expect("then the bytes");
  assert_eq!(&buf, b"ghi");
}

#[test]
fn close_returns_promptly_during_a_stall() {
  let (_feed, body) = channel_body();
  let (producer, _) = scripted(vec![opened(body, None, false)]);
  let mut reader = Reader::open(producer, STREAM_RING_BYTES);
  let handle = reader.handle();
  let blocked = thread::spawn(move || reader.read(&mut [0u8; 8]).map(|_| ()));
  thread::sleep(SETTLE);
  let started = Instant::now();
  handle.close();
  let r = blocked.join().expect("the reading thread");
  assert!(started.elapsed() < PROMPT);
  r.expect_err("a closed source fails the read");
}

#[test]
fn seek_from_the_end_needs_a_length() {
  let (producer, _) = scripted(vec![opened(from_bytes(&b"0123456789"[..]), None, true)]);
  let mut reader = Reader::open(producer, STREAM_RING_BYTES);
  let err = reader.seek(SeekFrom::End(0)).expect_err("no length");
  assert_eq!(err.kind(), ErrorKind::Unsupported);
  let err = reader.seek(SeekFrom::Current(-1)).expect_err("before the start");
  assert_eq!(err.kind(), ErrorKind::InvalidInput);
  let (got, end) = read_all(&mut reader);
  end.expect("clean end");
  assert_eq!(got, b"0123456789");
}

#[test]
fn a_transient_open_failure_on_a_seekable_source_is_retried() {
  let (producer, opens) = scripted(vec![
    opened(from_bytes(&b"01234"[..]), Some(10), true),
    Err(io::Error::other("connection reset")),
    opened(from_bytes(&b"56789"[..]), Some(10), true),
  ]);
  let mut reader = Reader::open(producer, STREAM_RING_BYTES);
  // The first body ends 5 short of the announced 10: a reconnect, whose
  // first attempt fails and whose second delivers the rest.
  let (got, end) = read_all(&mut reader);
  end.expect("clean end after the reconnects");
  assert_eq!(got, b"0123456789");
  assert_eq!(*lock(&opens), [0, 5, 5]);
}

// --- The source rule ---

#[test]
fn source_rule_takes_paths_and_http_urls_only() {
  use crate::source::Source;
  assert_eq!(Source::parse("clip.webm"), Ok(Source::Path("clip.webm".into())));
  assert_eq!(Source::parse("/media/clip.webm"), Ok(Source::Path("/media/clip.webm".into())));
  assert_eq!(Source::parse("C:\\media\\clip.webm"), Ok(Source::Path("C:\\media\\clip.webm".into())));
  assert_eq!(Source::parse("http://host:8080/clip.webm"), Ok(Source::Http("http://host:8080/clip.webm".into())));
  assert_eq!(Source::parse("HTTPS://host/clip.webm"), Ok(Source::Http("HTTPS://host/clip.webm".into())));
  assert!(Source::parse("file:///media/clip.webm").is_err());
  assert!(Source::parse("ftp://host/clip.webm").is_err());
  assert!(Source::parse("data:video/webm;base64,AAAA").is_err());
}
