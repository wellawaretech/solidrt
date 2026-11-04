// flux:net over TLS: connectTls to listenTls with a private CA and ALPN,
// mutual TLS, the pin, startTls after a plain-text exchange, half-close and
// a stalled client. Loopback only, with the forge fixtures (their README
// says how they were made).
import { expect, test } from "flux:test"
import { type Conn, connect, connectTls, listen, listenTls, type Listener, startTls } from "flux:net"
import CA from "../../forge/src/tests/data/tls/ca.pem" with { type: "text" }
import SERVER from "../../forge/src/tests/data/tls/server.pem" with { type: "text" }
import SERVER_KEY from "../../forge/src/tests/data/tls/server-key.pem" with { type: "text" }
import CLIENT from "../../forge/src/tests/data/tls/client.pem" with { type: "text" }
import CLIENT_KEY from "../../forge/src/tests/data/tls/client-key.pem" with { type: "text" }
import CLIENT_PIN from "../../forge/src/tests/data/tls/client.pin" with { type: "text" }
import SELF_SIGNED from "../../forge/src/tests/data/tls/self-signed.pem" with { type: "text" }
import SELF_SIGNED_KEY from "../../forge/src/tests/data/tls/self-signed-key.pem" with { type: "text" }
import SELF_SIGNED_PIN from "../../forge/src/tests/data/tls/self-signed.pin" with { type: "text" }

let enc = new TextEncoder()
let dec = new TextDecoder()
let portOf = (addr: string) => Number(addr.split(":").pop())

// The next connection a listener accepts.
async function accept(listener: Listener): Promise<Conn> {
  let { value } = await listener[Symbol.asyncIterator]().next()
  return value!
}

// The next chunk a connection reads, as text; null at the end.
async function readText(conn: Conn): Promise<string | null> {
  let reader = conn.readable.getReader()
  let { value, done } = await reader.read()
  reader.releaseLock()
  return done ? null : dec.decode(value)
}

async function write(conn: Conn, text: string) {
  let writer = conn.writable.getWriter()
  await writer.write(enc.encode(text))
  writer.releaseLock()
}

// A TLS listener and a connection to it as "localhost", with the peer the
// listener accepted.
async function tlsPair(server: Parameters<typeof listenTls>[1], client: Parameters<typeof connectTls>[2]) {
  let listener = await listenTls(0, server)
  let accepted = accept(listener)
  let conn = await connectTls("localhost", portOf(listener.localAddr), client)
  let peer = await accepted
  return { listener, conn, peer }
}

test("connectTls reaches listenTls through a private CA, with ALPN", async () => {
  let { listener, conn, peer } = await tlsPair(
    { cert: SERVER, key: SERVER_KEY, alpn: ["x-other", "x-test"] },
    { caCerts: [CA], alpn: ["x-test"] },
  )
  expect(conn.alpnProtocol).toBe("x-test")
  expect(peer.alpnProtocol).toBe("x-test")
  expect(conn.peerKey).not.toBe(null)
  expect(peer.peerKey).toBe(null)
  await write(conn, "ping")
  expect(await readText(peer)).toBe("ping")
  await write(peer, "pong")
  expect(await readText(conn)).toBe("pong")
  conn.close()
  peer.close()
  listener.close()
})

test("a plain connection reports no ALPN protocol and no peer key", async () => {
  let listener = await listen(0)
  let accepted = accept(listener)
  let conn = await connect("127.0.0.1", portOf(listener.localAddr))
  let peer = await accepted
  expect(conn.alpnProtocol).toBe(null)
  expect(conn.peerKey).toBe(null)
  conn.close()
  peer.close()
  listener.close()
})

test("the built-in roots reject a private CA", async () => {
  let listener = await listenTls(0, { cert: SERVER, key: SERVER_KEY })
  accept(listener)
  await expect(connectTls("localhost", portOf(listener.localAddr))).rejects.toThrow("invalid peer certificate")
  listener.close()
})

test("mutual TLS with caCerts reports the client's key", async () => {
  let { listener, conn, peer } = await tlsPair(
    { cert: SERVER, key: SERVER_KEY, caCerts: [CA] },
    { caCerts: [CA], cert: CLIENT, key: CLIENT_KEY },
  )
  expect(peer.peerKey).toBe(CLIENT_PIN)
  conn.close()
  peer.close()
  listener.close()
})

test("anyClientCert accepts any client key and reports it", async () => {
  let { listener, conn, peer } = await tlsPair(
    { cert: SERVER, key: SERVER_KEY, anyClientCert: true },
    { caCerts: [CA], cert: CLIENT, key: CLIENT_KEY },
  )
  expect(peer.peerKey).toBe(CLIENT_PIN)
  conn.close()
  peer.close()
  listener.close()
})

test("a pin trusts a self-signed server by its key, and a wrong pin rejects", async () => {
  let listener = await listenTls(0, { cert: SELF_SIGNED, key: SELF_SIGNED_KEY })
  let port = portOf(listener.localAddr)
  let accepted = accept(listener)
  let conn = await connectTls("127.0.0.1", port, { pin: SELF_SIGNED_PIN })
  expect(conn.peerKey).toBe(SELF_SIGNED_PIN)
  ;(await accepted).close()
  conn.close()
  accept(listener)
  await expect(connectTls("127.0.0.1", port, { pin: CLIENT_PIN })).rejects.toThrow("does not match the pin")
  listener.close()
})

test("the options are checked before anything is sent", async () => {
  expect(() => connectTls("localhost", 1, { pin: "abc" })).toThrow("pin must be 64 hex characters")
  expect(() => connectTls("localhost", 1, { pin: CLIENT_PIN, caCerts: [CA] })).toThrow("exclude each other")
  expect(() => connectTls("localhost", 1, { cert: CLIENT })).toThrow("cert and key go together")
  expect(() => listenTls(0, { cert: "not a pem", key: SERVER_KEY })).toThrow("listenTls: cert")
})

// A STARTTLS server: answers the plain-text "STARTTLS" line with "OK", then
// relays the socket to a TLS listener, which does the handshake and echoes.
async function starttlsServer() {
  let tls = await listenTls(0, { cert: SERVER, key: SERVER_KEY })
  let plain = await listen(0)
  ;(async () => {
    let conn = await accept(plain)
    expect(await readText(conn)).toBe("STARTTLS\r\n")
    await write(conn, "OK\r\n")
    let upstream = await connect("127.0.0.1", portOf(tls.localAddr))
    conn.readable.pipeTo(upstream.writable).catch(() => {})
    upstream.readable.pipeTo(conn.writable).catch(() => {})
  })()
  ;(async () => {
    let conn = await accept(tls)
    let text = await readText(conn)
    await write(conn, `echo ${text}`)
  })()
  return { port: portOf(plain.localAddr), close: () => (tls.close(), plain.close()) }
}

test("startTls upgrades after a plain-text answer read with a released reader", async () => {
  let server = await starttlsServer()
  let conn = await connect("127.0.0.1", server.port)
  await write(conn, "STARTTLS\r\n")
  expect(await readText(conn)).toBe("OK\r\n")
  let secure = await startTls(conn, { hostname: "localhost", caCerts: [CA] })
  await expect(write(conn, "late")).rejects.toThrow("connection is closed")
  await write(secure, "secret")
  expect(await readText(secure)).toBe("echo secret")
  secure.close()
  server.close()
})

test("startTls upgrades after a for await that broke", async () => {
  let server = await starttlsServer()
  let conn = await connect("127.0.0.1", server.port)
  await write(conn, "STARTTLS\r\n")
  for await (let chunk of conn.readable) {
    expect(dec.decode(chunk)).toBe("OK\r\n")
    break
  }
  let secure = await startTls(conn, { hostname: "localhost", caCerts: [CA] })
  await write(secure, "after break")
  expect(await readText(secure)).toBe("echo after break")
  secure.close()
  server.close()
})

test("startTls needs a hostname or a pin", async () => {
  let listener = await listen(0)
  accept(listener)
  let conn = await connect("127.0.0.1", portOf(listener.localAddr))
  expect(() => startTls(conn, {})).toThrow("hostname is required without a pin")
  conn.close()
  listener.close()
})

test("the half-close sends close_notify and the answer still arrives", async () => {
  let { listener, conn, peer } = await tlsPair({ cert: SERVER, key: SERVER_KEY }, { caCerts: [CA] })
  let writer = conn.writable.getWriter()
  await writer.write("request")
  await writer.close()
  let got = ""
  for await (let chunk of peer.readable) got += dec.decode(chunk)
  expect(got).toBe("request")
  await write(peer, "answer")
  expect(await readText(conn)).toBe("answer")
  conn.close()
  peer.close()
  listener.close()
})

test("a client that connects and stalls does not hold up the next one", async () => {
  let listener = await listenTls(0, { cert: SERVER, key: SERVER_KEY })
  let port = portOf(listener.localAddr)
  let stalled = await connect("127.0.0.1", port)
  let accepted = accept(listener)
  let conn = await connectTls("localhost", port, { caCerts: [CA] })
  let peer = await accepted
  await write(conn, "first through")
  expect(await readText(peer)).toBe("first through")
  stalled.close()
  conn.close()
  peer.close()
  listener.close()
})
