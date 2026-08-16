use bytes::Bytes;
use futures_core::Stream;
use std::collections::VecDeque;
use std::io;
use std::path::PathBuf;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use crate::cache::Cache;
use crate::stream::ByteStream;

fn temp_dir(name: &str) -> PathBuf {
  let dir = std::env::temp_dir().join(format!("forge-cache-test-{}-{}", name, std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  dir
}

struct VecStream(VecDeque<Result<Bytes, io::Error>>);

impl Stream for VecStream {
  type Item = Result<Bytes, io::Error>;

  fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
    Poll::Ready(self.0.pop_front())
  }
}

fn body(chunks: &[&[u8]]) -> ByteStream {
  Box::pin(VecStream(chunks.iter().map(|c| Ok(Bytes::copy_from_slice(c))).collect()))
}

fn failing_body(chunks: &[&[u8]]) -> ByteStream {
  let mut items: VecDeque<Result<Bytes, io::Error>> = chunks.iter().map(|c| Ok(Bytes::copy_from_slice(c))).collect();
  items.push_back(Err(io::Error::other("connection reset")));
  Box::pin(VecStream(items))
}

async fn next_chunk(s: &mut ByteStream) -> Option<Result<Bytes, io::Error>> {
  std::future::poll_fn(|cx| s.as_mut().poll_next(cx)).await
}

/// Drains a stream, returning the concatenated bytes and whether it ended in
/// an error.
async fn drain(mut s: ByteStream) -> (Vec<u8>, bool) {
  let mut out = Vec::new();
  while let Some(item) = next_chunk(&mut s).await {
    match item {
      Ok(chunk) => out.extend_from_slice(&chunk),
      Err(_) => return (out, true),
    }
  }
  (out, false)
}

/// Commit happens in a background writer task; poll until the entry appears.
async fn wait_for_entry(cache: &Cache, key: &str) -> (Vec<u8>, Vec<u8>) {
  for _ in 0..200 {
    if let Some((meta, body)) = cache.lookup(key).await {
      let (bytes, errored) = drain(body).await;
      assert!(!errored, "cached body errored");
      return (meta, bytes);
    }
    tokio::time::sleep(Duration::from_millis(10)).await;
  }
  panic!("entry for {key} never committed");
}

/// The writer's temp file is removed by a background task; wait for the
/// directory to empty out (bounded), then assert nothing was left behind.
async fn wait_for_empty(dir: &std::path::Path) {
  for _ in 0..200 {
    if std::fs::read_dir(dir).map(|rd| rd.count()).unwrap_or(0) == 0 {
      return;
    }
    tokio::time::sleep(Duration::from_millis(10)).await;
  }
  panic!("cache dir {} not cleaned up", dir.display());
}

#[tokio::test]
async fn scan_lists_committed_entries() {
  let dir = temp_dir("scan");
  let cache = Cache::new(dir.clone(), 1024 * 1024);
  let tee = cache.store("k1", b"meta1".to_vec(), body(&[b"abc"]));
  drain(tee).await;
  wait_for_entry(&cache, "k1").await;
  // A stray tmp file (an in-flight writer) is not an entry.
  std::fs::write(dir.join("deadbeef.123-4.tmp"), b"partial").expect("tmp file");

  let entries = crate::cache::scan(&dir);
  assert_eq!(entries.len(), 1);
  assert_eq!(entries[0].meta, b"meta1");
  // Length prefix + meta + body.
  assert_eq!(entries[0].size, 4 + 5 + 3);

  // A missing dir is an empty cache.
  assert!(crate::cache::scan(&temp_dir("scan-missing")).is_empty());
  let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn roundtrip() {
  let cache = Cache::new(temp_dir("roundtrip"), 1024 * 1024);
  let tee = cache.store("k", b"meta-blob".to_vec(), body(&[b"hello ", b"world"]));
  let (passthrough, errored) = drain(tee).await;
  assert!(!errored);
  assert_eq!(passthrough, b"hello world");

  let (meta, bytes) = wait_for_entry(&cache, "k").await;
  assert_eq!(meta, b"meta-blob");
  assert_eq!(bytes, b"hello world");
  assert!(cache.lookup("other-key").await.is_none());
}

#[tokio::test]
async fn no_commit_on_body_error() {
  let dir = temp_dir("body-error");
  let cache = Cache::new(dir.clone(), 1024 * 1024);
  let tee = cache.store("k", b"m".to_vec(), failing_body(&[b"partial"]));
  let (passthrough, errored) = drain(tee).await;
  // The consumer sees exactly what the network delivered, error included.
  assert_eq!(passthrough, b"partial");
  assert!(errored);

  // Nothing committed and the temp file is cleaned up.
  wait_for_empty(&dir).await;
  assert!(cache.lookup("k").await.is_none());
}

#[tokio::test]
async fn no_commit_on_abandoned_stream() {
  let dir = temp_dir("abandoned");
  let cache = Cache::new(dir.clone(), 1024 * 1024);
  let mut tee = cache.store("k", b"m".to_vec(), body(&[b"first", b"second"]));
  let first = next_chunk(&mut tee).await.expect("first chunk").expect("first chunk ok");
  assert_eq!(&first[..], b"first");
  drop(tee);

  wait_for_empty(&dir).await;
  assert!(cache.lookup("k").await.is_none());
}

#[tokio::test]
async fn per_entry_cap_aborts_write_not_consumer() {
  // max 100 -> per-entry cap 25; a 30-byte body is passed through untouched
  // but never cached.
  let cache = Cache::new(temp_dir("entry-cap"), 100);
  let tee = cache.store("k", b"m".to_vec(), body(&[&[0u8; 15], &[1u8; 15]]));
  let (passthrough, errored) = drain(tee).await;
  assert!(!errored);
  assert_eq!(passthrough.len(), 30);

  tokio::time::sleep(Duration::from_millis(100)).await;
  assert!(cache.lookup("k").await.is_none());
}

#[tokio::test]
async fn lru_eviction_prefers_recently_used() {
  // Entry file = 4 (meta len) + 1 (meta) + 10 (body) = 15 bytes; cap 80 holds
  // five entries (75), a sixth (90) evicts the least recently used.
  let cache = Cache::new(temp_dir("lru"), 80);
  for key in ["k1", "k2", "k3", "k4", "k5"] {
    let tee = cache.store(key, b"m".to_vec(), body(&[&[7u8; 10]]));
    drain(tee).await;
    wait_for_entry(&cache, key).await;
    // mtime is the LRU clock; keep the write order distinguishable.
    tokio::time::sleep(Duration::from_millis(20)).await;
  }

  // Touch k1 so k2 becomes the least recently used.
  assert!(cache.lookup("k1").await.is_some());
  tokio::time::sleep(Duration::from_millis(20)).await;

  let tee = cache.store("k6", b"m".to_vec(), body(&[&[7u8; 10]]));
  drain(tee).await;
  wait_for_entry(&cache, "k6").await;

  assert!(cache.lookup("k2").await.is_none(), "least recently used entry should be evicted");
  assert!(cache.lookup("k1").await.is_some(), "recently touched entry should survive");
  assert!(cache.lookup("k5").await.is_some());
}
