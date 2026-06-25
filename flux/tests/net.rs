#![cfg(feature = "compile")]

mod common;

use common::run_source;
use std::time::Duration;

// flux:net exercised through real JS: interface listing, the connect-scan probe,
// UDP send/recv, and a TCP echo round-trip over the Conn/Listener classes. All
// use loopback only, so they need no network and stay deterministic. Each run is
// wrapped in a hard timeout: a hung socket op fails the test cleanly instead of
// blocking the whole suite (run_source itself has no timeout).
async fn run(code: &str) -> common::Captured {
  tokio::time::timeout(Duration::from_secs(10), run_source(code)).await.expect("flux:net test timed out")
}

#[tokio::test]
async fn interfaces_lists_loopback() {
  let out = run(
    r#"
        import { interfaces } from "flux:net"
        let ifs = interfaces()
        console.log(ifs.some(i => i.loopback), ifs.some(i => i.addrs.some(a => a.ip === "127.0.0.1")))
        "#,
  )
  .await;
  assert!(out.errors().is_empty(), "stderr: {}", out.errors());
  assert_eq!(out.log(), "true true");
}

#[tokio::test]
async fn probe_reports_open_listener() {
  // A bound listener's port answers the connect probe as "open".
  let out = run(
    r#"
        import { listen, probe } from "flux:net"
        let l = await listen(0)
        let port = Number(l.localAddr.split(":").pop())
        console.log(await probe("127.0.0.1", port))
        "#,
  )
  .await;
  assert!(out.errors().is_empty(), "stderr: {}", out.errors());
  assert_eq!(out.log(), "open");
}

#[tokio::test]
async fn udp_send_and_recv() {
  // One socket sends a datagram to another's port; recv() yields the bytes,
  // decoded back to the original string.
  let out = run(
    r#"
        import { udp } from "flux:net"
        let rx = await udp({ reuse: true })
        let port = Number(rx.localAddr.split(":").pop())
        let tx = await udp({ reuse: true })
        await tx.send("ping", "127.0.0.1", port)
        let msg = await rx.recv()
        console.log(new TextDecoder().decode(msg.data))
        "#,
  )
  .await;
  assert!(out.errors().is_empty(), "stderr: {}", out.errors());
  assert_eq!(out.log(), "ping");
}

#[tokio::test]
async fn connect_writes_and_reads_echo() {
  // End-to-end over the Conn/Listener classes: the server accepts one connection,
  // reads one chunk, echoes it, and closes; the client writes "hi" and reads the
  // echo back. Exercises listen-accept iteration plus Conn read/write.
  let out = run(
    r#"
        import { listen, connect } from "flux:net"
        let l = await listen(0)
        let port = Number(l.localAddr.split(":").pop())
        let served = (async () => {
          let s = l[Symbol.asyncIterator]()
          let { value: conn } = await s.next()   // accept one connection
          let r = conn[Symbol.asyncIterator]()
          let { value: chunk } = await r.next()  // read one chunk
          await conn.write(chunk)                 // echo it back
          conn.close()
        })()
        let conn = await connect("127.0.0.1", port)
        await conn.write("hi")
        let r = conn[Symbol.asyncIterator]()
        let { value } = await r.next()
        console.log(new TextDecoder().decode(value))
        conn.close()
        await served
        "#,
  )
  .await;
  assert!(out.errors().is_empty(), "stderr: {}", out.errors());
  assert_eq!(out.log(), "hi");
}
