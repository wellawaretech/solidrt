// forge::isolate::Link: the port link semantics the flux Port class marshals.

use crate::isolate::{Kill, Link, Msg};
use crate::Value;

#[tokio::test]
async fn link_pair_delivers_in_order_and_closes() {
  let (a, b) = Link::pair();
  a.send(Msg::Value(Value::Int(1))).expect("send 1");
  a.send(Msg::Value(Value::Int(2))).expect("send 2");
  a.close();
  assert!(matches!(b.recv().await, Some(Msg::Value(Value::Int(1)))));
  assert!(matches!(b.recv().await, Some(Msg::Value(Value::Int(2)))));
  assert!(matches!(b.recv().await, None));
  assert!(matches!(b.recv().await, None), "closed stays closed");
  assert!(a.send(Msg::Value(Value::Null)).is_err(), "sending on a closed end fails");
  // The other direction is independent of a's close.
  b.send(Msg::Error("boom".into())).expect("send error");
  assert!(matches!(a.recv().await, Some(Msg::Error(e)) if e == "boom"));
}

#[tokio::test]
async fn send_to_a_dropped_peer_is_silent() {
  let (a, b) = Link::pair();
  drop(b);
  assert!(a.send(Msg::Value(Value::Null)).is_ok());
  assert!(matches!(a.recv().await, None));
}

#[tokio::test]
async fn kill_fires_once_before_or_after_await() {
  let kill = Kill::default();
  assert!(!kill.flag().load(std::sync::atomic::Ordering::Relaxed));
  kill.fire();
  assert!(kill.flag().load(std::sync::atomic::Ordering::Relaxed));
  kill.fired().await;
}
