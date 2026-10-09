// flux:net: interface listing, the connect-scan probe, UDP send/recv, and the
// Conn/Listener classes: the echo, a proxy of two pipes, backpressure, cancel
// and abort over the readable/writable pair. All use loopback only, so they
// need no network and stay deterministic.
import { expect, test } from "flux:test"
import { type Conn, connect, interfaces, listen, type Listener, probe, udp } from "flux:net"

// A write that has not settled this long after the socket buffers filled is
// held back by the peer; loopback settles a write that fits in microseconds.
const SETTLE_MS = 20
// One chunk of the fill, and how many at most before giving up on filling
// the loopback buffers (a few MiB on every platform).
const FILL_CHUNK = 1024 * 1024
const FILL_CHUNKS_MAX = 64

let dec = new TextDecoder()
let portOf = (addr: string) => Number(addr.split(":").pop())
let sleep = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms))

// The next connection a listener accepts.
async function accept(listener: Listener): Promise<Conn> {
  let { value } = await listener[Symbol.asyncIterator]().next()
  return value!
}

// A listener and a connection to it, with the peer the listener accepted.
async function pair() {
  let listener = await listen(0)
  let accepted = accept(listener)
  let conn = await connect("127.0.0.1", portOf(listener.localAddr))
  let peer = await accepted
  return { listener, conn, peer }
}

test("interfaces lists loopback", () => {
  let ifs = interfaces()
  expect(ifs.some((i) => i.loopback)).toBe(true)
  expect(ifs.some((i) => i.addrs.some((a) => a.ip === "127.0.0.1"))).toBe(true)
})

test("probe reports an open listener", async () => {
  // A bound listener's port answers the connect probe as "open".
  let l = await listen(0)
  expect(await probe("127.0.0.1", portOf(l.localAddr))).toBe("open")
  l.close()
})

test("udp sends and receives", async () => {
  // One socket sends a datagram to another's port; recv() yields the bytes,
  // decoded back to the original string.
  let rx = await udp({ reuse: true })
  let tx = await udp({ reuse: true })
  await tx.send("ping", "127.0.0.1", portOf(rx.localAddr))
  let msg = await rx.recv()
  expect(dec.decode(msg!.data)).toBe("ping")
  rx.close()
  tx.close()
})

test("connect writes and reads the echo over the stream pair", async () => {
  // End-to-end over the Conn/Listener classes: the server accepts one
  // connection, reads one chunk, echoes it, and closes; the client writes
  // "hi" and reads the echo back.
  let listener = await listen(0)
  let served = (async () => {
    let conn = await accept(listener)
    let { value: chunk } = await conn.readable.getReader().read()
    let writer = conn.writable.getWriter()
    await writer.write(chunk!)
    conn.close()
  })()
  let conn = await connect("127.0.0.1", portOf(listener.localAddr))
  expect(conn.readable instanceof ReadableStream).toBe(true)
  expect(conn.writable instanceof WritableStream).toBe(true)
  let writer = conn.writable.getWriter()
  await writer.write("hi")
  let { value } = await conn.readable.getReader().read()
  expect(dec.decode(value)).toBe("hi")
  conn.close()
  await served
  listener.close()
})

test("a proxy of two pipes relays the bytes and carries the half-close", async () => {
  // An upstream server reads its request to end-of-stream before it answers,
  // so the answer proves the client's half-close travelled through the proxy:
  // the pipe into the upstream closed its write side when the client's
  // readable ended. The reply travels back the same way.
  let upstream = await listen(0)
  let upstreamGot = (async () => {
    let conn = await accept(upstream)
    let request = await new Response(conn.readable).text()
    let writer = conn.writable.getWriter()
    await writer.write("echo:" + request)
    await writer.close()
    return request
  })()
  let proxy = await listen(0)
  let proxied = (async () => {
    let client = await accept(proxy)
    let server = await connect("127.0.0.1", portOf(upstream.localAddr))
    await Promise.all([client.readable.pipeTo(server.writable), server.readable.pipeTo(client.writable)])
    client.close()
    server.close()
  })()
  let conn = await connect("127.0.0.1", portOf(proxy.localAddr))
  let writer = conn.writable.getWriter()
  await writer.write("hello")
  await writer.close()
  expect(await new Response(conn.readable).text()).toBe("echo:hello")
  expect(await upstreamGot).toBe("hello")
  await proxied
  conn.close()
  upstream.close()
  proxy.close()
})

test("a write to a peer that does not read stays pending once the buffers fill", async () => {
  // The peer never reads: once the loopback buffers are full the OS holds
  // the write, and so does the promise. Closing the connection rejects it.
  let { listener, conn, peer } = await pair()
  let writer = conn.writable.getWriter()
  let chunk = new Uint8Array(FILL_CHUNK)
  let pending: Promise<void> | undefined
  for (let i = 0; i < FILL_CHUNKS_MAX && pending === undefined; i++) {
    let write = writer.write(chunk)
    let outcome = await Promise.race([write.then(() => "settled"), sleep(SETTLE_MS).then(() => "pending")])
    if (outcome === "pending") pending = write
  }
  expect(pending !== undefined).toBe(true)
  conn.close()
  await expect(pending!).rejects.toThrow("connection is closed")
  peer.close()
  listener.close()
})

test("cancelling the readable ends a pending read and leaves the write side usable", async () => {
  let { listener, conn, peer } = await pair()
  let reader = conn.readable.getReader()
  let read = reader.read()
  await reader.cancel()
  expect((await read).done).toBe(true)
  // Reading stopped; writing did not.
  let writer = conn.writable.getWriter()
  await writer.write("still writing")
  let { value } = await peer.readable.getReader().read()
  expect(dec.decode(value)).toBe("still writing")
  conn.close()
  peer.close()
  listener.close()
})

test("aborting the writable tears the connection down with the reason", async () => {
  let { listener, conn, peer } = await pair()
  let read = conn.readable.getReader().read()
  await conn.writable.abort(new Error("upstream broke"))
  // The read side reports the abort; the write side is gone with it.
  await expect(read).rejects.toThrow("upstream broke")
  await expect(conn.writable.getWriter().write("x")).rejects.toThrow()
  // TCP has no abortive half-close: the peer sees a plain end-of-stream.
  expect((await peer.readable.getReader().read()).done).toBe(true)
  peer.close()
  listener.close()
})
