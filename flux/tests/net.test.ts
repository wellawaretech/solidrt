// flux:net: interface listing, the connect-scan probe, UDP send/recv, and a
// TCP echo round-trip over the Conn/Listener classes. All use loopback
// only, so they need no network and stay deterministic.
import { expect, test } from "flux:test"
import { connect, interfaces, listen, probe, udp } from "flux:net"

let portOf = (addr: string) => Number(addr.split(":").pop())

test("interfaces lists loopback", () => {
  let ifs = interfaces()
  expect(ifs.some((i) => i.loopback)).toBe(true)
  expect(ifs.some((i) => i.addrs.some((a) => a.ip === "127.0.0.1"))).toBe(true)
})

test("probe reports an open listener", async () => {
  // A bound listener's port answers the connect probe as "open".
  let l = await listen(0)
  expect(await probe("127.0.0.1", portOf(l.localAddr))).toBe("open")
})

test("udp sends and receives", async () => {
  // One socket sends a datagram to another's port; recv() yields the bytes,
  // decoded back to the original string.
  let rx = await udp({ reuse: true })
  let tx = await udp({ reuse: true })
  await tx.send("ping", "127.0.0.1", portOf(rx.localAddr))
  let msg = await rx.recv()
  expect(new TextDecoder().decode(msg!.data)).toBe("ping")
})

test("connect writes and reads the echo", async () => {
  // End-to-end over the Conn/Listener classes: the server accepts one
  // connection, reads one chunk, echoes it, and closes; the client writes
  // "hi" and reads the echo back. Exercises listen-accept iteration plus
  // Conn read/write.
  let l = await listen(0)
  let served = (async () => {
    let s = l[Symbol.asyncIterator]()
    let { value: conn } = await s.next() // accept one connection
    let r = conn![Symbol.asyncIterator]()
    let { value: chunk } = await r.next() // read one chunk
    await conn!.write(chunk!) // echo it back
    conn!.close()
  })()
  let conn = await connect("127.0.0.1", portOf(l.localAddr))
  await conn.write("hi")
  let r = conn[Symbol.asyncIterator]()
  let { value } = await r.next()
  expect(new TextDecoder().decode(value!)).toBe("hi")
  conn.close()
  await served
})
