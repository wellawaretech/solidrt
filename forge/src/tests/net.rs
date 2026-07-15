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
  let (data, _ip, _port) = tokio::time::timeout(Duration::from_secs(2), rx.recv()).await.unwrap().unwrap();
  assert_eq!(data.as_slice(), b"ping");
}
