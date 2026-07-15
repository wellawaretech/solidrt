// The transport (bind/join/send/recv) needs a real mDNS responder on the link,
// so these cover the deterministic pieces: name construction, the query
// round-trip, and correlation over a synthetic response built with hickory.
use std::net::Ipv4Addr;

use hickory_proto::op::{Message, MessageType, OpCode, Query};
use hickory_proto::rr::{DNSClass, Name, RData, RecordType};

use crate::mdns::*;
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
