use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::Instant;

use crate::cache::Cache;
use crate::fetch::{cached_meta, do_fetch, do_fetch_cached, CacheMode, Client, HostLimits, RequestBody, ResponseData};
use crate::stream::ByteStream;

/// The permit (`None` for an HTTP/2 host) if `acquire` returns within `ms`.
async fn acquires_within(
  limits: &HostLimits,
  host: &str,
  ms: u64,
) -> Option<Option<tokio::sync::OwnedSemaphorePermit>> {
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

#[tokio::test]
async fn http2_host_skips_the_cap_but_not_the_cooldown() {
  let limits = HostLimits::new(1);
  let _p1 = limits.acquire("a:443").await;
  assert!(acquires_within(&limits, "a:443", 50).await.is_none());

  // A host recorded as HTTP/2 takes no slot, however many are in flight...
  limits.record("a:443", true);
  assert!(matches!(acquires_within(&limits, "a:443", 50).await, Some(None)));
  assert!(matches!(acquires_within(&limits, "a:443", 50).await, Some(None)));

  // ...but still waits out a cooldown...
  limits.cooldown("a:443", Duration::from_millis(300));
  assert!(acquires_within(&limits, "a:443", 50).await.is_none());
  assert!(acquires_within(&limits, "a:443", 1000).await.is_some());

  // ...and is capped again once a response comes back over HTTP/1.1.
  limits.record("a:443", false);
  assert!(acquires_within(&limits, "a:443", 50).await.is_none());
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
const TOO_MANY_SOON: &str =
  "HTTP/1.1 429 Too Many Requests\r\nretry-after: 1\r\ncontent-length: 0\r\nconnection: close\r\n\r\n";
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

fn test_client() -> Client {
  Client::new("forge-test").expect("build client")
}

async fn fetch_asset(url: &str, cache_dir: &str) -> ResponseData {
  let cache = Rc::new(Cache::new(temp_dir(cache_dir), 1024 * 1024));
  let limits = HostLimits::new(2);
  do_fetch_cached(&test_client(), &limits, "GET", url, vec![], None, cache, CacheMode::ForceCache).await.expect("fetch")
}

/// Read a body to its end.
async fn drain(mut body: ByteStream) {
  while std::future::poll_fn(|cx| body.as_mut().poll_next(cx)).await.is_some() {}
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

#[tokio::test]
async fn plain_get_retries_after_429() {
  let (addr, hits) = scripted_server(vec![TOO_MANY, OK]).await;
  let limits = HostLimits::new(2);
  let resp =
    do_fetch(&test_client(), &limits, "GET", &format!("http://{addr}/api"), vec![], None).await.expect("fetch");
  assert_eq!(resp.status, 200);
  assert_eq!(hits.load(Ordering::SeqCst), 2);
}

// --- the per-host cap on plain fetches ---

#[tokio::test]
async fn plain_fetches_beyond_the_cap_queue() {
  let (addr, hits) = scripted_server(vec![OK; 4]).await;
  let url = format!("http://{addr}/api");
  let client = test_client();
  let limits = HostLimits::new(2);
  let get = || do_fetch(&client, &limits, "GET", &url, vec![], None);

  // Two responses whose bodies are not read yet hold both slots...
  let first = get().await.expect("first fetch");
  let second = get().await.expect("second fetch");
  // ...so a third waits and never reaches the server.
  assert!(tokio::time::timeout(Duration::from_millis(200), get()).await.is_err());
  assert_eq!(hits.load(Ordering::SeqCst), 2);

  // A body read to its end frees its slot...
  drain(first.body).await;
  let third = tokio::time::timeout(Duration::from_secs(5), get()).await.expect("slot freed by body end");
  assert_eq!(third.expect("third fetch").status, 200);

  // ...and so does a body its consumer drops (the third's slot is taken).
  drop(second);
  let fourth = tokio::time::timeout(Duration::from_secs(5), get()).await.expect("slot freed by drop");
  assert_eq!(fourth.expect("fourth fetch").status, 200);
  assert_eq!(hits.load(Ordering::SeqCst), 4);
}

#[tokio::test]
async fn post_429_is_not_retried_and_cools_the_host() {
  let (addr, hits) = scripted_server(vec![TOO_MANY_SOON, OK, OK]).await;
  let client = test_client();
  let limits = HostLimits::new(2);

  let post = do_fetch(
    &client,
    &limits,
    "POST",
    &format!("http://{addr}/api"),
    vec![],
    Some(RequestBody::bytes(b"payload".to_vec())),
  )
  .await
  .expect("post");
  assert_eq!(post.status, 429);
  assert_eq!(hits.load(Ordering::SeqCst), 1);
  drop(post);

  // The cooldown holds back plain and cached fetches to that host alike.
  let start = Instant::now();
  let cache = Rc::new(Cache::new(temp_dir("cooldown"), 1024 * 1024));
  let plain_url = format!("http://{addr}/api");
  let cached_url = format!("http://{addr}/img");
  let (plain, cached) = tokio::join!(
    do_fetch(&client, &limits, "GET", &plain_url, vec![], None),
    do_fetch_cached(&client, &limits, "GET", &cached_url, vec![], None, cache, CacheMode::ForceCache),
  );
  assert_eq!(plain.expect("plain fetch").status, 200);
  assert_eq!(cached.expect("cached fetch").status, 200);
  // The Retry-After is whole seconds; allow for the time the POST took.
  assert!(start.elapsed() >= Duration::from_millis(900), "fetches started during the cooldown");
  assert_eq!(hits.load(Ordering::SeqCst), 3);
}

// --- TLS options and HTTP/2, against a local TLS server ---

use super::tls::{
  server_config_of, CA, CLIENT, CLIENT_KEY, CLIENT_PIN, SELF_SIGNED, SELF_SIGNED_KEY, SELF_SIGNED_PIN, SERVER,
  SERVER_KEY,
};
use crate::tls::{client_config, parse_pin, ClientAuth, ClientOptions, ServerConfig};

/// Serve `ok` over TLS on 127.0.0.1, one connection after another: HTTP/2
/// when the client agreed `h2` by ALPN, HTTP/1.1 otherwise.
async fn tls_server(config: Arc<ServerConfig>) -> String {
  use hyper_util::rt::{TokioExecutor, TokioIo};
  let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind test server");
  let addr = listener.local_addr().expect("test server addr");
  let acceptor = tokio_rustls::TlsAcceptor::from(config);
  tokio::spawn(async move {
    while let Ok((tcp, _)) = listener.accept().await {
      let acceptor = acceptor.clone();
      tokio::spawn(async move {
        let Ok(tls) = acceptor.accept(tcp).await else { return };
        let h2 = tls.get_ref().1.alpn_protocol() == Some(b"h2".as_slice());
        let io = TokioIo::new(tls);
        let service = hyper::service::service_fn(|_req| async {
          Ok::<_, std::convert::Infallible>(hyper::Response::new(http_body_util::Full::new(bytes::Bytes::from_static(
            b"ok",
          ))))
        });
        let _ = match h2 {
          true => hyper::server::conn::http2::Builder::new(TokioExecutor::new()).serve_connection(io, service).await,
          false => hyper::server::conn::http1::Builder::new().serve_connection(io, service).await,
        };
      });
    }
  });
  format!("https://{addr}/")
}

fn tls_client(opts: ClientOptions) -> Client {
  Client::with_tls("forge-test", &client_config(&opts).expect("client config")).expect("build client")
}

async fn get(client: &Client, limits: &HostLimits, url: &str) -> Result<ResponseData, String> {
  do_fetch(client, limits, "GET", url, vec![], None).await
}

fn ca_server() -> Arc<ServerConfig> {
  server_config_of(SERVER, SERVER_KEY, ClientAuth::None, &["h2", "http/1.1"])
}

#[tokio::test]
async fn a_client_with_ca_certs_trusts_a_private_ca() {
  let url = tls_server(ca_server()).await;
  let limits = HostLimits::new(2);
  let resp = get(&tls_client(ClientOptions { ca_certs: vec![CA.to_vec()], ..Default::default() }), &limits, &url)
    .await
    .expect("fetch");
  assert_eq!(resp.status, 200);
  let err = get(&test_client(), &limits, &url).await.err().expect("the default client rejects it");
  assert!(err.contains("invalid peer certificate"), "{err}");
}

#[tokio::test]
async fn mutual_tls_presents_the_client_certificate() {
  let url = tls_server(server_config_of(SERVER, SERVER_KEY, ClientAuth::Chain(vec![CA.to_vec()]), &[])).await;
  let limits = HostLimits::new(2);
  let with_cert = ClientOptions {
    ca_certs: vec![CA.to_vec()],
    cert: Some(CLIENT.to_vec()),
    key: Some(CLIENT_KEY.to_vec()),
    ..Default::default()
  };
  assert_eq!(get(&tls_client(with_cert), &limits, &url).await.expect("fetch").status, 200);
  let without = ClientOptions { ca_certs: vec![CA.to_vec()], ..Default::default() };
  assert!(get(&tls_client(without), &limits, &url).await.is_err(), "the server refuses a client without one");
}

#[tokio::test]
async fn a_pinned_client_trusts_the_key_and_rejects_another() {
  let url = tls_server(server_config_of(SELF_SIGNED, SELF_SIGNED_KEY, ClientAuth::None, &[])).await;
  let limits = HostLimits::new(2);
  let pinned = |pin: &str| tls_client(ClientOptions { pin: Some(parse_pin(pin).expect("pin")), ..Default::default() });
  assert_eq!(get(&pinned(SELF_SIGNED_PIN), &limits, &url).await.expect("fetch").status, 200);
  let err = get(&pinned(CLIENT_PIN), &limits, &url).await.err().expect("a pin mismatch fails");
  assert!(err.contains("does not match the pin"), "{err}");
}

#[tokio::test]
async fn an_http2_host_takes_no_slot() {
  let url = tls_server(ca_server()).await;
  let client = tls_client(ClientOptions { ca_certs: vec![CA.to_vec()], ..Default::default() });
  let limits = HostLimits::new(1);
  // The first response, its body unread, holds the one slot...
  let first = get(&client, &limits, &url).await.expect("first fetch");
  assert!(first.http2, "the server answered over HTTP/2");
  // ...and the host, now known to multiplex, takes the next request anyway.
  let second = tokio::time::timeout(Duration::from_secs(5), get(&client, &limits, &url)).await.expect("not capped");
  assert!(second.expect("second fetch").http2);
  drop(first);
}
