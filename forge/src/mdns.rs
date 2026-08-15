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
use crate::Value;

/// The link-local mDNS multicast group (RFC 6762).
const MDNS_GROUP: Ipv4Addr = Ipv4Addr::new(224, 0, 0, 251);
/// The mDNS port. Bound with reuse so we coexist with a running avahi/mDNSResponder.
const MDNS_PORT: u16 = 5353;
/// The DNS-SD meta-query that enumerates the service types on the link.
pub(crate) const SERVICE_ENUM: &str = "_services._dns-sd._udp.local.";

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

/// `{ instance, service, host, port, addrs, txt }` with `txt` as a map.
impl From<ServiceInstance> for Value {
  fn from(i: ServiceInstance) -> Value {
    Value::map([
      ("instance", Value::from(i.instance)),
      ("service", Value::from(i.service)),
      ("host", Value::from(i.host)),
      ("port", Value::from(i.port)),
      ("addrs", Value::list(i.addrs)),
      ("txt", Value::map(i.txt)),
    ])
  }
}

/// A `resolve` answer: the address that was asked about and the host name it
/// resolved to.
pub struct ResolvedHost {
  pub ip: String,
  pub host: String,
}

impl From<ResolvedHost> for Value {
  fn from(r: ResolvedHost) -> Value {
    Value::map([("ip", r.ip), ("host", r.host)])
  }
}

/// Reverse-resolve each IPv4 address to its mDNS `.local` hostname (a PTR query
/// against `in-addr.arpa`). Returns the `(ip, host)` pairs that answered within
/// `timeout_ms`. IPv6 inputs are skipped (the immediate consumer scans v4 subnets).
pub async fn resolve(ips: Vec<String>, timeout_ms: u64) -> Result<Vec<ResolvedHost>, String> {
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
  Ok(correlate_resolve(&messages, &wanted).into_iter().map(|(ip, host)| ResolvedHost { ip, host }).collect())
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
      Ok(Ok(Some((buf, _ip, _port)))) => {
        if let Ok(m) = Message::from_vec(&buf) {
          out.push(m); // query packets carry no answers, so they fall out in correlation
        }
      }
      Ok(Ok(None)) => break, // socket closed
      Ok(Err(_)) => break,   // socket errored
      Err(_) => break,       // window elapsed
    }
  }
  out
}

// ---- correlation (pure; testable without a LAN) -----------------------------

/// Match PTR answers back to the requested reverse names, yielding `(ip, host)`
/// pairs (deduped). `wanted` is `(reverse-name lowercased, ip)`.
pub(crate) fn correlate_resolve(messages: &[Message], wanted: &[(String, String)]) -> Vec<(String, String)> {
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
pub(crate) fn correlate_browse(messages: &[Message], service_fqdn: &str) -> Vec<ServiceInstance> {
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
pub(crate) fn collect_addrs(messages: &[Message], host: &str) -> Vec<String> {
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
pub(crate) fn correlate_services(messages: &[Message]) -> Vec<String> {
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
pub(crate) fn reverse_ptr_name(ip: Ipv4Addr) -> String {
  let [a, b, c, d] = ip.octets();
  format!("{d}.{c}.{b}.{a}.in-addr.arpa.")
}

/// Fully qualify a service type with `.local.` if it is not already, e.g.
/// `"_http._tcp"` -> `"_http._tcp.local."`.
pub(crate) fn ensure_local(service: &str) -> String {
  let s = trim_dot(service);
  if s.to_ascii_lowercase().ends_with(".local") {
    format!("{s}.")
  } else {
    format!("{s}.local.")
  }
}

/// Drop a trailing `.local` (and the FQDN dot), e.g. `"_http._tcp.local."` ->
/// `"_http._tcp"`. A name without the suffix is returned dot-trimmed.
pub(crate) fn strip_local(name: &str) -> String {
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
