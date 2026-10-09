use std::future::poll_fn;
use std::time::Duration;

use tokio::net::TcpListener;

use crate::net::*;
use crate::stream::ByteStream;

/// The next chunk of a read view: `Ok(Some(bytes))`, `Ok(None)` at the end,
/// `Err` for an errored (aborted) duplex.
async fn next_chunk(stream: &mut ByteStream) -> Result<Option<Vec<u8>>, String> {
  match poll_fn(|cx| stream.as_mut().poll_next(cx)).await {
    Some(Ok(chunk)) => Ok(Some(chunk.to_vec())),
    Some(Err(e)) => Err(e.to_string()),
    None => Ok(None),
  }
}

async fn soon<T>(fut: impl std::future::Future<Output = T>) -> T {
  tokio::time::timeout(Duration::from_secs(2), fut).await.expect("within the budget")
}

/// Probe budget wide enough for a loopback refusal on every platform: Windows
/// retries the SYN for ~2 s before reporting the RST, so anything tighter
/// reads a refused port as Filtered there.
const PROBE_BUDGET_MS: u64 = 3000;

#[tokio::test]
async fn probe_open_then_closed() {
  let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
  let port = listener.local_addr().unwrap().port();
  assert_eq!(probe("127.0.0.1", port, PROBE_BUDGET_MS).await, Liveness::Open);
  drop(listener);
  // Nothing listening on a loopback port -> the kernel refuses (RST) -> Closed,
  // which still means "host up". This is the distinction the sweep relies on.
  assert_eq!(probe("127.0.0.1", port, PROBE_BUDGET_MS).await, Liveness::Closed);
}

#[tokio::test]
async fn interfaces_include_loopback() {
  let ifaces = interfaces();
  assert!(ifaces.iter().any(|i| i.loopback), "expected a loopback interface");
  assert!(ifaces.iter().any(|i| i.addrs.iter().any(|a| a.ip == "127.0.0.1")), "expected 127.0.0.1 on some interface");
}

#[tokio::test]
async fn udp_loopback_roundtrip() {
  let rx = udp_bind(0, true).await.unwrap();
  let port = rx.local_addr().rsplit(':').next().unwrap().parse::<u16>().unwrap();
  let tx = udp_bind(0, true).await.unwrap();
  tx.send(b"ping", "127.0.0.1", port).await.unwrap();
  let (data, _ip, _port) =
    tokio::time::timeout(Duration::from_secs(2), rx.recv()).await.unwrap().unwrap().expect("datagram, not closed");
  assert_eq!(data.as_slice(), b"ping");
}

#[tokio::test]
async fn icmp_echo_loopback() {
  // Reply where unprivileged ICMP is permitted; Unsupported (not a hang or a
  // panic) where ping_group_range excludes us, so CI passes either way.
  let payload = b"solidrt-icmp-test".to_vec();
  match icmp_echo("127.0.0.1", payload.clone(), 2000).await {
    IcmpEcho::Reply { payload: echoed, rtt_ms } => {
      assert_eq!(echoed, payload, "reply echoes the request payload");
      assert!(rtt_ms >= 0.0);
    }
    IcmpEcho::Unsupported => {}
    IcmpEcho::Timeout => panic!("loopback ping neither replied nor reported unsupported"),
  }
}

#[tokio::test]
async fn udp_close_unblocks_pending_recv() {
  let udp = udp_bind(0, false).await.unwrap();
  let (recv, _) = tokio::time::timeout(Duration::from_secs(2), async {
    tokio::join!(udp.recv(), async {
      tokio::time::sleep(Duration::from_millis(50)).await;
      udp.close();
    })
  })
  .await
  .unwrap();
  assert!(matches!(recv, Ok(None)), "pending recv resolves end on close");
  assert!(matches!(udp.recv().await, Ok(None)), "recv after close is end, not a hang");
  assert!(udp.send(b"x", "127.0.0.1", 9).await.is_err(), "send after close errors");
  assert_eq!(udp.local_addr(), "");
}

#[tokio::test]
async fn listener_close_unblocks_pending_accept() {
  let listener = listen("127.0.0.1", 0).await.unwrap();
  let (accept, _) = tokio::time::timeout(Duration::from_secs(2), async {
    tokio::join!(listener.accept(), async {
      tokio::time::sleep(Duration::from_millis(50)).await;
      listener.close();
    })
  })
  .await
  .unwrap();
  assert!(matches!(accept, Ok(None)), "pending accept resolves end on close");
  assert!(matches!(listener.accept().await, Ok(None)), "accept after close is end, not a hang");
  assert_eq!(listener.local_addr(), "");
}

#[tokio::test]
async fn close_write_signals_eof_but_keeps_reading() {
  let listener = listen("127.0.0.1", 0).await.unwrap();
  let port = listener.local_addr().rsplit(':').next().unwrap().parse::<u16>().unwrap();
  let (client, server) = tokio::join!(connect("127.0.0.1", port, 1000), listener.accept());
  let client = client.unwrap();
  let server = server.unwrap().expect("accepted conn");
  let mut from_client = server.readable();
  let mut from_server = client.readable();

  // The netcat-style exchange: write the request, signal end-of-request with FIN.
  client.write(b"request".to_vec()).await.unwrap();
  client.close_write().await.unwrap();

  let chunk = soon(next_chunk(&mut from_client)).await.unwrap().expect("request bytes");
  assert_eq!(chunk.as_slice(), b"request");
  let eof = soon(next_chunk(&mut from_client)).await;
  assert!(matches!(eof, Ok(None)), "server sees EOF after the client's closeWrite");

  // The read side stayed open: the answer still comes through.
  server.write(b"response".to_vec()).await.unwrap();
  let resp = soon(next_chunk(&mut from_server)).await.unwrap().expect("response bytes");
  assert_eq!(resp.as_slice(), b"response");

  assert!(client.write(b"x".to_vec()).await.is_err(), "write after closeWrite errors");
  assert!(client.close_write().await.is_ok(), "closeWrite is idempotent");
}

#[tokio::test]
async fn conn_close_unblocks_pending_read_and_fins_peer() {
  let listener = listen("127.0.0.1", 0).await.unwrap();
  let port = listener.local_addr().rsplit(':').next().unwrap().parse::<u16>().unwrap();
  let (client, server) = tokio::join!(connect("127.0.0.1", port, 1000), listener.accept());
  let client = client.unwrap();
  let server = server.unwrap().expect("accepted conn");
  let mut from_server = client.readable();
  let mut from_client = server.readable();

  let (read, _) = soon(async {
    tokio::join!(next_chunk(&mut from_server), async {
      tokio::time::sleep(Duration::from_millis(50)).await;
      client.close();
    })
  })
  .await;
  assert!(matches!(read, Ok(None)), "pending read resolves end on close");

  // close() drops the write half, so the FIN reaches the peer now, not at GC.
  let peer_read = soon(next_chunk(&mut from_client)).await;
  assert!(matches!(peer_read, Ok(None)), "peer sees EOF right after close");
  assert!(client.write(b"x".to_vec()).await.is_err(), "write after close errors");
}

#[tokio::test]
async fn conn_abort_errors_the_pending_read_with_the_reason() {
  let listener = listen("127.0.0.1", 0).await.unwrap();
  let port = listener.local_addr().rsplit(':').next().unwrap().parse::<u16>().unwrap();
  let (client, server) = tokio::join!(connect("127.0.0.1", port, 1000), listener.accept());
  let client = client.unwrap();
  let _server = server.unwrap().expect("accepted conn");
  let mut from_server = client.readable();

  let (read, _) = soon(async {
    tokio::join!(next_chunk(&mut from_server), async {
      tokio::time::sleep(Duration::from_millis(50)).await;
      client.abort("upstream broke".to_string());
    })
  })
  .await;
  assert_eq!(read, Err("upstream broke".to_string()), "pending read reports the abort reason");
  assert!(matches!(next_chunk(&mut from_server).await, Ok(None)), "then the view is done");
  assert_eq!(client.write(b"x".to_vec()).await, Err("upstream broke".to_string()), "a write reports it too");
}

#[tokio::test]
async fn dropping_the_read_view_releases_the_half_and_keeps_writing() {
  let listener = listen("127.0.0.1", 0).await.unwrap();
  let port = listener.local_addr().rsplit(':').next().unwrap().parse::<u16>().unwrap();
  let (client, server) = tokio::join!(connect("127.0.0.1", port, 1000), listener.accept());
  let client = client.unwrap();
  let server = server.unwrap().expect("accepted conn");
  let mut from_client = server.readable();

  drop(client.readable());
  // The write half is untouched by the view going away.
  client.write(b"still writing".to_vec()).await.unwrap();
  let chunk = soon(next_chunk(&mut from_client)).await.unwrap().expect("bytes");
  assert_eq!(chunk.as_slice(), b"still writing");
  // A new view over the released half finds nothing to read: it ends at once.
  assert!(matches!(
    client.readable().as_mut().poll_next(&mut std::task::Context::from_waker(std::task::Waker::noop())),
    std::task::Poll::Ready(None)
  ));
}
