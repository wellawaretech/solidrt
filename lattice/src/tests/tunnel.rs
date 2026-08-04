//! Needs a live `srt server --tunnel`; pass its ticket (skipped otherwise):
//!
//!   SRT_TUNNEL_TICKET='...' cargo test -p lattice --features go --lib tunnel -- --nocapture

use futures_util::StreamExt;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[test]
fn forwards_dev_protocol() {
  let Ok(ticket) = std::env::var("SRT_TUNNEL_TICKET") else {
    println!("SRT_TUNNEL_TICKET not set; skipping");
    return;
  };
  let rt = tokio::runtime::Runtime::new().expect("runtime");
  rt.block_on(async {
    let (addr, _tunnel) = crate::go::tunnel::start(ticket).await.expect("tunnel start");

    // HTTP through the tunnel: the control API answers on the first read.
    let mut tcp = tokio::net::TcpStream::connect(addr).await.expect("connect loopback");
    tcp.write_all(b"GET /__control__/clients HTTP/1.1\r\nHost: tunnel\r\n\r\n").await.expect("write request");
    let mut buf = vec![0u8; 4096];
    let n = tcp.read(&mut buf).await.expect("read response");
    let head = String::from_utf8_lossy(&buf[..n]).into_owned();
    assert!(head.starts_with("HTTP/1.1 200"), "unexpected response: {head}");

    // WS through the tunnel: upgrade and read the server's welcome push.
    let uri =
      http::Uri::builder().scheme("ws").authority(addr.to_string()).path_and_query("/").build().expect("ws uri");
    let (mut ws, _) = tokio_websockets::ClientBuilder::from_uri(uri).connect().await.expect("ws connect");
    let msg = ws.next().await.expect("ws closed early").expect("ws message");
    let text = msg.as_text().expect("welcome is text").to_string();
    assert!(text.contains("\"welcome\""), "unexpected first message: {text}");
    println!("ok: http 200 + ws welcome through the tunnel ({text})");
  });
}

#[test]
fn decode_hex_roundtrip() {
  let key: [u8; 32] = std::array::from_fn(|i| (i * 7 + 1) as u8);
  let hex: String = key.iter().map(|b| format!("{b:02x}")).collect();
  assert_eq!(crate::go::tunnel::decode_hex(&hex), Some(key));
  assert_eq!(crate::go::tunnel::decode_hex(&hex.to_uppercase()), Some(key));
  // Wrong length, non-hex, non-ASCII: all rejected.
  assert_eq!(crate::go::tunnel::decode_hex(&hex[..62]), None);
  assert_eq!(crate::go::tunnel::decode_hex(&hex.replace('0', "g")), None);
  assert_eq!(crate::go::tunnel::decode_hex(&format!("{}\u{e9}x", &hex[..61])), None);
}
