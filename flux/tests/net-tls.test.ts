// flux:net over TLS: connectTls to listenTls with a private CA and ALPN,
// mutual TLS, the pin, startTls after a plain-text exchange (as the client
// and as the server), half-close, a stalled client, session resumption and
// generated certificates. Loopback only, with the forge fixtures (their
// README says how they were made).
import { expect, test } from "flux:test"
import { createHttpClient, serve } from "flux:http"
import {
  type Conn,
  connect,
  connectTls,
  generateCertificate,
  listen,
  listenTls,
  type Listener,
  startTls,
} from "flux:net"
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
  expect(conn.resumed).toBe(false)
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
// Both serve any number of connections. A client may also reach the TLS
// listener directly on `tlsPort`: one listener, so one session cache.
async function starttlsServer() {
  let tls = await listenTls(0, { cert: SERVER, key: SERVER_KEY })
  let plain = await listen(0)
  ;(async () => {
    for await (let conn of plain) {
      ;(async () => {
        expect(await readText(conn)).toBe("STARTTLS\r\n")
        await write(conn, "OK\r\n")
        let upstream = await connect("127.0.0.1", portOf(tls.localAddr))
        conn.readable.pipeTo(upstream.writable).catch(() => {})
        upstream.readable.pipeTo(conn.writable).catch(() => {})
      })()
    }
  })()
  ;(async () => {
    for await (let conn of tls) {
      ;(async () => {
        let text = await readText(conn)
        await write(conn, `echo ${text}`)
      })()
    }
  })()
  return {
    port: portOf(plain.localAddr),
    tlsPort: portOf(tls.localAddr),
    close: () => (tls.close(), plain.close()),
  }
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

// A connection to `listener` as "localhost" that read one greeting from the
// server: on TLS 1.3 that read takes in the session the server sent.
async function greeted(listener: Listener, client: Parameters<typeof connectTls>[2]) {
  let accepted = accept(listener)
  let conn = await connectTls("localhost", portOf(listener.localAddr), client)
  let peer = await accepted
  await write(peer, "hello")
  expect(await readText(conn)).toBe("hello")
  return { conn, peer }
}

test("a second connectTls with equal options resumes the session", async () => {
  let listener = await listenTls(0, { cert: SERVER, key: SERVER_KEY })
  let first = await greeted(listener, { caCerts: [CA] })
  expect(first.conn.resumed).toBe(false)
  expect(first.peer.resumed).toBe(false)
  let second = await greeted(listener, { caCerts: [CA] })
  expect(second.conn.resumed).toBe(true)
  expect(second.peer.resumed).toBe(true)
  for (let { conn, peer } of [first, second]) (conn.close(), peer.close())
  listener.close()
})

test("startTls with the options of an earlier connectTls resumes its session", async () => {
  let server = await starttlsServer()
  let first = await connectTls("localhost", server.tlsPort, { caCerts: [CA] })
  await write(first, "first")
  expect(await readText(first)).toBe("echo first")
  expect(first.resumed).toBe(false)
  let conn = await connect("127.0.0.1", server.port)
  await write(conn, "STARTTLS\r\n")
  expect(await readText(conn)).toBe("OK\r\n")
  let secure = await startTls(conn, { hostname: "localhost", caCerts: [CA] })
  expect(secure.resumed).toBe(true)
  await write(secure, "second")
  expect(await readText(secure)).toBe("echo second")
  first.close()
  secure.close()
  server.close()
})

test("a connectTls with another alpn list does a full handshake", async () => {
  let listener = await listenTls(0, { cert: SERVER, key: SERVER_KEY, alpn: ["x-a", "x-b"] })
  let first = await greeted(listener, { caCerts: [CA], alpn: ["x-a"] })
  let second = await greeted(listener, { caCerts: [CA], alpn: ["x-b"] })
  expect(second.conn.alpnProtocol).toBe("x-b")
  expect(second.conn.resumed).toBe(false)
  expect(second.peer.resumed).toBe(false)
  for (let { conn, peer } of [first, second]) (conn.close(), peer.close())
  listener.close()
})

test("resume: false does a full handshake and leaves the sessions of the others", async () => {
  let listener = await listenTls(0, { cert: SERVER, key: SERVER_KEY })
  let first = await greeted(listener, { caCerts: [CA] })
  let second = await greeted(listener, { caCerts: [CA], resume: false })
  expect(second.conn.resumed).toBe(false)
  expect(second.peer.resumed).toBe(false)
  let third = await greeted(listener, { caCerts: [CA] })
  expect(third.conn.resumed).toBe(true)
  expect(third.peer.resumed).toBe(true)
  for (let { conn, peer } of [first, second, third]) (conn.close(), peer.close())
  listener.close()
})

test("resume: a Conn with equal options resumes", async () => {
  let listener = await listenTls(0, { cert: SERVER, key: SERVER_KEY })
  let first = await greeted(listener, { caCerts: [CA] })
  let second = await greeted(listener, { caCerts: [CA], resume: first.conn })
  expect(second.conn.resumed).toBe(true)
  expect(second.peer.resumed).toBe(true)
  for (let { conn, peer } of [first, second]) (conn.close(), peer.close())
  listener.close()
})

test("resume checks its Conn before it connects", async () => {
  let listener = await listenTls(0, { cert: SERVER, key: SERVER_KEY, alpn: ["x-a"] })
  let port = portOf(listener.localAddr)
  let first = await greeted(listener, { caCerts: [CA] })
  let plain = await connect("127.0.0.1", port)
  expect(() => connectTls("localhost", port, { caCerts: [CA], alpn: ["x-a"], resume: first.conn })).toThrow(
    "connectTls: resume: the connection's TLS options differ",
  )
  expect(() => connectTls("localhost", port, { caCerts: [CA], resume: plain })).toThrow(
    "connectTls: resume: not a TLS client connection",
  )
  expect(() => connectTls("localhost", port, { caCerts: [CA], resume: first.peer })).toThrow(
    "connectTls: resume: not a TLS client connection",
  )
  expect(() => connectTls("localhost", port, { caCerts: [CA], resume: 1 } as any)).toThrow(
    "connectTls: resume must be a boolean or a Conn",
  )
  expect(() =>
    startTls(plain, { hostname: "localhost", caCerts: [CA], alpn: ["x-a"], resume: first.conn }),
  ).toThrow("startTls: resume: the connection's TLS options differ")
  plain.close()
  first.conn.close()
  first.peer.close()
  listener.close()
})

// Both ends of a plain connection through listen().
async function plainPair() {
  let listener = await listen(0)
  let accepted = accept(listener)
  let conn = await connect("127.0.0.1", portOf(listener.localAddr))
  let peer = await accepted
  return { listener, conn, peer }
}

test("startTls upgrades both sides of a plain connection, the server with isServer", async () => {
  let { listener, conn, peer } = await plainPair()
  await write(conn, "STARTTLS\r\n")
  expect(await readText(peer)).toBe("STARTTLS\r\n")
  await write(peer, "OK\r\n")
  expect(await readText(conn)).toBe("OK\r\n")
  let [server, client] = await Promise.all([
    startTls(peer, { isServer: true, cert: SERVER, key: SERVER_KEY, anyClientCert: true }),
    startTls(conn, { hostname: "localhost", caCerts: [CA], cert: CLIENT, key: CLIENT_KEY }),
  ])
  expect(server.peerKey).toBe(CLIENT_PIN)
  expect(client.peerKey).not.toBe(null)
  await expect(write(peer, "late")).rejects.toThrow("connection is closed")
  await write(client, "up")
  expect(await readText(server)).toBe("up")
  await write(server, "down")
  expect(await readText(client)).toBe("down")
  client.close()
  server.close()
  listener.close()
})

test("startTls with isServer checks its options", async () => {
  let { listener, conn, peer } = await plainPair()
  expect(() => startTls(peer, { isServer: true } as any)).toThrow("startTls: cert and key are required with isServer")
  expect(() => startTls(peer, { isServer: true, cert: SERVER, key: SERVER_KEY, hostname: "x" } as any)).toThrow(
    "startTls: hostname is a client option",
  )
  expect(() => startTls(peer, { isServer: true, cert: SERVER, key: SERVER_KEY, pin: CLIENT_PIN } as any)).toThrow(
    "startTls: pin is a client option",
  )
  expect(() => startTls(peer, { isServer: true, cert: SERVER, key: SERVER_KEY, resume: false } as any)).toThrow(
    "startTls: resume is a client option",
  )
  expect(() =>
    startTls(peer, { isServer: true, cert: SERVER, key: SERVER_KEY, caCerts: [CA], anyClientCert: true }),
  ).toThrow("startTls: caCerts and anyClientCert exclude each other")
  expect(() => startTls(peer, { isServer: true, cert: "not a pem", key: SERVER_KEY })).toThrow("startTls: cert")
  conn.close()
  peer.close()
  listener.close()
})

test("a generated self-signed certificate serves https, trusted by its pin", async () => {
  let made = generateCertificate({ names: ["localhost"] })
  expect(made.cert).toContain("-----BEGIN CERTIFICATE-----")
  expect(made.key).toContain("-----BEGIN PRIVATE KEY-----")
  expect(/^[0-9a-f]{64}$/.test(made.pin)).toBe(true)
  let server = serve({ host: "127.0.0.1", tls: { cert: made.cert, key: made.key }, fetch: () => "hello" })
  let client = createHttpClient({ pin: made.pin })
  try {
    let res = await fetch(server.url, { client })
    expect(await res.text()).toBe("hello")
  } finally {
    server.close()
    client.close()
  }
})

test("a generated CA issues a certificate that a caCerts client trusts", async () => {
  let ca = generateCertificate({ names: ["test-ca"], isCa: true, validDays: 30 })
  let leaf = generateCertificate({ names: ["localhost", "127.0.0.1"], issuer: ca })
  let { listener, conn, peer } = await tlsPair({ cert: leaf.cert, key: leaf.key }, { caCerts: [ca.cert] })
  expect(conn.peerKey).toBe(leaf.pin)
  await write(conn, "ping")
  expect(await readText(peer)).toBe("ping")
  conn.close()
  peer.close()
  listener.close()
})

test("generateCertificate checks its options", () => {
  let ca = generateCertificate({ names: ["test-ca"], isCa: true })
  expect(() => generateCertificate({} as any)).toThrow("generateCertificate: names must be an array of strings")
  expect(() => generateCertificate({ names: [1] } as any)).toThrow("generateCertificate: names must be")
  expect(() => generateCertificate({ names: [] })).toThrow("generateCertificate: names: at least one")
  expect(() => generateCertificate({ names: ["localhost"], validDays: 0 })).toThrow(
    "generateCertificate: validDays must be a positive integer",
  )
  expect(() => generateCertificate({ names: ["localhost"], validDays: 1.5 })).toThrow("validDays must be")
  expect(() => generateCertificate({ names: ["localhost"], isCa: "yes" } as any)).toThrow("isCa must be a boolean")
  expect(() => generateCertificate({ names: ["localhost"], issuer: { cert: ca.cert } } as any)).toThrow(
    "generateCertificate: issuer must be",
  )
  expect(() => generateCertificate({ names: ["localhost"], issuer: { cert: "not a pem", key: ca.key } })).toThrow(
    "generateCertificate: issuer.cert",
  )
  expect(() => generateCertificate({ names: ["localhost"], issuer: { cert: ca.cert, key: "not a pem" } })).toThrow(
    "generateCertificate: issuer.key",
  )
})
