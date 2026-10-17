//! The self-hosted relay (`p2p::Relay`): an endpoint configured with the
//! relay's URL, token included, and no UDP socket of its own (relay-only)
//! reaches a peer through it on a ticket that names nothing but the relay;
//! one configured with the bare URL never does.
//! The connection is the proof: iroh picks the relay as its home from the
//! HTTP latency probe, which the relay answers for anyone, so a refused
//! endpoint's ticket still names the relay while its relay connection is
//! denied at the handshake.

use std::time::Duration;

use crate::p2p::{Endpoint, Relay};

const PROTOCOL: &str = "forge-test/relay";
// How long a dial through the relay may take on one machine.
const DIAL_WAIT: Duration = Duration::from_secs(10);
// How long a refused endpoint is given to prove it gets nowhere.
const REFUSED_WAIT: Duration = Duration::from_secs(3);

/// The ticket with its direct addresses cut: id and relay only.
fn relay_only(ticket: &str) -> String {
  let mut parts = ticket.split('|');
  format!("{}|{}|", parts.next().unwrap_or(""), parts.next().unwrap_or(""))
}

#[tokio::test]
async fn relay_admits_its_token_only() {
  let relay = Relay::serve(0).await.expect("the relay serves");
  let url = relay.url().to_string();
  assert!(url.starts_with("http://") && url.contains('@'), "the URL carries the token: {url}");
  let at = url.find('@').expect("token");
  let bare = format!("http://{}", &url[at + 1..]);

  let server = Endpoint::bind(None, Some(url.clone()), vec![PROTOCOL.as_bytes().to_vec()], None, false)
    .await
    .expect("the server binds");
  let ticket = relay_only(&server.ticket().await);
  assert!(
    ticket.split('|').nth(1).is_some_and(|relay| relay.contains('@')),
    "the ticket carries the relay with its token: {ticket}"
  );

  // With the token, and no UDP socket: the dial lands, through the relay.
  let insider = Endpoint::bind(None, Some(url.clone()), Vec::new(), None, true).await.expect("the insider binds");
  let accepting = server.clone();
  let accepted = tokio::spawn(async move { accepting.accept_one(PROTOCOL.as_bytes()).await });
  let dialed = tokio::time::timeout(DIAL_WAIT, insider.connect(ticket.clone(), PROTOCOL.to_string())).await;
  let stream = dialed.expect("the dial through the relay completes in time").expect("the dial succeeds");
  stream.write(b"hello".to_vec()).await.expect("a write over the relayed stream");
  let incoming =
    tokio::time::timeout(DIAL_WAIT, accepted).await.expect("accept in time").expect("accept task").expect("accept");
  assert!(incoming.is_some(), "the server accepted the relayed stream");

  // Without the token: the relay denies the endpoint at the handshake, so
  // a ticket naming only the relay leads nowhere.
  let outsider = Endpoint::bind(None, Some(bare), Vec::new(), None, false).await.expect("the outsider binds");
  let refused = tokio::time::timeout(REFUSED_WAIT, outsider.connect(ticket, PROTOCOL.to_string())).await;
  assert!(!matches!(refused, Ok(Ok(_))), "an endpoint without the token must not get through the relay");

  outsider.close().await;
  insider.close().await;
  server.close().await;
  relay.close().await;
}
