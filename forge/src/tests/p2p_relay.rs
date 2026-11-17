//! The self-hosted relay (`p2p::Relay`): an endpoint configured with the
//! relay's URL, token included, and no UDP socket of its own (relay-only)
//! reaches a peer through it on a ticket that names nothing but the relay;
//! one configured with the bare URL never does.
//! The connection is the proof: iroh picks the relay as its home from the
//! HTTP latency probe, which the relay answers for anyone, so a refused
//! endpoint's ticket still names the relay while its relay connection is
//! denied at the handshake.
//! A relay that never comes online is visible: `ticket` fails naming the
//! relay and the reason, and `relay_status` reports it not connected; an
//! adopted relay iroh made the home relay does not stand in for it, and
//! `ticket` without the relay is direct-only and does not wait for it.
//! An endpoint with no relay of its own adopts the one a ticket names, with
//! its token, and still names no relay in its own ticket and status; a
//! ticket naming a known relay with another token or pin is refused (the
//! configured relay takes another token), `adopt_relay: false` adopts
//! nothing (and a ticket left with no way to dial is refused before the
//! dial, naming the relay without its token), and `relays` lists what the
//! endpoint knows. A ticket naming a loopback relay carries the local
//! addresses too; one naming a relay other machines reach carries none.
//! Tests that prove a path through a loopback relay cut its ticket to
//! `id|relay|`, so the relay is the only way to the peer.
//! The relay options: the URL host of an unspecified bind (the default
//! route's IPv4, else the first non-loopback one), a loopback host, a second token, an allowlist and its
//! replacement (which drops a live client) over HTTP and HTTPS, the connected
//! clients, the URL override (keeping the pin of an HTTPS relay), the
//! validation errors, and an HTTPS relay's port and URL (its pin by default
//! for a self-signed leaf only) with a handshake by pin, and its renewal in
//! place (a new key on an unpinned relay, the same key only on a pinned one,
//! the URL kept).
//! Endpoint trust: an HTTPS relay reached by the pin in its URL (through an
//! adopted ticket relay too), refused on a wrong pin, a CA-signed one
//! reached through `relayCaCerts` only, a pinned relay and `relayCaCerts`
//! on one endpoint, a second pin for one host refused, and the pin in a
//! relay URL parsed, validated and kept.
//! Switching the relay (`set_relay_url`): the ticket, the status, iroh's
//! home relay and `relays` move to the new relay, a peer on it dials the
//! new ticket, and a connection through the old relay keeps carrying data;
//! a known relay (adopted, or replaced before) keeps its token, so another
//! one is refused; a new relay that never answers leaves the old one home,
//! which the status and the ticket report; a second check for one host is
//! refused and changes nothing.

use std::time::Duration;

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use crate::p2p::relay::relay_host_ipv4;
use crate::p2p::{
  parse_ticket, ConnectOptions, Endpoint, Relay, RelayOptions, RelayTls, TicketAddresses, TicketOptions,
};
use crate::stream::ByteStream;
use crate::tls::{generate_certificate, CertificateRequest, GeneratedCertificate, DEFAULT_CERT_VALID_DAYS};

const PROTOCOL: &str = "forge-test/relay";
// How long a dial through the relay may take on one machine.
const DIAL_WAIT: Duration = Duration::from_secs(10);
// How long a refused endpoint is given to prove it gets nowhere.
const REFUSED_WAIT: Duration = Duration::from_secs(3);
// How long the test watches for iroh making an adopted relay the home relay.
const HOME_WAIT: Duration = Duration::from_secs(2);
// How often it looks.
const HOME_POLL: Duration = Duration::from_millis(50);
// How long the relay may take to register or drop a client, or an
// endpoint to report a denial.
const CLIENTS_WAIT: Duration = Duration::from_secs(5);
// How long a dial that cannot land is given before it is dropped: the relay
// adoption before it is all a test needs from it.
const ADOPT_WAIT: Duration = Duration::from_millis(500);
// Tokens long enough for the relay's minimum.
const TOKEN_A: &str = "first-token-0123456789";
const TOKEN_B: &str = "second-token-0123456789";

#[tokio::test]
async fn relay_admits_its_token_only() {
  let relay = Relay::serve(RelayOptions::default()).await.expect("the relay serves");
  let url = relay.url().to_string();
  assert!(url.starts_with("http://") && url.contains('@'), "the URL carries the token: {url}");
  let at = url.find('@').expect("token");
  let bare = format!("http://{}", &url[at + 1..]);

  let server = Endpoint::bind(None, Some(url.clone()), vec![PROTOCOL.as_bytes().to_vec()], None, false, Vec::new())
    .await
    .expect("the server binds");
  let ticket = server.ticket(TicketOptions::default()).await.expect("the server's relay comes online");
  let status = server.relay_status().expect("a configured relay has a status");
  assert!(status.connected && status.error.is_none(), "the server is on its relay");
  assert!(!status.url.contains('@'), "the status names the relay without its token: {}", status.url);
  assert!(
    ticket.split('|').nth(1).is_some_and(|relay| relay.contains('@')),
    "the ticket carries the relay with its token: {ticket}"
  );

  // With the token, and no UDP socket: the dial lands, through the relay.
  let insider =
    Endpoint::bind(None, Some(url.clone()), Vec::new(), None, true, Vec::new()).await.expect("the insider binds");
  let accepting = server.clone();
  let accepted = tokio::spawn(async move { accepting.accept_one(PROTOCOL.as_bytes()).await });
  let dialed =
    tokio::time::timeout(DIAL_WAIT, insider.connect(ticket.clone(), PROTOCOL.to_string(), ConnectOptions::default()))
      .await;
  let stream = dialed.expect("the dial through the relay completes in time").expect("the dial succeeds");
  stream.write(b"hello".to_vec()).await.expect("a write over the relayed stream");
  let incoming =
    tokio::time::timeout(DIAL_WAIT, accepted).await.expect("accept in time").expect("accept task").expect("accept");
  assert!(incoming.is_some(), "the server accepted the relayed stream");

  // Without the token: the relay denies the endpoint at the handshake, so
  // a ticket naming only the relay leads nowhere.
  let outsider =
    Endpoint::bind(None, Some(bare), Vec::new(), None, false, Vec::new()).await.expect("the outsider binds");
  let refused =
    tokio::time::timeout(REFUSED_WAIT, outsider.connect(ticket, PROTOCOL.to_string(), ConnectOptions::default())).await;
  assert!(!matches!(refused, Ok(Ok(_))), "an endpoint without the token must not get through the relay");
  let denial = wait_status_error(&outsider, "admits its own tokens only").await;
  assert!(denial.contains("Authentication denied"), "the outsider's status names the denial: {denial}");

  outsider.close().await;
  insider.close().await;
  server.close().await;
  relay.close().await;
}

/// `ticket` on an endpoint whose relay is not online fails, and the error
/// names the relay (without its token) and the reason.
async fn assert_ticket_fails(endpoint: &Endpoint, relay: &str, reason: &str) {
  let error = endpoint.ticket(TicketOptions::default()).await.expect_err("no ticket without the relay online");
  assert!(error.contains(relay) && error.contains(reason), "the error names the relay and the reason: {error}");
  assert!(!error.contains('@'), "the error carries no token: {error}");
  let status = endpoint.relay_status().expect("a configured relay has a status");
  assert!(!status.connected, "the relay is not connected");
  assert_eq!(status.url, relay);
}

#[tokio::test]
async fn ticket_fails_on_a_refused_token() {
  let relay = Relay::serve(RelayOptions::default()).await.expect("the relay serves");
  let url = relay.url().to_string();
  let at = url.find('@').expect("token");
  let bare = format!("http://{}", &url[at + 1..]);
  let wrong = format!("http://wrong-token@{}", &url[at + 1..]);
  let endpoint =
    Endpoint::bind(None, Some(wrong), Vec::new(), None, false, Vec::new()).await.expect("the endpoint binds");
  assert_ticket_fails(&endpoint, &bare, "Authentication denied").await;
  let status = endpoint.relay_status().expect("a configured relay has a status");
  assert!(status.error.is_some(), "the status carries the denial");
  endpoint.close().await;
  relay.close().await;
}

/// The error text of `endpoint`'s relay status once it contains `text`,
/// or whatever it is after `CLIENTS_WAIT`.
async fn wait_status_error(endpoint: &Endpoint, text: &str) -> String {
  let deadline = tokio::time::Instant::now() + CLIENTS_WAIT;
  loop {
    let error = endpoint.relay_status().and_then(|status| status.error).unwrap_or_default();
    if error.contains(text) || tokio::time::Instant::now() >= deadline {
      return error;
    }
    tokio::time::sleep(HOME_POLL).await;
  }
}

/// A relay URL nobody serves: a port that was free a moment ago. Another
/// process may take the port in between; on a test machine that is rare
/// enough, and no port is reserved for nothing to answer on.
fn dead_relay_url() -> (String, String) {
  let port = std::net::TcpListener::bind("127.0.0.1:0").expect("a free port").local_addr().expect("its address").port();
  (format!("http://token-0123456789abcdef@127.0.0.1:{port}/"), format!("http://127.0.0.1:{port}/"))
}

#[tokio::test]
async fn ticket_fails_on_a_relay_nobody_serves() {
  let (url, bare) = dead_relay_url();
  let endpoint =
    Endpoint::bind(None, Some(url), Vec::new(), None, false, Vec::new()).await.expect("the endpoint binds");
  assert_ticket_fails(&endpoint, &bare, crate::p2p::RELAY_NOT_REACHED).await;
  endpoint.close().await;
}

#[tokio::test]
async fn ticket_without_the_relay_is_direct_only() {
  let (url, _) = dead_relay_url();
  let endpoint =
    Endpoint::bind(None, Some(url), Vec::new(), None, false, Vec::new()).await.expect("the endpoint binds");
  let started = tokio::time::Instant::now();
  let ticket = endpoint
    .ticket(TicketOptions { relay: false, ..TicketOptions::default() })
    .await
    .expect("a ticket without the relay");
  assert!(started.elapsed() < crate::p2p::TICKET_WAIT, "no wait for the relay");
  let parts = parse_ticket(&ticket).expect("a valid ticket");
  assert!(parts.relay_url.is_none(), "no relay in the ticket: {ticket}");
  assert!(!parts.addresses.is_empty(), "the direct addresses: {ticket}");
  let ipv4 = |a: &String| a.parse::<std::net::SocketAddr>().is_ok_and(|a| a.is_ipv4());
  assert!(parts.addresses.iter().all(ipv4), "IPv4 only: {ticket}");
  endpoint.close().await;
}

// The bug this guards: iroh's `online` resolves for any home relay, so an
// endpoint whose own relay is dead but whose adopted relay became home got
// a ticket naming the dead relay.
#[tokio::test]
async fn ticket_fails_when_an_adopted_relay_is_home_instead() {
  let relay = Relay::serve(RelayOptions::default()).await.expect("the relay serves");
  let alice =
    Endpoint::bind(None, Some(relay.url().to_string()), vec![PROTOCOL.as_bytes().to_vec()], None, false, Vec::new())
      .await
      .expect("alice binds");
  let ticket = alice.ticket(TicketOptions::default()).await.expect("alice's relay comes online");
  let (dead, dead_bare) = dead_relay_url();
  let bob = Endpoint::bind(None, Some(dead), Vec::new(), None, false, Vec::new()).await.expect("bob binds");
  let accepting = alice.clone();
  let accepted = tokio::spawn(async move { accepting.accept_one(PROTOCOL.as_bytes()).await });
  let dialed = tokio::time::timeout(DIAL_WAIT, bob.connect(ticket, PROTOCOL.to_string(), ConnectOptions::default()))
    .await
    .expect("the dial completes in time")
    .expect("the dial through the adopted relay succeeds");
  dialed.write(b"hello".to_vec()).await.expect("a write over the stream");
  let _ = tokio::time::timeout(DIAL_WAIT, accepted).await;

  let adopted = relay.url().split('@').nth(1).map(|rest| format!("http://{rest}")).expect("a token in the URL");
  let deadline = tokio::time::Instant::now() + DIAL_WAIT;
  while !bob.home_relays().contains(&adopted) && tokio::time::Instant::now() < deadline {
    tokio::time::sleep(HOME_POLL).await;
  }
  assert!(bob.home_relays().contains(&adopted), "iroh made the adopted relay home: {:?}", bob.home_relays());
  let reason = format!("iroh made {adopted} the home relay");
  assert_ticket_fails(&bob, &dead_bare, &reason).await;
  let error = bob.relay_status().expect("a configured relay has a status").error;
  assert_eq!(error.as_deref(), Some(reason.as_str()), "the status names the other home relay");
  let known = bob.relays();
  assert!(known.len() == 2 && known[0].configured && !known[0].home, "the dead configured relay is not home");
  assert!(known[1].url == adopted && known[1].home, "the adopted relay is home");

  bob.close().await;
  alice.close().await;
  relay.close().await;
}

// A relay ticket names no direct address: the peers meet on the relay, then
// exchange their addresses inside the connection and move to a direct path.
#[tokio::test]
async fn a_relay_ticket_carries_no_addresses_and_goes_direct() {
  let relay = Relay::serve(RelayOptions::default()).await.expect("the relay serves");
  let url = relay.url().to_string();
  let alice = Endpoint::bind(None, Some(url.clone()), vec![PROTOCOL.as_bytes().to_vec()], None, false, Vec::new())
    .await
    .expect("alice binds");
  let ticket = alice.ticket(TicketOptions::default()).await.expect("alice's relay comes online");
  let parts = parse_ticket(&ticket).expect("a valid ticket");
  assert_eq!(parts.relay_url.as_deref(), Some(url.as_str()), "the ticket names the relay: {ticket}");
  assert!(parts.addresses.is_empty(), "no direct address beside the relay: {ticket}");
  let all = TicketOptions { addresses: TicketAddresses::All, ..TicketOptions::default() };
  let full = parse_ticket(&alice.ticket(all).await.expect("every address")).expect("a valid ticket");
  assert!(!full.addresses.is_empty(), "addresses: all writes the direct addresses beside the relay");

  let bob = Endpoint::bind(None, Some(url), Vec::new(), None, false, Vec::new()).await.expect("bob binds");
  let accepting = alice.clone();
  let accepted = tokio::spawn(async move { accepting.accept_one(PROTOCOL.as_bytes()).await });
  let stream = tokio::time::timeout(DIAL_WAIT, bob.connect(ticket, PROTOCOL.to_string(), ConnectOptions::default()))
    .await
    .expect("the dial completes in time")
    .expect("the dial through the relay succeeds");
  stream.write(b"hello".to_vec()).await.expect("a write over the stream");
  let incoming =
    tokio::time::timeout(DIAL_WAIT, accepted).await.expect("accept in time").expect("accept task").expect("accept");
  assert!(incoming.is_some(), "alice accepted the stream bob dialed from the relay ticket");

  let deadline = tokio::time::Instant::now() + DIAL_WAIT;
  let mut path = bob.conn_info(alice.id()).await.expect("a path").path;
  while path != "direct" && path != "mixed" && tokio::time::Instant::now() < deadline {
    tokio::time::sleep(HOME_POLL).await;
    path = bob.conn_info(alice.id()).await.expect("a path").path;
  }
  assert!(path == "direct" || path == "mixed", "the connection moved to a direct path: {path}");

  drop(stream);
  bob.close().await;
  alice.close().await;
  relay.close().await;
}

#[tokio::test]
async fn connect_adopts_the_ticket_relay() {
  let relay = Relay::serve(RelayOptions::default()).await.expect("the relay serves");
  let url = relay.url().to_string();
  let alice = Endpoint::bind(None, Some(url.clone()), vec![PROTOCOL.as_bytes().to_vec()], None, false, Vec::new())
    .await
    .expect("alice binds");
  // A relay ticket carries no direct addresses: the relay is the way to Alice.
  let ticket = alice.ticket(TicketOptions::default()).await.expect("alice's relay comes online");
  let bob = Endpoint::bind(None, None, Vec::new(), None, false, Vec::new()).await.expect("bob binds");
  assert!(bob.relay_status().is_none(), "bob has no relay of his own");

  let accepting = alice.clone();
  let accepted = tokio::spawn(async move { accepting.accept_one(PROTOCOL.as_bytes()).await });
  let dialed =
    tokio::time::timeout(DIAL_WAIT, bob.connect(ticket.clone(), PROTOCOL.to_string(), ConnectOptions::default())).await;
  let stream = dialed.expect("the dial through the ticket's relay completes in time").expect("the dial succeeds");
  stream.write(b"hello".to_vec()).await.expect("a write over the stream");
  let incoming =
    tokio::time::timeout(DIAL_WAIT, accepted).await.expect("accept in time").expect("accept task").expect("accept");
  assert!(incoming.is_some(), "alice accepted the stream bob dialed from the ticket alone");

  // iroh may make the adopted relay Bob's home relay; his surface still
  // names no relay.
  let deadline = tokio::time::Instant::now() + HOME_WAIT;
  while bob.home_relays().is_empty() && tokio::time::Instant::now() < deadline {
    tokio::time::sleep(HOME_POLL).await;
  }
  assert!(!bob.home_relays().is_empty(), "iroh made the adopted relay bob's home relay");
  let own = bob.ticket(TicketOptions::default()).await.expect("bob's ticket");
  assert_eq!(own.split('|').nth(1), Some(""), "bob's ticket names no relay: {own}");
  assert!(bob.relay_status().is_none(), "bob's status names no relay");

  bob.close().await;
  alice.close().await;
  relay.close().await;
}

#[test]
fn parse_ticket_keeps_the_token() {
  let id = "116072673e30625053b0a67b3b8abf3de89f0a7aa99737b259f802423a1cf9ab";
  let relay = "http://secret@127.0.0.1:3340/";
  let ticket = parse_ticket(&format!("{id}|{relay}|127.0.0.1:4000,10.0.0.2:4001")).expect("a valid ticket");
  assert_eq!(ticket.id, id);
  assert_eq!(ticket.relay_url.as_deref(), Some(relay));
  assert_eq!(ticket.addresses, vec!["127.0.0.1:4000".to_string(), "10.0.0.2:4001".to_string()]);
  let bare = parse_ticket(id).expect("a bare id");
  assert!(bare.relay_url.is_none() && bare.addresses.is_empty(), "a bare id has no relay and no addresses");
  assert!(parse_ticket("not-an-id||").is_err(), "an invalid id is an error");
}

/// `url` with its user part replaced by `token`.
fn with_token(url: &str, token: &str) -> String {
  let mut url = url::Url::parse(url).expect("a relay URL");
  url.set_username(token).expect("the URL takes a token");
  url.to_string()
}

/// A fresh endpoint key and its id.
fn identity() -> ([u8; 32], String) {
  let key = iroh::SecretKey::generate();
  (key.to_bytes(), key.public().to_string())
}

/// Wait until `relay.clients()` is `want`, or `CLIENTS_WAIT` passes.
async fn wait_clients(relay: &Relay, want: &[String]) -> Vec<String> {
  let deadline = tokio::time::Instant::now() + CLIENTS_WAIT;
  loop {
    let clients = relay.clients();
    if clients == want || tokio::time::Instant::now() >= deadline {
      return clients;
    }
    tokio::time::sleep(HOME_POLL).await;
  }
}

#[tokio::test]
async fn relay_on_loopback_admits_its_second_token() {
  let opts = RelayOptions {
    host: Some("127.0.0.1".to_string()),
    tokens: Some(vec![TOKEN_A.to_string(), TOKEN_B.to_string()]),
    ..RelayOptions::default()
  };
  let relay = Relay::serve(opts).await.expect("the relay serves");
  let url = relay.url().to_string();
  assert_eq!(url, format!("http://{TOKEN_A}@127.0.0.1:{}/", relay.port()), "the first token, the bound host");
  let second = with_token(&url, TOKEN_B);
  let endpoint =
    Endpoint::bind(None, Some(second), Vec::new(), None, false, Vec::new()).await.expect("the endpoint binds");
  endpoint.ticket(TicketOptions::default()).await.expect("the second token is admitted");
  assert_eq!(wait_clients(&relay, &[endpoint.id()]).await, vec![endpoint.id()], "the relay lists the client");
  endpoint.close().await;
  assert!(wait_clients(&relay, &[]).await.is_empty(), "a closed endpoint leaves the list");
  relay.close().await;
}

/// The allowlist over a relay served on plain HTTP, or on HTTPS with `tls`
/// (a self-signed leaf for 127.0.0.1, reached by the pin in the URL): an
/// allowed id is admitted, a denied one fails its ticket with the denial in
/// its status, and replacing the list drops the live client.
async fn allowlist_admits_and_set_allow_drops(tls: Option<&GeneratedCertificate>) {
  let (alice_key, alice_id) = identity();
  let (bob_key, bob_id) = identity();
  let opts = RelayOptions {
    host: Some("127.0.0.1".to_string()),
    allow: Some(vec![alice_id.clone()]),
    tls: tls.map(|cert| RelayTls {
      cert: cert.cert.clone().into_bytes(),
      key: cert.key.clone().into_bytes(),
      pinned: None,
    }),
    ..RelayOptions::default()
  };
  let relay = Relay::serve(opts).await.expect("the relay serves");
  let url = relay.url().to_string();
  let scheme = if tls.is_some() { "https" } else { "http" };
  let bare = format!("{scheme}://127.0.0.1:{}/", relay.port());

  let alice =
    Endpoint::bind(Some(alice_key), Some(url.clone()), Vec::new(), None, false, Vec::new()).await.expect("alice binds");
  alice.ticket(TicketOptions::default()).await.expect("an allowed id is admitted");
  let bob =
    Endpoint::bind(Some(bob_key), Some(url.clone()), Vec::new(), None, false, Vec::new()).await.expect("bob binds");
  assert_ticket_fails(&bob, &bare, "does not admit this endpoint").await;
  let denial = bob.relay_status().expect("a status").error.expect("the status carries the denial");
  assert!(denial.contains("does not admit this endpoint"), "the status names the denial: {denial}");
  assert_eq!(wait_clients(&relay, std::slice::from_ref(&alice_id)).await, vec![alice_id.clone()]);

  // A list that keeps Alice keeps her connected.
  relay.set_allow(Some(vec![alice_id.clone(), bob_id.clone()])).expect("valid ids");
  assert_eq!(relay.clients(), vec![alice_id.clone()], "a kept client stays connected");
  assert!(alice.relay_status().expect("a status").connected, "alice stays on the relay");

  // Revoking Alice (to a list that admits neither, so Bob's retries cannot
  // take her place) drops her live connection, and her reconnect is denied.
  relay.set_allow(Some(Vec::new())).expect("valid ids");
  assert!(wait_clients(&relay, &[]).await.is_empty(), "the revoked client is disconnected");
  let denial = wait_status_error(&alice, "does not admit this endpoint").await;
  assert!(denial.contains("does not admit this endpoint"), "alice's status names the denial: {denial}");
  assert!(!alice.relay_status().expect("a status").connected, "alice is off the relay");

  assert!(relay.set_allow(Some(vec!["not-an-id".to_string()])).is_err(), "a malformed id is an error");
  relay.set_allow(None).expect("no allowlist");
  bob.close().await;
  alice.close().await;
  relay.close().await;
}

#[tokio::test]
async fn relay_admits_allowed_ids_only_and_set_allow_drops_a_client() {
  allowlist_admits_and_set_allow_drops(None).await;
}

#[tokio::test]
async fn tls_relay_admits_allowed_ids_only_and_set_allow_drops_a_client() {
  allowlist_admits_and_set_allow_drops(Some(&certificate("127.0.0.1", false, None))).await;
}

#[tokio::test]
async fn relay_url_override_carries_the_token() {
  let opts = RelayOptions {
    tokens: Some(vec![TOKEN_A.to_string()]),
    url: Some("https://relay.example.test:8443/".to_string()),
    ..RelayOptions::default()
  };
  let relay = Relay::serve(opts).await.expect("the relay serves");
  assert_eq!(relay.url(), format!("https://{TOKEN_A}@relay.example.test:8443/"));
  relay.close().await;

  // Over HTTPS the override keeps the pin of a self-signed leaf.
  let leaf = certificate("127.0.0.1", false, None);
  let tls = RelayTls { cert: leaf.cert.clone().into_bytes(), key: leaf.key.clone().into_bytes(), pinned: None };
  let opts = RelayOptions {
    host: Some("127.0.0.1".to_string()),
    tokens: Some(vec![TOKEN_A.to_string()]),
    url: Some("https://relay.example.test:8443/".to_string()),
    tls: Some(tls),
    ..RelayOptions::default()
  };
  let pinned = Relay::serve(opts).await.expect("the HTTPS relay serves");
  assert_eq!(pinned.url(), format!("https://{TOKEN_A}@relay.example.test:8443/?pin={}", leaf.pin));
  pinned.close().await;
}

#[test]
fn relay_options_are_validated() {
  let bad = |opts: RelayOptions, what: &str| {
    let error = opts.check().err().unwrap_or_else(|| panic!("{what} is an error"));
    assert!(error.starts_with(what), "the error names {what}: {error}");
  };
  bad(RelayOptions { host: Some("not a host".to_string()), ..RelayOptions::default() }, "host");
  bad(RelayOptions { tokens: Some(Vec::new()), ..RelayOptions::default() }, "token");
  bad(RelayOptions { tokens: Some(vec!["short".to_string()]), ..RelayOptions::default() }, "token");
  bad(RelayOptions { tokens: Some(vec!["not/url%safe-0123456789".to_string()]), ..RelayOptions::default() }, "token");
  bad(RelayOptions { allow: Some(vec!["not-an-id".to_string()]), ..RelayOptions::default() }, "allow");
  for url in ["ftp://host/", "not a url", "http://user@host/", "http://host/?pin=00", "http://host/#x"] {
    bad(RelayOptions { url: Some(url.to_string()), ..RelayOptions::default() }, "url");
  }
  let error = RelayOptions { tokens: Some(vec!["secret/0123456789abcdef".to_string()]), ..RelayOptions::default() }
    .check()
    .err()
    .expect("a token that is not URL-safe");
  assert!(!error.contains("secret"), "the error does not echo the token: {error}");
  let error = RelayOptions { url: Some("https://secret@host/".to_string()), ..RelayOptions::default() }
    .check()
    .err()
    .expect("a url with a user part");
  assert!(!error.contains("secret"), "the error does not echo the user part: {error}");
  // A pin needs https, and an HTTPS relay URL cannot carry an IPv6 address.
  let leaf = certificate("127.0.0.1", false, None);
  let tls =
    || Some(RelayTls { cert: leaf.cert.clone().into_bytes(), key: leaf.key.clone().into_bytes(), pinned: None });
  bad(
    RelayOptions { url: Some("http://relay.example.test/".to_string()), tls: tls(), ..RelayOptions::default() },
    "url",
  );
  bad(RelayOptions { host: Some("::1".to_string()), tls: tls(), ..RelayOptions::default() }, "host");
  bad(RelayOptions { url: Some("https://[::1]:8443/".to_string()), ..RelayOptions::default() }, "url");
  // TLS with an http:// url, pinned or not: clients would dial without TLS.
  let unpinned =
    RelayTls { cert: leaf.cert.clone().into_bytes(), key: leaf.key.clone().into_bytes(), pinned: Some(false) };
  bad(
    RelayOptions {
      url: Some("http://relay.example.test/".to_string()),
      tls: Some(unpinned),
      ..RelayOptions::default()
    },
    "url",
  );
  bad(RelayOptions { url: Some("https://relay.example.test/a|b".to_string()), ..RelayOptions::default() }, "url");
  assert!(
    RelayOptions { host: Some("::1".to_string()), ..RelayOptions::default() }.check().is_ok(),
    "plain HTTP on IPv6"
  );
  let tls = RelayTls { cert: b"not pem".to_vec(), key: b"not pem".to_vec(), pinned: None };
  bad(RelayOptions { tls: Some(tls), ..RelayOptions::default() }, "tls");
}

/// A certificate for `name`. A CA takes a name of its own: a leaf whose
/// issuer equals its subject reads as self-signed.
fn certificate(name: &str, is_ca: bool, issuer: Option<&GeneratedCertificate>) -> GeneratedCertificate {
  let names = [name.to_string()];
  generate_certificate(&CertificateRequest {
    names: &names,
    valid_days: DEFAULT_CERT_VALID_DAYS,
    is_ca,
    issuer: issuer.map(|ca| (ca.cert.as_bytes(), ca.key.as_bytes())),
  })
  .expect("a certificate")
}

async fn tls_relay(cert: &GeneratedCertificate, pinned: Option<bool>) -> Relay {
  let tls = RelayTls { cert: cert.cert.clone().into_bytes(), key: cert.key.clone().into_bytes(), pinned };
  let opts = RelayOptions { host: Some("127.0.0.1".to_string()), tls: Some(tls), ..RelayOptions::default() };
  Relay::serve(opts).await.expect("the HTTPS relay serves")
}

/// A TLS handshake with the relay's port, trusting `pin` alone.
async fn handshake(port: u16, pin: &str) -> Result<(), String> {
  let opts = crate::tls::ClientOptions { pin: Some(crate::tls::parse_pin(pin)?), ..Default::default() };
  let connector = tokio_rustls::TlsConnector::from(crate::tls::client_config(&opts)?);
  let tcp = tokio::net::TcpStream::connect(("127.0.0.1", port)).await.map_err(|e| e.to_string())?;
  let name = crate::tls::server_name("127.0.0.1")?;
  tokio::time::timeout(DIAL_WAIT, connector.connect(name, tcp))
    .await
    .map_err(|_| "handshake timed out".to_string())?
    .map(|_| ())
    .map_err(|e| e.to_string())
}

#[tokio::test]
async fn tls_relay_pins_a_self_signed_leaf() {
  let leaf = certificate("127.0.0.1", false, None);
  let relay = tls_relay(&leaf, None).await;
  let url = relay.url().to_string();
  assert!(url.starts_with("https://") && url.contains('@'), "an HTTPS URL with the token: {url}");
  assert!(url.contains(&format!(":{}/", relay.port())), "the HTTPS port: {url}");
  assert!(url.ends_with(&format!("/?pin={}", leaf.pin)), "a self-signed leaf is pinned: {url}");
  handshake(relay.port(), &leaf.pin).await.expect("the relay's port speaks TLS with the pinned key");
  let other = certificate("127.0.0.1", false, None);
  assert!(handshake(relay.port(), &other.pin).await.is_err(), "another pin is refused");
  relay.close().await;

  let unpinned = tls_relay(&leaf, Some(false)).await;
  assert!(!unpinned.url().contains("pin="), "pinned: false leaves the pin out: {}", unpinned.url());
  unpinned.close().await;
}

#[tokio::test]
async fn tls_relay_leaves_a_ca_signed_leaf_unpinned() {
  let ca = certificate("relay-ca.test", true, None);
  let leaf = certificate("127.0.0.1", false, Some(&ca));
  let relay = tls_relay(&leaf, None).await;
  assert!(relay.url().starts_with("https://"), "an HTTPS URL: {}", relay.url());
  assert!(!relay.url().contains("pin="), "a CA-signed leaf is not pinned: {}", relay.url());
  relay.close().await;

  let pinned = tls_relay(&leaf, Some(true)).await;
  assert!(pinned.url().ends_with(&format!("/?pin={}", leaf.pin)), "pinned: true pins it: {}", pinned.url());
  pinned.close().await;
}

// The certificate renews in place: iroh-relay asks the resolver at every
// handshake, so the running relay presents the new pair, and the URL stays.
#[tokio::test]
async fn an_unpinned_relay_renews_its_certificate() {
  let ca = certificate("relay-ca.test", true, None);
  let first = certificate("127.0.0.1", false, Some(&ca));
  let second = certificate("127.0.0.1", false, Some(&ca));
  let relay = tls_relay(&first, None).await;
  let url = relay.url().to_string();
  handshake(relay.port(), &first.pin).await.expect("the first key");
  relay.set_certificate(second.cert.as_bytes(), second.key.as_bytes()).expect("an unpinned relay takes a new key");
  handshake(relay.port(), &second.pin).await.expect("the new key");
  assert!(handshake(relay.port(), &first.pin).await.is_err(), "the first key is gone");
  assert_eq!(relay.url(), url, "the URL stays");
  let endpoint = Endpoint::bind(None, Some(url), Vec::new(), None, false, vec![ca.cert.clone().into_bytes()])
    .await
    .expect("the endpoint binds");
  endpoint.ticket(TicketOptions::default()).await.expect("the renewed relay is reached through relayCaCerts");
  endpoint.close().await;
  relay.close().await;
}

#[tokio::test]
async fn a_pinned_relay_keeps_its_key() {
  let leaf = certificate("127.0.0.1", false, None);
  let relay = tls_relay(&leaf, None).await;
  let url = relay.url().to_string();
  let other = certificate("127.0.0.1", false, None);
  let err = relay.set_certificate(other.cert.as_bytes(), other.key.as_bytes()).err().expect("another key");
  assert!(err.contains("a pinned relay keeps its key"), "{err}");
  let err = relay.set_certificate(leaf.cert.as_bytes(), other.key.as_bytes()).err().expect("a mismatch");
  assert_eq!(err, "key: does not belong to cert");
  relay.set_certificate(leaf.cert.as_bytes(), leaf.key.as_bytes()).expect("the same key");
  handshake(relay.port(), &leaf.pin).await.expect("the pinned key still serves");
  assert_eq!(relay.url(), url, "the URL stays");
  relay.close().await;

  let plain = Relay::serve(RelayOptions { host: Some("127.0.0.1".to_string()), ..RelayOptions::default() })
    .await
    .expect("the relay serves");
  let err = plain.set_certificate(leaf.cert.as_bytes(), leaf.key.as_bytes()).err().expect("a plain relay");
  assert!(err.contains("no certificate"), "{err}");
  plain.close().await;
}

/// `url` with its `?pin=` replaced by `pin`.
fn with_pin(url: &str, pin: &str) -> String {
  let mut url = url::Url::parse(url).expect("a relay URL");
  url.set_query(Some(&format!("pin={pin}")));
  url.to_string()
}

#[tokio::test]
async fn endpoint_reaches_a_self_signed_relay_by_its_pin() {
  let leaf = certificate("127.0.0.1", false, None);
  let relay = tls_relay(&leaf, None).await;
  let url = relay.url().to_string();
  let alice = Endpoint::bind(None, Some(url.clone()), vec![PROTOCOL.as_bytes().to_vec()], None, false, Vec::new())
    .await
    .expect("alice binds");
  let ticket = alice.ticket(TicketOptions::default()).await.expect("the pinned relay comes online");
  let status = alice.relay_status().expect("a configured relay has a status");
  assert!(status.connected && status.error.is_none(), "alice is on her relay");
  assert!(!status.url.contains("pin=") && !status.url.contains('@'), "the status names the URL proper: {}", status.url);
  assert_eq!(parse_ticket(&ticket).expect("a valid ticket").relay_url.as_deref(), Some(url.as_str()));

  // Bob has no relay: he adopts the ticket's, pin included, and checks it.
  // The ticket is cut to the relay, so the relay is his only way to Alice.
  let bob = Endpoint::bind(None, None, Vec::new(), None, false, Vec::new()).await.expect("bob binds");
  let accepting = alice.clone();
  let accepted = tokio::spawn(async move { accepting.accept_one(PROTOCOL.as_bytes()).await });
  let ticket = without_addresses(&ticket);
  let dialed =
    tokio::time::timeout(DIAL_WAIT, bob.connect(ticket, PROTOCOL.to_string(), ConnectOptions::default())).await;
  let stream = dialed.expect("the dial completes in time").expect("the dial succeeds");
  stream.write(b"hello".to_vec()).await.expect("a write over the stream");
  let incoming =
    tokio::time::timeout(DIAL_WAIT, accepted).await.expect("accept in time").expect("accept task").expect("accept");
  assert!(incoming.is_some(), "alice accepted the stream bob dialed through her pinned relay");
  let bare = format!("https://127.0.0.1:{}/", relay.port());
  let known = bob.relays();
  assert_eq!(known.len(), 1, "bob knows the adopted relay");
  assert!(known[0].url == bare && !known[0].configured, "adopted, the URL proper: {}", known[0].url);
  assert_eq!(known[0].pin.as_deref(), Some(leaf.pin.as_str()), "with its pin");
  let own = alice.relays();
  assert!(own.len() == 1 && own[0].configured && own[0].url == bare, "alice knows her configured relay");

  // A ticket without the pin: bob keeps checking the relay by the pin he
  // knows, and the dial lands.
  let pinless = {
    let mut url = url::Url::parse(&url).expect("a relay URL");
    url.set_query(None);
    format!("{}|{url}|", alice.id())
  };
  let accepting = alice.clone();
  let accepted = tokio::spawn(async move { accepting.accept_one(PROTOCOL.as_bytes()).await });
  let stream = tokio::time::timeout(DIAL_WAIT, bob.connect(pinless, PROTOCOL.to_string(), ConnectOptions::default()))
    .await
    .expect("the pinless dial completes in time")
    .expect("a ticket without the pin uses the known one");
  stream.write(b"hello".to_vec()).await.expect("a write over the stream");
  let _ = tokio::time::timeout(DIAL_WAIT, accepted).await;
  assert_eq!(bob.relays().len(), 1, "the same relay");

  bob.close().await;
  alice.close().await;
  relay.close().await;
}

// A failed certificate check fails iroh's HTTPS latency probe already, so
// iroh never makes the relay its home relay and has no error for it: the
// reason in the rejection and the status is what the endpoint's verifier
// recorded.
#[tokio::test]
async fn a_wrong_pin_fails_the_ticket() {
  let leaf = certificate("127.0.0.1", false, None);
  let relay = tls_relay(&leaf, None).await;
  let other = certificate("127.0.0.1", false, None);
  let wrong = with_pin(relay.url(), &other.pin);
  let endpoint =
    Endpoint::bind(None, Some(wrong), Vec::new(), None, false, Vec::new()).await.expect("the endpoint binds");
  let bare = format!("https://127.0.0.1:{}/", relay.port());
  assert_ticket_fails(&endpoint, &bare, "the server's key does not match the pin").await;
  let error = endpoint.relay_status().expect("a status").error.expect("the status carries the certificate error");
  assert!(error.contains("does not match the pin"), "the status names the pin mismatch: {error}");
  endpoint.close().await;
  relay.close().await;
}

#[tokio::test]
async fn a_ca_signed_relay_is_reached_through_relay_ca_certs_only() {
  let ca = certificate("relay-ca.test", true, None);
  let leaf = certificate("127.0.0.1", false, Some(&ca));
  let relay = tls_relay(&leaf, None).await;
  let url = relay.url().to_string();
  let trusting = Endpoint::bind(None, Some(url.clone()), Vec::new(), None, false, vec![ca.cert.clone().into_bytes()])
    .await
    .expect("the endpoint binds");
  trusting.ticket(TicketOptions::default()).await.expect("the relay's chain ends at relayCaCerts");
  let bare = format!("https://127.0.0.1:{}/", relay.port());
  let untrusting =
    Endpoint::bind(None, Some(url), Vec::new(), None, false, Vec::new()).await.expect("the endpoint binds");
  // Without the CA the chain check fails (see `a_wrong_pin_fails_the_ticket`).
  assert_ticket_fails(&untrusting, &bare, "invalid peer certificate").await;
  let error = untrusting.relay_status().expect("a status").error.expect("the status carries the certificate error");
  assert!(error.contains("UnknownIssuer"), "the status names the unknown issuer: {error}");
  untrusting.close().await;
  trusting.close().await;
  relay.close().await;
}

// A pinned relayUrl and relayCaCerts on one endpoint: Bob's own relay is
// pinned (127.0.0.1), and the relay of Alice's ticket is CA-signed and named
// by another host (localhost), so Bob checks one by its pin and the other by
// its chain.
#[tokio::test]
async fn a_pinned_relay_and_relay_ca_certs_work_side_by_side() {
  let ca = certificate("relay-ca.test", true, None);
  let ca_leaf = certificate("localhost", false, Some(&ca));
  let ca_relay = tls_relay(&ca_leaf, None).await;
  let ca_url = ca_relay.url().replace("127.0.0.1", "localhost");
  let pinned_leaf = certificate("127.0.0.1", false, None);
  let pinned_relay = tls_relay(&pinned_leaf, None).await;
  let roots = vec![ca.cert.clone().into_bytes()];

  let alice = Endpoint::bind(None, Some(ca_url), vec![PROTOCOL.as_bytes().to_vec()], None, false, roots.clone())
    .await
    .expect("alice binds");
  let ticket = alice.ticket(TicketOptions::default()).await.expect("alice's CA-signed relay comes online");
  let bob = Endpoint::bind(None, Some(pinned_relay.url().to_string()), Vec::new(), None, false, roots)
    .await
    .expect("bob binds");
  bob.ticket(TicketOptions::default()).await.expect("bob's pinned relay comes online beside relayCaCerts");

  let accepting = alice.clone();
  let accepted = tokio::spawn(async move { accepting.accept_one(PROTOCOL.as_bytes()).await });
  let ticket = without_addresses(&ticket);
  let dialed =
    tokio::time::timeout(DIAL_WAIT, bob.connect(ticket, PROTOCOL.to_string(), ConnectOptions::default())).await;
  let stream = dialed.expect("the dial completes in time").expect("the dial succeeds");
  stream.write(b"hello".to_vec()).await.expect("a write over the stream");
  let incoming =
    tokio::time::timeout(DIAL_WAIT, accepted).await.expect("accept in time").expect("accept task").expect("accept");
  assert!(incoming.is_some(), "alice accepted the stream bob dialed through her CA-signed relay");

  bob.close().await;
  alice.close().await;
  pinned_relay.close().await;
  ca_relay.close().await;
}

#[tokio::test]
async fn a_second_pin_for_one_host_is_refused_at_connect() {
  let leaf = certificate("127.0.0.1", false, None);
  let other = certificate("127.0.0.1", false, None);
  let id = "116072673e30625053b0a67b3b8abf3de89f0a7aa99737b259f802423a1cf9ab";
  let own = format!("https://token@127.0.0.1:1/?pin={}", leaf.pin);
  let endpoint =
    Endpoint::bind(None, Some(own), Vec::new(), None, false, Vec::new()).await.expect("the endpoint binds");
  let ticket = format!("{id}|https://token@127.0.0.1:2/?pin={}|", other.pin);
  let error = endpoint
    .connect(ticket, PROTOCOL.to_string(), ConnectOptions::default())
    .await
    .err()
    .expect("a second pin for the host");
  assert!(error.contains("https://127.0.0.1:2/") && error.contains("already checked"), "the error names it: {error}");
  assert!(!error.contains("token"), "the error carries no token: {error}");
  assert_eq!(endpoint.relays().len(), 1, "the relay is not adopted");

  // The configured relay named with another pin: the pin conflict.
  let ticket = format!("{id}|https://token@127.0.0.1:1/?pin={}|", other.pin);
  let error =
    endpoint.connect(ticket, PROTOCOL.to_string(), ConnectOptions::default()).await.err().expect("another pin");
  assert!(error.contains("https://127.0.0.1:1/") && error.contains("another pin"), "the error names it: {error}");
  assert!(!error.contains(&other.pin) && !error.contains("token@"), "the error carries no secret: {error}");

  // An adopted relay named with another pin: the same conflict. The first
  // dial cannot land (nobody serves the relay); its adoption is what counts.
  let ticket = format!("{id}|https://token@127.0.0.1:3/?pin={}|", leaf.pin);
  let _ =
    tokio::time::timeout(ADOPT_WAIT, endpoint.connect(ticket, PROTOCOL.to_string(), ConnectOptions::default())).await;
  assert_eq!(endpoint.relays().len(), 2, "the relay is adopted");
  let ticket = format!("{id}|https://token@127.0.0.1:3/?pin={}|", other.pin);
  let error =
    endpoint.connect(ticket, PROTOCOL.to_string(), ConnectOptions::default()).await.err().expect("another pin");
  assert!(error.contains("https://127.0.0.1:3/") && error.contains("another pin"), "the error names it: {error}");

  // The configured pinned relay named without a pin: no conflict, the dial
  // goes on (nobody serves it, so it is dropped).
  let ticket = format!("{id}|https://token@127.0.0.1:1/|");
  let dialed =
    tokio::time::timeout(ADOPT_WAIT, endpoint.connect(ticket, PROTOCOL.to_string(), ConnectOptions::default())).await;
  if let Ok(Err(error)) = dialed {
    assert!(!error.starts_with("Relay "), "no conflict for a ticket without the pin: {error}");
  }
  endpoint.close().await;
}

#[tokio::test]
async fn a_pin_for_a_chain_checked_relay_and_a_token_for_a_token_less_one_are_refused() {
  let leaf = certificate("127.0.0.1", false, None);
  let id = "116072673e30625053b0a67b3b8abf3de89f0a7aa99737b259f802423a1cf9ab";
  let endpoint =
    Endpoint::bind(None, Some("https://token@127.0.0.1:1/".to_string()), Vec::new(), None, false, Vec::new())
      .await
      .expect("the endpoint binds");
  let ticket = format!("{id}|https://token@127.0.0.1:1/?pin={}|", leaf.pin);
  let error = endpoint.connect(ticket, PROTOCOL.to_string(), ConnectOptions::default()).await.err().expect("a pin");
  assert!(
    error.contains("https://127.0.0.1:1/")
      && error.contains("the ticket pins it; this endpoint checks its certificate chain"),
    "the error names the relay and the case: {error}"
  );
  assert!(!error.contains(&leaf.pin), "the error carries no pin: {error}");

  // A relay adopted without a token keeps none: a ticket with one is refused.
  let ticket = format!("{id}|http://127.0.0.1:5/|");
  let _ =
    tokio::time::timeout(ADOPT_WAIT, endpoint.connect(ticket, PROTOCOL.to_string(), ConnectOptions::default())).await;
  assert_eq!(endpoint.relays().len(), 2, "the relay is adopted");
  let ticket = format!("{id}|http://{TOKEN_A}@127.0.0.1:5/|");
  let error = endpoint.connect(ticket, PROTOCOL.to_string(), ConnectOptions::default()).await.err().expect("a token");
  assert!(error.contains("this endpoint uses it without a token"), "the error names the case: {error}");
  assert!(!error.contains(TOKEN_A), "the error carries no token: {error}");
  endpoint.close().await;
}

#[tokio::test]
async fn relay_urls_endpoints_refuse() {
  let refused = |url: &str| Endpoint::bind(None, Some(url.to_string()), Vec::new(), None, false, Vec::new());
  for (url, what) in [
    ("wss://token@127.0.0.1:1/", "not http or https"),
    ("ftp://127.0.0.1:1/", "not http or https"),
    ("http://token@127.0.0.1:1/a|b", "separates a ticket's fields"),
    ("https://token@[::1]:1/", "IPv6"),
  ] {
    let error = refused(url).await.err().unwrap_or_else(|| panic!("{url} is refused"));
    assert!(error.starts_with("Invalid relay URL") && error.contains(what), "{url}: {error}");
    assert!(!error.contains("token"), "the error carries no token: {error}");
  }
  let plain = refused("http://token@[::1]:1/").await.expect("plain HTTP on IPv6 binds");
  plain.close().await;
}

#[tokio::test]
async fn a_relay_only_ticket_without_the_relay_is_refused_at_once() {
  let (url, _) = dead_relay_url();
  let endpoint = Endpoint::bind(None, Some(url), Vec::new(), None, true, Vec::new()).await.expect("the endpoint binds");
  let started = tokio::time::Instant::now();
  let error = endpoint
    .ticket(TicketOptions { relay: false, ..TicketOptions::default() })
    .await
    .expect_err("no direct address to write");
  assert!(started.elapsed() < crate::p2p::TICKET_WAIT, "no wait");
  assert!(error.contains("paths \"relay\""), "the error names the cause: {error}");
  endpoint.close().await;
}

/// `ticket` with its relay's user part replaced by `token` (`None`: no
/// user part).
fn ticket_with_token(ticket: &str, token: Option<&str>) -> String {
  let mut parts = ticket.split('|');
  let id = parts.next().unwrap_or("");
  let mut url = url::Url::parse(parts.next().unwrap_or("")).expect("a relay URL in the ticket");
  url.set_username(token.unwrap_or("")).expect("the URL takes a token");
  format!("{id}|{url}|{}", parts.next().unwrap_or(""))
}

/// `ticket` cut to `id|relay|`: no direct address, so the relay is the
/// only way to the peer.
fn without_addresses(ticket: &str) -> String {
  let mut parts = ticket.split('|');
  let id = parts.next().unwrap_or("");
  format!("{id}|{}|", parts.next().unwrap_or(""))
}

/// A dial of `ticket` from `from` that lands: Ok, or the error.
async fn dial(from: &Endpoint, ticket: &str, opts: ConnectOptions) -> Result<(), String> {
  let stream = tokio::time::timeout(DIAL_WAIT, from.connect(ticket.to_string(), PROTOCOL.to_string(), opts))
    .await
    .map_err(|_| "the dial timed out".to_string())??;
  stream.write(b"hello".to_vec()).await
}

#[tokio::test]
async fn a_known_relay_keeps_its_token_and_adopt_relay_false_adopts_nothing() {
  let opts = RelayOptions {
    host: Some("127.0.0.1".to_string()),
    tokens: Some(vec![TOKEN_A.to_string(), TOKEN_B.to_string()]),
    ..RelayOptions::default()
  };
  let relay = Relay::serve(opts).await.expect("the relay serves");
  let bare = format!("http://127.0.0.1:{}/", relay.port());
  let alice =
    Endpoint::bind(None, Some(relay.url().to_string()), vec![PROTOCOL.as_bytes().to_vec()], None, false, Vec::new())
      .await
      .expect("alice binds");
  let local = alice.ticket(TicketOptions::default()).await.expect("alice's relay comes online");
  // A loopback relay reaches no other machine: the ticket carries the
  // local address beside it.
  assert!(!parse_ticket(&local).expect("a valid ticket").addresses.is_empty(), "a local address: {local}");
  let ticket = without_addresses(&local);
  let all = TicketOptions { addresses: TicketAddresses::All, ..TicketOptions::default() };
  let full = alice.ticket(all).await.expect("alice's ticket with every address");
  let accepting = alice.clone();
  let accepts = tokio::spawn(async move { while let Ok(Some(_)) = accepting.accept_one(PROTOCOL.as_bytes()).await {} });

  // Bob adopts the relay with token A; token B for it is refused, no token
  // uses the known one.
  let bob = Endpoint::bind(None, None, Vec::new(), None, false, Vec::new()).await.expect("bob binds");
  dial(&bob, &ticket, ConnectOptions::default()).await.expect("bob adopts the relay");
  let known = bob.relays();
  assert!(known.len() == 1 && known[0].url == bare && !known[0].configured && known[0].pin.is_none());
  let error =
    dial(&bob, &ticket_with_token(&ticket, Some(TOKEN_B)), ConnectOptions::default()).await.expect_err("another token");
  assert!(error.contains(&bare) && error.contains("another token") && error.contains("new endpoint"), "{error}");
  assert!(!error.contains(TOKEN_A) && !error.contains(TOKEN_B), "the error carries no token: {error}");
  dial(&bob, &ticket_with_token(&ticket, None), ConnectOptions::default()).await.expect("no token: the known one");
  let no_adopt = ConnectOptions { adopt_relay: false };
  dial(&bob, &ticket, no_adopt).await.expect("a known relay is used without adopting");
  assert_eq!(bob.relays().len(), 1, "nothing new adopted");

  // Carol's configured relay admits her own token (B): a ticket naming it
  // with token A is fine.
  let carol = Endpoint::bind(None, Some(with_token(relay.url(), TOKEN_B)), Vec::new(), None, false, Vec::new())
    .await
    .expect("carol binds");
  dial(&carol, &ticket, ConnectOptions::default()).await.expect("the configured relay takes another token");
  let known = carol.relays();
  assert!(known.len() == 1 && known[0].configured, "carol knows only her configured relay");

  dial(&carol, &ticket, no_adopt).await.expect("the configured relay is used without adopting");

  // Dave adopts nothing: the ticket cut to the relay is refused before the
  // dial, naming the relay without its token; the loopback relay's own
  // ticket and the full one reach Alice directly.
  let dave = Endpoint::bind(None, None, Vec::new(), None, false, Vec::new()).await.expect("dave binds");
  let error = dial(&dave, &ticket, no_adopt).await.expect_err("no relay adopted, no path");
  assert!(
    error.contains(&format!("Relay {bare}: not known to this endpoint and adoptRelay is false"))
      && error.contains("no direct address"),
    "the error names the relay and the cause: {error}"
  );
  assert!(!error.contains(TOKEN_A) && !error.contains('@'), "the error carries no token: {error}");
  dial(&dave, &local, no_adopt).await.expect("the local address beside a loopback relay reaches alice");
  dial(&dave, &full, no_adopt).await.expect("the direct addresses reach alice");
  assert!(dave.relays().is_empty(), "dave adopted no relay");

  accepts.abort();
  dave.close().await;
  carol.close().await;
  bob.close().await;
  alice.close().await;
  relay.close().await;
}

/// A relay on loopback admitting `tokens` (its default token when `None`),
/// and its URL proper.
async fn loopback_relay(tokens: Option<Vec<String>>) -> (Relay, String) {
  let opts = RelayOptions { host: Some("127.0.0.1".to_string()), tokens, ..RelayOptions::default() };
  let relay = Relay::serve(opts).await.expect("the relay serves");
  let bare = format!("http://127.0.0.1:{}/", relay.port());
  (relay, bare)
}

/// The next chunk `readable` yields, within `DIAL_WAIT`.
async fn read_chunk(readable: &mut ByteStream) -> Vec<u8> {
  let next = std::future::poll_fn(|cx| readable.as_mut().poll_next(cx));
  let chunk = tokio::time::timeout(DIAL_WAIT, next).await.expect("a chunk in time");
  chunk.expect("the stream is open").expect("a read").to_vec()
}

/// Wait until iroh's home relays are `want`, or `DIAL_WAIT` passes.
async fn wait_home(endpoint: &Endpoint, want: &[String]) -> Vec<String> {
  let deadline = tokio::time::Instant::now() + DIAL_WAIT;
  loop {
    let homes = endpoint.home_relays();
    if homes == want || tokio::time::Instant::now() >= deadline {
      return homes;
    }
    tokio::time::sleep(HOME_POLL).await;
  }
}

// Relay-only endpoints, so the relay is the only path: Bob's connection to
// Alice runs through the first relay, and still carries data after Alice
// switched to the second one.
#[tokio::test]
async fn set_relay_url_moves_the_endpoint_to_another_relay() {
  let (first, first_bare) = loopback_relay(None).await;
  let (second, second_bare) = loopback_relay(None).await;
  let alice =
    Endpoint::bind(None, Some(first.url().to_string()), vec![PROTOCOL.as_bytes().to_vec()], None, true, Vec::new())
      .await
      .expect("alice binds");
  let old = alice.ticket(TicketOptions::default()).await.expect("alice's first relay comes online");
  let bob =
    Endpoint::bind(None, Some(first.url().to_string()), Vec::new(), None, true, Vec::new()).await.expect("bob binds");
  let accepting = alice.clone();
  let accepted = tokio::spawn(async move { accepting.accept_one(PROTOCOL.as_bytes()).await });
  let stream = tokio::time::timeout(DIAL_WAIT, bob.connect(old, PROTOCOL.to_string(), ConnectOptions::default()))
    .await
    .expect("the dial completes in time")
    .expect("the dial through the first relay succeeds");
  stream.write(b"before".to_vec()).await.expect("a write before the switch");
  let incoming = tokio::time::timeout(DIAL_WAIT, accepted)
    .await
    .expect("accept in time")
    .expect("accept task")
    .expect("accept")
    .expect("alice accepted the stream");
  let mut from_bob = incoming.readable();
  assert_eq!(read_chunk(&mut from_bob).await, b"before");

  alice.set_relay_url(second.url().to_string()).await.expect("alice switches to the second relay");
  let ticket = alice.ticket(TicketOptions::default()).await.expect("the second relay comes online");
  let parts = parse_ticket(&ticket).expect("a valid ticket");
  assert_eq!(parts.relay_url.as_deref(), Some(second.url()), "the ticket names the second relay: {ticket}");
  let status = alice.relay_status().expect("a configured relay has a status");
  assert!(status.url == second_bare && status.connected, "the status is the second relay, connected");
  let homes = wait_home(&alice, std::slice::from_ref(&second_bare)).await;
  assert_eq!(homes, vec![second_bare.clone()], "iroh made the second relay home");
  let known = alice.relays();
  assert!(known.len() == 1 && known[0].url == second_bare && known[0].configured, "only the second relay is known");
  assert_ne!(known[0].url, first_bare, "the first relay is gone from relays");

  // The connection through the first relay keeps running, both ways.
  stream.write(b"after".to_vec()).await.expect("a write after the switch");
  assert_eq!(read_chunk(&mut from_bob).await, b"after");
  incoming.write(b"reply".to_vec()).await.expect("a reply after the switch");
  assert_eq!(read_chunk(&mut stream.readable()).await, b"reply");

  // A peer on the second relay reaches Alice by her new ticket alone.
  let carol = Endpoint::bind(None, Some(second.url().to_string()), Vec::new(), None, true, Vec::new())
    .await
    .expect("carol binds");
  let accepting = alice.clone();
  let accepted = tokio::spawn(async move { accepting.accept_one(PROTOCOL.as_bytes()).await });
  let cut = without_addresses(&ticket);
  let dialed = tokio::time::timeout(DIAL_WAIT, carol.connect(cut, PROTOCOL.to_string(), ConnectOptions::default()))
    .await
    .expect("carol's dial completes in time")
    .expect("carol dials the new ticket");
  dialed.write(b"hello".to_vec()).await.expect("a write over carol's stream");
  let incoming2 =
    tokio::time::timeout(DIAL_WAIT, accepted).await.expect("accept in time").expect("accept task").expect("accept");
  assert!(incoming2.is_some(), "alice accepted the stream carol dialed through the second relay");

  drop(incoming2);
  drop(dialed);
  drop(from_bob);
  drop(incoming);
  drop(stream);
  carol.close().await;
  bob.close().await;
  alice.close().await;
  second.close().await;
  first.close().await;
}

#[tokio::test]
async fn set_relay_url_keeps_a_known_relays_token() {
  let (relay, bare) = loopback_relay(Some(vec![TOKEN_A.to_string(), TOKEN_B.to_string()])).await;
  let url = relay.url().to_string();
  let alice = Endpoint::bind(None, Some(url.clone()), vec![PROTOCOL.as_bytes().to_vec()], None, false, Vec::new())
    .await
    .expect("alice binds");
  let ticket = without_addresses(&alice.ticket(TicketOptions::default()).await.expect("alice's relay comes online"));
  let accepting = alice.clone();
  let accepts = tokio::spawn(async move { while let Ok(Some(_)) = accepting.accept_one(PROTOCOL.as_bytes()).await {} });

  // Bob adopts the relay with token A, then makes it his own.
  let bob = Endpoint::bind(None, None, Vec::new(), None, false, Vec::new()).await.expect("bob binds");
  dial(&bob, &ticket, ConnectOptions::default()).await.expect("bob adopts the relay");
  bob.set_relay_url(url.clone()).await.expect("the adopted relay becomes bob's own");
  let known = bob.relays();
  assert!(known.len() == 1 && known[0].url == bare && known[0].configured, "one record, configured");
  let own = bob.ticket(TicketOptions::default()).await.expect("bob's relay is online");
  assert_eq!(parse_ticket(&own).expect("a valid ticket").relay_url.as_deref(), Some(url.as_str()));
  let error = bob.set_relay_url(with_token(&url, TOKEN_B)).await.expect_err("another token for the relay");
  assert!(
    error.contains(&bare) && error.contains("the URL carries another token than this endpoint uses for it"),
    "the error names the relay and the case: {error}"
  );
  assert!(!error.contains(TOKEN_A) && !error.contains(TOKEN_B), "the error carries no token: {error}");
  bob.set_relay_url(url.clone()).await.expect("the configured relay again is a no-op");
  assert_eq!(bob.relays().len(), 1, "nothing new");

  // Away and back: the dropped relay keeps token A.
  let (dead, dead_bare) = dead_relay_url();
  bob.set_relay_url(dead).await.expect("bob switches to another relay");
  let known = bob.relays();
  assert!(known.len() == 1 && known[0].url == dead_bare && known[0].configured, "the dropped relay is not listed");
  let error = dial(&bob, &ticket_with_token(&ticket, Some(TOKEN_B)), ConnectOptions::default())
    .await
    .expect_err("a ticket with another token for the dropped relay");
  assert!(error.contains(&bare) && error.contains("the ticket carries another token"), "{error}");
  let error = dial(&bob, &ticket, ConnectOptions { adopt_relay: false }).await.expect_err("not in the map");
  assert!(error.contains("not known to this endpoint and adoptRelay is false"), "a dropped relay is unknown: {error}");
  let error = bob.set_relay_url(with_token(&url, TOKEN_B)).await.expect_err("another token for the dropped relay");
  assert!(error.contains(&bare) && error.contains("the URL carries another token"), "{error}");
  bob.set_relay_url(url.clone()).await.expect("back to the dropped relay with its token");
  let known = bob.relays();
  assert!(known.len() == 1 && known[0].url == bare && known[0].configured, "the relay is configured again");
  let own = bob.ticket(TicketOptions::default()).await.expect("bob is back on the relay");
  assert_eq!(parse_ticket(&own).expect("a valid ticket").relay_url.as_deref(), Some(url.as_str()));

  accepts.abort();
  bob.close().await;
  alice.close().await;
  relay.close().await;
}

// iroh keeps the current home relay while no relay in its map answers, also
// one no longer in the map.
#[tokio::test]
async fn set_relay_url_to_a_dead_relay_leaves_the_old_one_home() {
  let (relay, bare) = loopback_relay(None).await;
  let alice = Endpoint::bind(None, Some(relay.url().to_string()), Vec::new(), None, false, Vec::new())
    .await
    .expect("alice binds");
  alice.ticket(TicketOptions::default()).await.expect("alice's relay comes online");
  let (dead, dead_bare) = dead_relay_url();
  alice.set_relay_url(dead).await.expect("alice switches to a relay nobody serves");
  let reason = format!("iroh made {bare} the home relay");
  assert_eq!(wait_status_error(&alice, &reason).await, reason, "the status names the old home relay");
  let status = alice.relay_status().expect("a configured relay has a status");
  assert!(status.url == dead_bare && !status.connected, "the status is the new relay, not connected");
  assert_ticket_fails(&alice, &dead_bare, &reason).await;
  let known = alice.relays();
  assert!(known.len() == 1 && known[0].url == dead_bare && known[0].configured && !known[0].home);
  alice.close().await;
  relay.close().await;
}

#[tokio::test]
async fn set_relay_url_refuses_a_second_check_for_one_host() {
  let leaf = certificate("127.0.0.1", false, None);
  let other = certificate("127.0.0.1", false, None);
  let own = format!("https://token@127.0.0.1:1/?pin={}", leaf.pin);
  let endpoint =
    Endpoint::bind(None, Some(own), Vec::new(), None, false, Vec::new()).await.expect("the endpoint binds");
  let unchanged = |endpoint: &Endpoint| {
    let status = endpoint.relay_status().expect("a configured relay has a status");
    assert_eq!(status.url, "https://127.0.0.1:1/", "the status names the relay as before");
    let known = endpoint.relays();
    assert!(known.len() == 1 && known[0].url == "https://127.0.0.1:1/" && known[0].configured, "as before");
    assert_eq!(known[0].pin.as_deref(), Some(leaf.pin.as_str()), "with its pin");
  };
  let error = endpoint
    .set_relay_url(format!("https://token@127.0.0.1:2/?pin={}", other.pin))
    .await
    .expect_err("a second pin for the host");
  assert!(error.contains("https://127.0.0.1:2/") && error.contains("already checked"), "the error names it: {error}");
  assert!(!error.contains("token"), "the error carries no token: {error}");
  unchanged(&endpoint);
  // The configured relay with another pin: the pin conflict.
  let error = endpoint
    .set_relay_url(format!("https://token@127.0.0.1:1/?pin={}", other.pin))
    .await
    .expect_err("another pin for the configured relay");
  assert!(error.contains("the URL carries another pin"), "the error names the case: {error}");
  assert!(!error.contains(&other.pin), "the error carries no pin: {error}");
  unchanged(&endpoint);
  endpoint.close().await;
}

#[test]
fn relay_url_pin_is_parsed_and_kept() {
  let id = "116072673e30625053b0a67b3b8abf3de89f0a7aa99737b259f802423a1cf9ab";
  let pin = "ab".repeat(crate::tls::PIN_LEN);
  let relay = format!("https://secret@relay.example.test:8443/?pin={pin}");
  let ticket = parse_ticket(&format!("{id}|{relay}|")).expect("a valid ticket");
  assert_eq!(ticket.relay_url.as_deref(), Some(relay.as_str()), "token and pin round-trip");
  let bad = |relay: String, what: &str| {
    let error = parse_ticket(&format!("{id}|{relay}|")).err().unwrap_or_else(|| panic!("{relay} is an error"));
    assert!(error.contains(what), "the error names {what}: {error}");
    assert!(!error.contains("secret") && !error.contains("hunter2"), "the error carries no secret: {error}");
  };
  bad(format!("http://secret@127.0.0.1:3340/?pin={pin}"), "https://");
  bad("https://relay.example.test/?pin=00".to_string(), "pin must be");
  bad(format!("https://relay.example.test/?pin={pin}&pin={pin}"), "twice");
  bad("https://relay.example.test/?quic=7842".to_string(), "unknown query parameter");
  bad("https://secret:hunter2@relay.example.test/".to_string(), "password");
  bad("https://secret@relay.example.test/#x".to_string(), "fragment");
  bad("wss://secret@relay.example.test/".to_string(), "not http or https");
  bad("https://secret@[::1]:8443/".to_string(), "IPv6");
  assert!(parse_ticket(&format!("{id}|http://secret@[::1]:3340/|")).is_ok(), "plain HTTP on IPv6");
  let error = parse_ticket(&format!("{id}|{relay}|127.0.0.1:4000|extra")).err().expect("a fourth field");
  assert!(error.contains("more than 3 fields"), "the error names the fields: {error}");
}

// The relay's URL host for an unspecified bind: the default route's IPv4,
// else the first non-loopback IPv4 of the interfaces, else loopback.
#[test]
fn an_unspecified_relay_names_the_default_route_else_a_non_loopback_ipv4() {
  let lan = IpAddr::from(Ipv4Addr::new(192, 168, 1, 20));
  let vpn = IpAddr::from(Ipv4Addr::new(10, 8, 0, 2));
  let loopback = IpAddr::from(Ipv4Addr::LOCALHOST);
  let global_v6 = IpAddr::from(Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1));
  let up = [loopback, vpn, lan, global_v6];
  assert_eq!(relay_host_ipv4(Some(&[global_v6, lan]), &up), Ipv4Addr::new(192, 168, 1, 20), "the default route");
  assert_eq!(relay_host_ipv4(None, &up), Ipv4Addr::new(10, 8, 0, 2), "no default route: the first non-loopback");
  assert_eq!(relay_host_ipv4(Some(&[global_v6]), &up), Ipv4Addr::new(10, 8, 0, 2), "an IPv6-only default route");
  assert_eq!(relay_host_ipv4(None, &[loopback, global_v6]), Ipv4Addr::LOCALHOST, "loopback only with no other IPv4");
  let self_assigned = IpAddr::from(Ipv4Addr::new(169, 254, 3, 4));
  assert_eq!(relay_host_ipv4(None, &[self_assigned, vpn]), Ipv4Addr::new(10, 8, 0, 2), "no link-local fallback");
  assert_eq!(relay_host_ipv4(None, &[self_assigned]), Ipv4Addr::LOCALHOST, "a link-local IPv4 alone is none");
}
