//! The direct addresses a ticket without a relay carries
//! (`TicketAddresses::Auto`): the default-route interface's IPv4, picked out of
//! the endpoint's addresses by a pure function over made-up interfaces (a
//! VPN, a container bridge, the LAN, a global IPv6), and the read of the
//! default-route interface's IPs from a netwatch interface state; which
//! relay hosts reach no other machine, so a ticket naming one carries the
//! local addresses as well.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use crate::p2p::{default_route_ips, local_relay, local_ticket_addresses, TicketAddresses};

// The port iroh pairs every interface address with.
const PORT: u16 = 50123;
const VPN: Ipv4Addr = Ipv4Addr::new(10, 8, 0, 2);
const BRIDGE: Ipv4Addr = Ipv4Addr::new(172, 17, 0, 1);
const LAN: Ipv4Addr = Ipv4Addr::new(192, 168, 1, 20);
const GLOBAL_V6: Ipv6Addr = Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1);
const LINK_LOCAL_V6: Ipv6Addr = Ipv6Addr::new(0xfe80, 0, 0, 0, 0, 0, 0, 1);
// A default-route IPv4 iroh did not report.
const UNREPORTED: Ipv4Addr = Ipv4Addr::new(192, 168, 7, 7);
// The one address of netwatch's fake interface state (`State::fake`).
const FAKE_WIFI: Ipv4Addr = Ipv4Addr::new(192, 168, 0, 189);

fn addr(ip: impl Into<IpAddr>) -> SocketAddr {
  SocketAddr::new(ip.into(), PORT)
}

/// What iroh reports on a desktop with a WireGuard tunnel, Docker and IPv6.
fn desktop() -> Vec<SocketAddr> {
  vec![addr(VPN), addr(BRIDGE), addr(LAN), addr(GLOBAL_V6)]
}

#[test]
fn the_lan_ipv4_of_the_default_route_only() {
  let route: Vec<IpAddr> = vec![LAN.into(), LINK_LOCAL_V6.into(), GLOBAL_V6.into()];
  assert_eq!(local_ticket_addresses(&desktop(), Some(&route)), vec![addr(LAN)]);
}

#[test]
fn a_full_tunnel_vpn_owns_the_default_route() {
  let route: Vec<IpAddr> = vec![VPN.into()];
  assert_eq!(local_ticket_addresses(&desktop(), Some(&route)), vec![addr(VPN)]);
}

#[test]
fn no_default_route_is_every_ipv4_and_no_ipv6() {
  assert_eq!(local_ticket_addresses(&desktop(), None), vec![addr(VPN), addr(BRIDGE), addr(LAN)]);
}

#[test]
fn a_default_route_without_ipv4_is_every_ipv4() {
  let route: Vec<IpAddr> = vec![GLOBAL_V6.into(), LINK_LOCAL_V6.into()];
  assert_eq!(local_ticket_addresses(&desktop(), Some(&route)), vec![addr(VPN), addr(BRIDGE), addr(LAN)]);
}

#[test]
fn a_default_route_ipv4_iroh_does_not_report_is_every_ipv4() {
  let route: Vec<IpAddr> = vec![UNREPORTED.into()];
  assert_eq!(local_ticket_addresses(&desktop(), Some(&route)), vec![addr(VPN), addr(BRIDGE), addr(LAN)]);
}

#[test]
fn no_addresses_is_none() {
  let route: Vec<IpAddr> = vec![LAN.into()];
  assert!(local_ticket_addresses(&[], Some(&route)).is_empty());
  assert!(local_ticket_addresses(&[], None).is_empty());
  assert!(local_ticket_addresses(&[addr(GLOBAL_V6)], None).is_empty(), "never an IPv6 address");
}

#[test]
fn the_default_route_ips_come_from_the_interface_state() {
  // netwatch's fake: one interface, wifi0, the default route.
  let mut state = netwatch::interfaces::State::fake();
  let ips = default_route_ips(&state).expect("a default route");
  assert_eq!(ips, vec![IpAddr::from(FAKE_WIFI)]);
  state.default_route_interface = Some("missing0".to_string());
  assert!(default_route_ips(&state).is_none(), "an interface not in the state is no default route");
  state.default_route_interface = None;
  assert!(default_route_ips(&state).is_none(), "no default route");
}

#[test]
fn the_addresses_option_parses() {
  assert_eq!(TicketAddresses::parse("auto"), Ok(TicketAddresses::Auto));
  assert_eq!(TicketAddresses::parse("all"), Ok(TicketAddresses::All));
  let error = TicketAddresses::parse("lan").expect_err("an unknown value");
  assert!(error.starts_with("addresses must be"), "the error names the option: {error}");
  assert!(error.contains("\"auto\" or \"all\"") && error.contains("lan"), "the error names the choices: {error}");
}

#[test]
fn a_loopback_link_local_or_unspecified_relay_serves_this_machine_only() {
  let local = |url: &str| local_relay(&url::Url::parse(url).expect("a relay URL"));
  for url in [
    "http://token@127.0.0.1:3340/",
    "http://127.8.0.1:3340/",
    "http://localhost:3340/",
    "https://LOCALHOST:8443/",
    "http://localhost.:3340/",
    "http://relay.localhost:3340/",
    "http://a.b.Localhost.:3340/",
    "http://169.254.10.1:3340/",
    "http://0.0.0.0:3340/",
    "http://[::1]:3340/",
    "http://[fe80::1]:3340/",
    "http://[::]:3340/",
    "http://[::ffff:127.0.0.1]:3340/",
  ] {
    assert!(local(url), "{url} serves this machine only");
  }
  for url in [
    "http://token@192.168.1.20:3340/",
    "http://10.8.0.2:3340/",
    "https://relay.example.test/",
    "http://notlocalhost:3340/",
    "http://localhost.example.test:3340/",
    "http://[2001:db8::1]:3340/",
  ] {
    assert!(!local(url), "{url} reaches other machines");
  }
}
