use std::time::Duration;

use tokio::net::TcpListener;

use crate::net::*;

#[tokio::test]
async fn probe_open_then_closed() {
  let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
  let port = listener.local_addr().unwrap().port();
  assert_eq!(probe("127.0.0.1", port, 1000).await, Liveness::Open);
  drop(listener);
  // Nothing listening on a loopback port -> the kernel refuses (RST) -> Closed,
  // which still means "host up". This is the distinction the sweep relies on.
  assert_eq!(probe("127.0.0.1", port, 1000).await, Liveness::Closed);
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
async fn conn_close_unblocks_pending_read_and_fins_peer() {
  let listener = listen("127.0.0.1", 0).await.unwrap();
  let port = listener.local_addr().rsplit(':').next().unwrap().parse::<u16>().unwrap();
  let (client, server) = tokio::join!(connect("127.0.0.1", port, 1000), listener.accept());
  let client = client.unwrap();
  let server = server.unwrap().expect("accepted conn");

  let (read, _) = tokio::time::timeout(Duration::from_secs(2), async {
    tokio::join!(client.read_chunk(), async {
      tokio::time::sleep(Duration::from_millis(50)).await;
      client.close();
    })
  })
  .await
  .unwrap();
  assert!(matches!(read, Ok(None)), "pending read resolves end on close");

  // close() drops the write half, so the FIN reaches the peer now, not at GC.
  let peer_read = tokio::time::timeout(Duration::from_secs(2), server.read_chunk()).await.unwrap();
  assert!(matches!(peer_read, Ok(None)), "peer sees EOF right after close");
  assert!(client.write(b"x".to_vec()).await.is_err(), "write after close errors");
}
