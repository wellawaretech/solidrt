//! Engine-free mDNS / DNS-SD core.
//!
//! The scripting-engine-independent half of `flux:mdns`: zero-config
//! (Bonjour/Avahi) discovery over the link-local multicast group, built on the
//! `forge::net` UDP socket plus `hickory-proto` for the DNS wire codec. It names
//! no scripting-engine types; the marshalling layer (`flux/src/plugins/modules/
//! mdns.rs`) decodes JS args into these calls and encodes the results back to JS.
//!
//! Why this exists: `.local` names are mDNS, not unicast DNS, so a host with no
//! `nss-mdns` in its resolver cannot resolve them. `forge::net` has no resolver to
//! piggyback on, but it has UDP multicast - which is exactly the mDNS transport -
//! so we speak mDNS directly in the runtime (like `forge::p2p` wraps iroh) and
//! every app gets reverse `.local` resolution plus DNS-SD service browsing with no
//! external binary and no root.
//!
//! Three queries, one transport: `resolve` (reverse PTR -> `.local` host),
//! `browse` (a DNS-SD service type -> its instances), and `services` (the service
//! types advertised on the LAN). All three build on the private `query` helper
//! (bind 5353, join the group on each interface, send, collect for a window) and a
//! pure `correlate_*` step over the collected messages, so the parsing is testable
//! without a LAN. End-to-end resolution of real names needs a Bonjour/avahi
//! responder on the link.

use std::collections::HashSet;
use std::net::Ipv4Addr;
use std::time::{Duration, Instant};

use hickory_proto::op::{Message, MessageType, OpCode, Query};
use hickory_proto::rr::{DNSClass, Name, RData, RecordType};

use crate::net;

/// The link-local mDNS multicast group (RFC 6762).
const MDNS_GROUP: Ipv4Addr = Ipv4Addr::new(224, 0, 0, 251);
/// The mDNS port. Bound with reuse so we coexist with a running avahi/mDNSResponder.
const MDNS_PORT: u16 = 5353;
/// The DNS-SD meta-query that enumerates the service types on the link.
const SERVICE_ENUM: &str = "_services._dns-sd._udp.local.";

/// One discovered DNS-SD service instance. Engine-free: the marshalling layer
/// encodes this to a JS object.
pub struct ServiceInstance {
  /// The human instance label, e.g. `"Office Printer"`.
  pub instance: String,
  /// The service type, e.g. `"_ipp._tcp"`.
  pub service: String,
  /// The target host the SRV record points at, e.g. `"printer.local"`.
  pub host: String,
  pub port: u16,
  /// A/AAAA addresses for `host`, when the responder bundled them.
  pub addrs: Vec<String>,
  /// TXT key/value attributes (a bare flag attribute has an empty value).
  pub txt: Vec<(String, String)>,
}

/// Reverse-resolve each IPv4 address to its mDNS `.local` hostname (a PTR query
/// against `in-addr.arpa`). Returns the `(ip, host)` pairs that answered within
/// `timeout_ms`. IPv6 inputs are skipped (the immediate consumer scans v4 subnets).
pub async fn resolve(ips: Vec<String>, timeout_ms: u64) -> Result<Vec<(String, String)>, String> {
  let mut questions = Vec::new();
  // (reverse-name lowercased, original ip): correlate answers back to the input.
  let mut wanted: Vec<(String, String)> = Vec::new();
  for ip in ips {
    let Ok(v4) = ip.parse::<Ipv4Addr>() else {
      continue; // IPv4 reverse only
    };
    let rev = reverse_ptr_name(v4);
    let Ok(name) = Name::from_ascii(&rev) else {
      continue;
    };
    questions.push((name, RecordType::PTR));
    wanted.push((rev.to_ascii_lowercase(), ip));
  }
  if questions.is_empty() {
    return Ok(Vec::new());
  }
  let messages = query(questions, timeout_ms).await?;
  Ok(correlate_resolve(&messages, &wanted))
}

/// Browse a DNS-SD service type (e.g. `"_http._tcp"`) for the instances on the
/// LAN. The service may be given bare or fully qualified; `.local.` is appended if
/// absent.
pub async fn browse(service: String, timeout_ms: u64) -> Result<Vec<ServiceInstance>, String> {
  let service_fqdn = ensure_local(&service);
  let name = Name::from_ascii(&service_fqdn).map_err(|e| format!("mdns: invalid service '{service}': {e}"))?;
  let messages = query(vec![(name, RecordType::PTR)], timeout_ms).await?;
  Ok(correlate_browse(&messages, &service_fqdn))
}

/// Enumerate the service types advertised on the LAN (the
/// `_services._dns-sd._udp.local` meta-query). Returns types like `"_http._tcp"`.
pub async fn services(timeout_ms: u64) -> Result<Vec<String>, String> {
  let name = Name::from_ascii(SERVICE_ENUM).map_err(|e| e.to_string())?;
  let messages = query(vec![(name, RecordType::PTR)], timeout_ms).await?;
  Ok(correlate_services(&messages))
}

// ---- transport --------------------------------------------------------------

/// Bind the mDNS socket, send one query carrying `questions`, and collect every
/// response datagram for a `timeout_ms` window. The shared step all three public
/// queries build on. Standard multicast-response (QM, `DNSClass::IN`) queries: we
/// are bound to 5353 and joined the group, so the multicast answers reach us; the
/// QU/unicast-response optimization is not needed.
async fn query(questions: Vec<(Name, RecordType)>, timeout_ms: u64) -> Result<Vec<Message>, String> {
  let mut msg = Message::new(0, MessageType::Query, OpCode::Query);
  for (name, rtype) in questions {
    let mut q = Query::query(name, rtype);
    q.set_query_class(DNSClass::IN);
    msg.add_query(q);
  }
  let packet = msg.to_vec().map_err(|e| format!("mdns: build query: {e}"))?;

  let udp = net::udp_bind(MDNS_PORT, true).await?;
  let _ = udp.set_multicast_ttl(1); // keep the query on the local link
  join_link_interfaces(&udp);

  udp.send(&packet, &MDNS_GROUP.to_string(), MDNS_PORT).await?;
  Ok(collect_responses(&udp, timeout_ms).await)
}

/// Join the mDNS group on every up, non-loopback, multicast-capable IPv4
/// interface so a multi-homed host (VPN/docker) hears answers on the LAN leg, not
/// just the default route. Falls back to the unspecified address if none qualify.
fn join_link_interfaces(udp: &net::Udp) {
  let mut joined = false;
  for iface in net::interfaces() {
    if !iface.up || iface.loopback || !iface.multicast {
      continue;
    }
    for addr in &iface.addrs {
      if addr.family != "v4" {
        continue;
      }
      if let Ok(ip) = addr.ip.parse::<Ipv4Addr>() {
        if udp.join_multicast(MDNS_GROUP, ip).is_ok() {
          joined = true;
        }
      }
    }
  }
  if !joined {
    let _ = udp.join_multicast(MDNS_GROUP, Ipv4Addr::UNSPECIFIED);
  }
}

/// Receive and DNS-parse datagrams until the `timeout_ms` window closes. Stray
/// undecodable datagrams are dropped; a socket error ends collection early.
async fn collect_responses(udp: &net::Udp, timeout_ms: u64) -> Vec<Message> {
  let deadline = Instant::now() + Duration::from_millis(timeout_ms);
  let mut out = Vec::new();
  loop {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
      break;
    }
    match tokio::time::timeout(remaining, udp.recv()).await {
      Ok(Ok((buf, _ip, _port))) => {
        if let Ok(m) = Message::from_vec(&buf) {
          out.push(m); // query packets carry no answers, so they fall out in correlation
        }
      }
      Ok(Err(_)) => break, // socket closed/errored
      Err(_) => break,     // window elapsed
    }
  }
  out
}

// ---- correlation (pure; testable without a LAN) -----------------------------

/// Match PTR answers back to the requested reverse names, yielding `(ip, host)`
/// pairs (deduped). `wanted` is `(reverse-name lowercased, ip)`.
fn correlate_resolve(messages: &[Message], wanted: &[(String, String)]) -> Vec<(String, String)> {
  let mut out = Vec::new();
  let mut seen = HashSet::new();
  for m in messages {
    for rec in m.all_sections() {
      let RData::PTR(ptr) = &rec.data else {
        continue;
      };
      let rec_name = rec.name.to_ascii().to_ascii_lowercase();
      let Some((_, ip)) = wanted.iter().find(|(rev, _)| *rev == rec_name) else {
        continue;
      };
      let host = trim_dot(&ptr.0.to_utf8());
      if seen.insert((ip.clone(), host.clone())) {
        out.push((ip.clone(), host));
      }
    }
  }
  out
}

/// Assemble `ServiceInstance`s for `service_fqdn` from the responses: the service
/// PTR names the instances, each instance's SRV gives host+port and TXT the
/// attributes, and the SRV target's A/AAAA give the addresses. Responders bundle
/// these in the additionals; some split them across packets, so all sections of
/// every message are indexed.
fn correlate_browse(messages: &[Message], service_fqdn: &str) -> Vec<ServiceInstance> {
  let service_key = service_fqdn.to_ascii_lowercase();
  let service_short = strip_local(service_fqdn);

  // The instance names the service PTR points at (dedup by fqdn, keep original case).
  let mut instances: Vec<Name> = Vec::new();
  for m in messages {
    for rec in m.all_sections() {
      if let RData::PTR(ptr) = &rec.data {
        if rec.name.to_ascii().to_ascii_lowercase() == service_key {
          let target = ptr.0.clone();
          let key = target.to_ascii().to_ascii_lowercase();
          if !instances.iter().any(|n| n.to_ascii().to_ascii_lowercase() == key) {
            instances.push(target);
          }
        }
      }
    }
  }

  let mut out = Vec::new();
  for inst in &instances {
    let inst_key = inst.to_ascii().to_ascii_lowercase();
    let mut host = String::new();
    let mut port = 0u16;
    let mut txt = Vec::new();
    for m in messages {
      for rec in m.all_sections() {
        if rec.name.to_ascii().to_ascii_lowercase() != inst_key {
          continue;
        }
        match &rec.data {
          RData::SRV(srv) => {
            host = trim_dot(&srv.target.to_utf8());
            port = srv.port;
          }
          RData::TXT(t) => {
            for entry in t.txt_data.iter() {
              if let Some(pair) = parse_txt_entry(entry) {
                txt.push(pair);
              }
            }
          }
          _ => {}
        }
      }
    }
    let addrs = if host.is_empty() { Vec::new() } else { collect_addrs(messages, &host) };
    out.push(ServiceInstance { instance: first_label(inst), service: service_short.clone(), host, port, addrs, txt });
  }
  out
}

/// The A/AAAA addresses advertised for `host` (a dot-trimmed `.local` name).
fn collect_addrs(messages: &[Message], host: &str) -> Vec<String> {
  let host_key = host.to_ascii_lowercase();
  let mut addrs = Vec::new();
  for m in messages {
    for rec in m.all_sections() {
      if trim_dot(&rec.name.to_ascii()).to_ascii_lowercase() != host_key {
        continue;
      }
      match &rec.data {
        RData::A(a) => addrs.push(a.0.to_string()),
        RData::AAAA(a) => addrs.push(a.0.to_string()),
        _ => {}
      }
    }
  }
  addrs
}

/// The service types named by the meta-query's PTR answers, e.g. `"_http._tcp"`.
fn correlate_services(messages: &[Message]) -> Vec<String> {
  let meta_key = SERVICE_ENUM.to_ascii_lowercase();
  let mut out = Vec::new();
  for m in messages {
    for rec in m.all_sections() {
      if let RData::PTR(ptr) = &rec.data {
        if rec.name.to_ascii().to_ascii_lowercase() == meta_key {
          let svc = strip_local(&ptr.0.to_ascii());
          if !out.contains(&svc) {
            out.push(svc);
          }
        }
      }
    }
  }
  out
}

// ---- name helpers -----------------------------------------------------------

/// `192.168.1.10` -> `"10.1.168.192.in-addr.arpa."`, the reverse PTR name.
fn reverse_ptr_name(ip: Ipv4Addr) -> String {
  let [a, b, c, d] = ip.octets();
  format!("{d}.{c}.{b}.{a}.in-addr.arpa.")
}

/// Fully qualify a service type with `.local.` if it is not already, e.g.
/// `"_http._tcp"` -> `"_http._tcp.local."`.
fn ensure_local(service: &str) -> String {
  let s = trim_dot(service);
  if s.to_ascii_lowercase().ends_with(".local") {
    format!("{s}.")
  } else {
    format!("{s}.local.")
  }
}

/// Drop a trailing `.local` (and the FQDN dot), e.g. `"_http._tcp.local."` ->
/// `"_http._tcp"`. A name without the suffix is returned dot-trimmed.
fn strip_local(name: &str) -> String {
  let s = trim_dot(name);
  match s.strip_suffix(".local").or_else(|| s.strip_suffix(".LOCAL")) {
    Some(short) => short.to_string(),
    None => s,
  }
}

/// The first DNS label of `name` as a string (raw bytes, unescaped) - the human
/// instance label of a DNS-SD instance name.
fn first_label(name: &Name) -> String {
  match name.iter().next() {
    Some(bytes) => String::from_utf8_lossy(bytes).into_owned(),
    None => String::new(),
  }
}

/// Strip a single trailing `.` (the FQDN root) from a name string.
fn trim_dot(s: &str) -> String {
  s.strip_suffix('.').unwrap_or(s).to_string()
}

/// Split one TXT entry into `(key, value)`. A bare flag attribute (no `=`) yields
/// an empty value; an empty entry is dropped.
fn parse_txt_entry(bytes: &[u8]) -> Option<(String, String)> {
  let s = String::from_utf8_lossy(bytes);
  match s.split_once('=') {
    Some((k, v)) => Some((k.to_string(), v.to_string())),
    None if s.is_empty() => None,
    None => Some((s.to_string(), String::new())),
  }
}

#[cfg(test)]
mod tests {
  // The transport (bind/join/send/recv) needs a real mDNS responder on the link,
  // so these cover the deterministic pieces: name construction, the query
  // round-trip, and correlation over a synthetic response built with hickory.
  use super::*;
  use hickory_proto::rr::rdata::{A, AAAA, PTR, SRV, TXT};
  use hickory_proto::rr::Record;

  #[test]
  fn reverse_name_for_ipv4() {
    assert_eq!(reverse_ptr_name(Ipv4Addr::new(192, 168, 1, 10)), "10.1.168.192.in-addr.arpa.");
  }

  #[test]
  fn service_qualification() {
    assert_eq!(ensure_local("_http._tcp"), "_http._tcp.local.");
    assert_eq!(ensure_local("_http._tcp.local"), "_http._tcp.local.");
    assert_eq!(strip_local("_http._tcp.local."), "_http._tcp");
  }

  #[test]
  fn query_round_trips_through_the_wire() {
    let mut msg = Message::new(0, MessageType::Query, OpCode::Query);
    let mut q = Query::query(Name::from_ascii("_ipp._tcp.local.").unwrap(), RecordType::PTR);
    q.set_query_class(DNSClass::IN);
    msg.add_query(q);

    let bytes = msg.to_vec().expect("encode");
    let parsed = Message::from_vec(&bytes).expect("decode");
    let q = parsed.queries.first().expect("a question");
    assert_eq!(q.name().to_ascii(), "_ipp._tcp.local.");
    assert_eq!(q.query_type(), RecordType::PTR);
  }

  #[test]
  fn correlate_resolve_matches_ptr_answer() {
    let mut m = Message::new(0, MessageType::Response, OpCode::Query);
    let rev = Name::from_ascii("10.1.168.192.in-addr.arpa.").unwrap();
    let host = Name::from_ascii("printer.local.").unwrap();
    m.answers.push(Record::from_rdata(rev, 120, RData::PTR(PTR(host))));

    let wanted = vec![("10.1.168.192.in-addr.arpa.".to_string(), "192.168.1.10".to_string())];
    let out = correlate_resolve(&[m], &wanted);
    assert_eq!(out, vec![("192.168.1.10".to_string(), "printer.local".to_string())]);
  }

  #[test]
  fn correlate_browse_assembles_instance() {
    let service = Name::from_ascii("_ipp._tcp.local.").unwrap();
    // The instance label carries a space, which is legal on the wire but not in
    // from_ascii presentation format, so build it from raw labels.
    let instance =
      Name::from_labels(vec![b"Office Printer".to_vec(), b"_ipp".to_vec(), b"_tcp".to_vec(), b"local".to_vec()])
        .unwrap();
    let host = Name::from_ascii("printer.local.").unwrap();

    let mut m = Message::new(0, MessageType::Response, OpCode::Query);
    m.answers.push(Record::from_rdata(service, 120, RData::PTR(PTR(instance.clone()))));
    m.additionals.push(Record::from_rdata(instance.clone(), 120, RData::SRV(SRV::new(0, 0, 631, host.clone()))));
    m.additionals.push(Record::from_rdata(
      instance,
      120,
      RData::TXT(TXT::new(vec!["rp=ipp/print".into(), "color".into()])),
    ));
    m.additionals.push(Record::from_rdata(host, 120, RData::A(A(Ipv4Addr::new(192, 168, 1, 10)))));

    let out = correlate_browse(&[m], "_ipp._tcp.local.");
    assert_eq!(out.len(), 1);
    let inst = &out[0];
    assert_eq!(inst.instance, "Office Printer");
    assert_eq!(inst.service, "_ipp._tcp");
    assert_eq!(inst.host, "printer.local");
    assert_eq!(inst.port, 631);
    assert_eq!(inst.addrs, vec!["192.168.1.10".to_string()]);
    assert_eq!(inst.txt, vec![("rp".to_string(), "ipp/print".to_string()), ("color".to_string(), String::new())]);
  }

  #[test]
  fn correlate_services_lists_types() {
    let meta = Name::from_ascii(SERVICE_ENUM).unwrap();
    let mut m = Message::new(0, MessageType::Response, OpCode::Query);
    m.answers.push(Record::from_rdata(
      meta.clone(),
      120,
      RData::PTR(PTR(Name::from_ascii("_ipp._tcp.local.").unwrap())),
    ));
    m.answers.push(Record::from_rdata(meta, 120, RData::PTR(PTR(Name::from_ascii("_http._tcp.local.").unwrap()))));

    let out = correlate_services(&[m]);
    assert_eq!(out, vec!["_ipp._tcp".to_string(), "_http._tcp".to_string()]);
  }

  // AAAA is exercised only for the address-collection path's type coverage.
  #[test]
  fn collect_addrs_includes_v6() {
    let host = Name::from_ascii("printer.local.").unwrap();
    let mut m = Message::new(0, MessageType::Response, OpCode::Query);
    m.additionals.push(Record::from_rdata(host, 120, RData::AAAA(AAAA("fe80::1".parse().unwrap()))));
    assert_eq!(collect_addrs(&[m], "printer.local"), vec!["fe80::1".to_string()]);
  }
}
