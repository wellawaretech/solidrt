use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::cache::Cache;
use crate::fetch::{cached_meta, do_fetch_cached, CacheMode, HostLimits};

async fn acquires_within(limits: &HostLimits, host: &str, ms: u64) -> Option<tokio::sync::OwnedSemaphorePermit> {
  tokio::time::timeout(Duration::from_millis(ms), limits.acquire(host)).await.ok()
}

#[tokio::test]
async fn per_host_limit_blocks_and_releases() {
  let limits = HostLimits::new(2);
  let p1 = limits.acquire("a:80").await;
  let _p2 = limits.acquire("a:80").await;

  // Third slot on the same host waits...
  assert!(acquires_within(&limits, "a:80", 50).await.is_none());
  // ...while another host is unaffected.
  assert!(acquires_within(&limits, "b:80", 50).await.is_some());

  // Dropping a permit frees the slot.
  drop(p1);
  assert!(acquires_within(&limits, "a:80", 1000).await.is_some());
}

#[tokio::test]
async fn cooldown_pauses_host_until_expiry() {
  let limits = HostLimits::new(2);
  limits.cooldown("a:80", Duration::from_millis(300));

  // The cooled-down host holds acquire back...
  assert!(acquires_within(&limits, "a:80", 50).await.is_none());
  // ...while another host is unaffected...
  assert!(acquires_within(&limits, "b:80", 50).await.is_some());
  // ...and the pause lifts once the cooldown expires.
  assert!(acquires_within(&limits, "a:80", 1000).await.is_some());
}

#[tokio::test]
async fn cooldown_extends_never_shortens() {
  let limits = HostLimits::new(1);
  limits.cooldown("a:80", Duration::from_millis(400));
  limits.cooldown("a:80", Duration::from_millis(10));
  assert!(acquires_within(&limits, "a:80", 100).await.is_none());
}

#[test]
fn cached_meta_reads_fetch_meta() {
  let meta = br#"{"status":200,"status_text":"OK","url":"https://host/img.jpg","headers":[["content-type","image/JPEG; charset=binary"]]}"#;
  let decoded = cached_meta(meta).expect("decodes");
  assert_eq!(decoded.url, "https://host/img.jpg");
  assert_eq!(decoded.content_type.as_deref(), Some("image/jpeg"));

  let no_type = br#"{"status":200,"status_text":"OK","url":"https://host/x","headers":[]}"#;
  assert_eq!(cached_meta(no_type).expect("decodes").content_type, None);
  assert!(cached_meta(b"not a fetch meta blob").is_none());
}

// --- 429 retry, end to end against a scripted server ---

const TOO_MANY: &str =
  "HTTP/1.1 429 Too Many Requests\r\nretry-after: 0\r\ncontent-length: 0\r\nconnection: close\r\n\r\n";
const TOO_MANY_LATER: &str =
  "HTTP/1.1 429 Too Many Requests\r\nretry-after: 3600\r\ncontent-length: 0\r\nconnection: close\r\n\r\n";
const OK: &str = "HTTP/1.1 200 OK\r\ncontent-length: 2\r\nconnection: close\r\n\r\nok";

fn temp_dir(name: &str) -> PathBuf {
  let dir = std::env::temp_dir().join(format!("forge-fetch-test-{}-{}", name, std::process::id()));
  let _ = std::fs::remove_dir_all(&dir);
  dir
}

/// Serve the canned responses one connection each (`connection: close` keeps
/// reqwest from reusing sockets), counting the requests actually received.
async fn scripted_server(responses: Vec<&'static str>) -> (std::net::SocketAddr, Arc<AtomicUsize>) {
  let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind test server");
  let addr = listener.local_addr().expect("test server addr");
  let hits = Arc::new(AtomicUsize::new(0));
  let count = hits.clone();
  tokio::spawn(async move {
    for resp in responses {
      let Ok((mut sock, _)) = listener.accept().await else { return };
      let mut buf = [0u8; 1024];
      let _ = sock.read(&mut buf).await;
      count.fetch_add(1, Ordering::SeqCst);
      let _ = sock.write_all(resp.as_bytes()).await;
    }
  });
  (addr, hits)
}

async fn fetch_asset(url: &str, cache_dir: &str) -> crate::fetch::ResponseData {
  let cache = Rc::new(Cache::new(temp_dir(cache_dir), 1024 * 1024));
  let limits = Rc::new(HostLimits::new(2));
  let client = Rc::new(reqwest::Client::new());
  do_fetch_cached(client, "GET", url, vec![], None, cache, CacheMode::ForceCache, limits).await.expect("fetch")
}

#[tokio::test]
async fn retries_after_429() {
  let (addr, hits) = scripted_server(vec![TOO_MANY, OK]).await;
  let resp = fetch_asset(&format!("http://{addr}/img"), "retry").await;
  assert_eq!(resp.status, 200);
  assert_eq!(hits.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn retry_limit_bounds_attempts() {
  let (addr, hits) = scripted_server(vec![TOO_MANY; 8]).await;
  let resp = fetch_asset(&format!("http://{addr}/img"), "retry-limit").await;
  assert_eq!(resp.status, 429);
  // The initial request plus RETRY_LIMIT retries.
  assert_eq!(hits.load(Ordering::SeqCst), 4);
}

#[tokio::test]
async fn distant_retry_after_gives_up() {
  let (addr, hits) = scripted_server(vec![TOO_MANY_LATER, OK]).await;
  let resp = fetch_asset(&format!("http://{addr}/img"), "retry-later").await;
  assert_eq!(resp.status, 429);
  assert_eq!(hits.load(Ordering::SeqCst), 1);
}
