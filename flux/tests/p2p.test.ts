// flux:p2p: two local endpoints on this machine (no relay, no discovery),
// one stream between them, and the readable/writable pair over it: the echo,
// a write after the peer went away, and an abort the peer's read reports;
// then the same pair through a relay served here, and an endpoint with no
// relay that reaches a peer on the relay its ticket names; then the relay's
// options: tokens, host, allowlist (over HTTPS too), clients, the URL of an
// HTTPS relay, its certificate renewed in place, and the validation throws;
// then endpoints that trust an HTTPS
// relay by the pin in its URL, with relayCaCerts beside it, and the relay
// URLs they refuse; then the relays an endpoint knows, a ticket naming a
// known relay with a new token, connect without adopting (a known relay is
// used still, a ticket left with no way to dial is refused before the
// dial), the local address beside a loopback relay, and a ticket without
// the relay (refused on a relay-only endpoint) and the addresses it carries.
import { expect, test } from "flux:test"
import { Endpoint, parseTicket, serveRelay, type P2pStream } from "flux:p2p"
import { generateCertificate } from "flux:net"

const PROTOCOL = "flux-test/1"
// A write on a connection the peer closed fails once the close has been
// processed here; the retries bound how long that may take.
const GONE_RETRIES = 50
const GONE_RETRY_MS = 100
// How long the relay may take to register or drop a client, in polls: 3 s,
// well inside the 5 s a test may take.
const CLIENT_POLLS = 30
const CLIENT_POLL_MS = 100
// Relay tokens long enough for the relay's minimum.
const TOKEN_A = "first-token-0123456789"
const TOKEN_B = "second-token-0123456789"

let sleep = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms))

// Two endpoints and a stream between them: `a` dialled and wrote the
// greeting, `b` accepted and read it. The dialer writes first because a
// QUIC stream reaches the acceptor with its first bytes.
const GREETING = "hello"

async function pair() {
  let server = await Endpoint.create({ protocols: [PROTOCOL] })
  let client = await Endpoint.create()
  let accepted = server.accept(PROTOCOL)[Symbol.asyncIterator]().next()
  let a = await client.connect(await server.ticket(), PROTOCOL)
  let writer = a.writable.getWriter()
  await writer.write(GREETING)
  writer.releaseLock()
  let b = (await accepted).value as P2pStream
  let reader = b.readable.getReader()
  expect(new TextDecoder().decode((await reader.read()).value)).toBe(GREETING)
  reader.releaseLock()
  return { server, client, a, b }
}

test("a stream pair echoes over readable and writable", async () => {
  let { server, client, a, b } = await pair()
  expect(b.remoteId).toBe(client.id)
  expect(server.relayStatus()).toBe(null)
  let writer = a.writable.getWriter()
  await writer.write("ping")
  await writer.close()
  // The half-close arrives: b reads to end-of-stream, then answers.
  expect(await new Response(b.readable).text()).toBe("ping")
  let back = b.writable.getWriter()
  await back.write("pong")
  await back.close()
  expect(await new Response(a.readable).text()).toBe("pong")
  a.close()
  b.close()
  await client.close()
  await server.close()
})

test("a write after the peer went away rejects", async () => {
  let { server, client, a, b } = await pair()
  b.close()
  await server.close()
  let writer = a.writable.getWriter()
  let failed = false
  for (let i = 0; i < GONE_RETRIES && !failed; i++) {
    try {
      await writer.write("still there?")
      await sleep(GONE_RETRY_MS)
    } catch {
      failed = true
    }
  }
  expect(failed).toBe(true)
  a.close()
  await client.close()
})

test("aborting the writable resets the stream for the peer and errors this side", async () => {
  let { server, client, a, b } = await pair()
  let peerRead = b.readable.getReader().read()
  await a.writable.abort(new Error("gave up"))
  await expect(peerRead).rejects.toThrow("reset")
  await expect(a.readable.getReader().read()).rejects.toThrow("gave up")
  b.close()
  await client.close()
  await server.close()
})

// Both endpoints name the relay served here (its URL carries the token it
// admits), and the dialer gets the server's ticket, which names the relay
// and no direct address, so its first packets have no path but the relay; iroh finds
// the direct path on its own after that (both ends are this machine), which
// is why the relay address is checked for presence, not for being the
// active path. That the relay refuses an endpoint without its token is a
// forge test (forge/src/tests/p2p_relay.rs), and so is a relay nobody serves.
test("two endpoints meet through a relay served here", async () => {
  let relay = await serveRelay()
  expect(relay.port).toBeGreaterThan(0)
  expect(relay.url.startsWith("http://")).toBe(true)
  let server = await Endpoint.create({ protocols: [PROTOCOL], relayUrl: relay.url })
  let client = await Endpoint.create({ relayUrl: relay.url })
  let ticket = await server.ticket()
  let parts = parseTicket(ticket)
  expect(parts.relayUrl).toBe(relay.url)
  expect(parts.addresses).toEqual([])
  expect(parseTicket(await server.ticket({ addresses: "all" })).addresses.length).toBeGreaterThan(0)
  let status = server.relayStatus()
  expect(status?.connected).toBe(true)
  expect(status?.url.includes("@")).toBe(false)
  expect(status?.error).toBe(undefined)
  let accepted = server.accept(PROTOCOL)[Symbol.asyncIterator]().next()
  let a = await client.connect(ticket, PROTOCOL)
  let writer = a.writable.getWriter()
  await writer.write(GREETING)
  await writer.close()
  let b = (await accepted).value as P2pStream
  expect(await new Response(b.readable).text()).toBe(GREETING)
  let info = await server.connInfo(client.id)
  expect(info.addrs.some(entry => entry.kind === "relay")).toBe(true)
  a.close()
  b.close()
  await client.close()
  await server.close()
  await relay.close()
})

// Scan and connect: Alice has a relay, Bob has none and only her ticket,
// which names the relay and no direct address, so his dial has no path but
// the ticket's relay, which he adopts with its token. Afterwards his own
// ticket and status still name no relay, whatever iroh chose as his home
// relay.
test("an endpoint without a relay connects through the relay a ticket names", async () => {
  let relay = await serveRelay()
  let alice = await Endpoint.create({ protocols: [PROTOCOL], relayUrl: relay.url })
  let bob = await Endpoint.create()
  let ticket = await alice.ticket()
  expect(parseTicket(ticket).relayUrl).toBe(relay.url)
  let accepted = alice.accept(PROTOCOL)[Symbol.asyncIterator]().next()
  let a = await bob.connect(ticket, PROTOCOL)
  let writer = a.writable.getWriter()
  await writer.write(GREETING)
  await writer.close()
  let b = (await accepted).value as P2pStream
  expect(b.remoteId).toBe(bob.id)
  expect(await new Response(b.readable).text()).toBe(GREETING)
  expect(parseTicket(await bob.ticket()).relayUrl).toBe(null)
  expect(bob.relayStatus()).toBe(null)
  // iroh makes the adopted relay home: relays() reports it.
  for (let i = 0; i < CLIENT_POLLS && !bob.relays()[0]?.home; i++) await sleep(CLIENT_POLL_MS)
  expect(bob.relays()).toEqual([{ url: relay.url.replace(/\/\/[^@]*@/, "//"), configured: false, home: true }])
  a.close()
  b.close()
  await bob.close()
  await alice.close()
  await relay.close()
})

test("parseTicket splits a ticket and keeps the relay token", () => {
  let id = "116072673e30625053b0a67b3b8abf3de89f0a7aa99737b259f802423a1cf9ab"
  let relayUrl = "http://secret@127.0.0.1:3340/"
  let parts = parseTicket(`${id}|${relayUrl}|127.0.0.1:4000,10.0.0.2:4001`)
  expect(parts.id).toBe(id)
  expect(parts.relayUrl).toBe(relayUrl)
  expect(parts.addresses).toEqual(["127.0.0.1:4000", "10.0.0.2:4001"])
  expect(parseTicket(id).relayUrl).toBe(null)
  expect(() => parseTicket("not-an-id||")).toThrow("Invalid endpoint id")
  expect(() => parseTicket(`${id}|${relayUrl}|127.0.0.1:4000|extra`)).toThrow("more than 3 fields")
})

// ticket() waits 3 s for the relay before it rejects, inside the 5 s a test
// may take: one call only.
test("ticket rejects when the relay refuses the token", async () => {
  let relay = await serveRelay()
  let endpoint = await Endpoint.create({ relayUrl: relay.url.replace(/\/\/[^@]*@/, "//wrong-token@") })
  await expect(endpoint.ticket()).rejects.toThrow("Authentication denied")
  let status = endpoint.relayStatus()
  expect(status?.connected).toBe(false)
  expect(status?.url.includes("wrong-token")).toBe(false)
  expect(typeof status?.error).toBe("string")
  await endpoint.close()
  await relay.close()
})

// Poll `relay.clients()` until it has `count` entries.
async function clientsReach(relay: { clients(): string[] }, count: number) {
  for (let i = 0; i < CLIENT_POLLS && relay.clients().length !== count; i++) await sleep(CLIENT_POLL_MS)
  return relay.clients()
}

test("a relay admits its second token, lists its clients and drops a revoked one", async () => {
  let relay = await serveRelay({ host: "127.0.0.1", token: [TOKEN_A, TOKEN_B] })
  expect(relay.url).toBe(`http://${TOKEN_A}@127.0.0.1:${relay.port}/`)
  let endpoint = await Endpoint.create({ relayUrl: relay.url.replace(TOKEN_A, TOKEN_B) })
  await endpoint.ticket()
  expect(await clientsReach(relay, 1)).toEqual([endpoint.id])
  relay.setAllow([])
  expect(await clientsReach(relay, 0)).toEqual([])
  relay.setAllow(null)
  await endpoint.close()
  await relay.close()
})

test("setAllow throws on a malformed id and without an argument", async () => {
  let relay = await serveRelay({ host: "127.0.0.1" })
  let malformed = () => relay.setAllow(["not-an-id"])
  expect(malformed).toThrow("setAllow: invalid endpoint id")
  try {
    malformed()
  } catch (e) {
    expect(String(e).includes("setAllow: setAllow:")).toBe(false)
  }
  expect(() => Reflect.apply(relay.setAllow, relay, [])).toThrow("setAllow")
  expect(() => relay.setAllow(undefined as never)).toThrow("setAllow")
  relay.setAllow(null)
  await relay.close()
})

// The allowlist over a self-signed HTTPS relay: the key is made first so
// its id can go into `allow`; replacing the list drops the live client. A
// denied id's ticket() waits 3 s before it rejects, so the denial is a forge
// test (forge/src/tests/p2p_relay.rs).
test("an HTTPS relay admits an allowed id and setAllow drops it", async () => {
  let keyed = await Endpoint.create()
  let secretKey = keyed.secretKey
  let id = keyed.id
  await keyed.close()
  let made = generateCertificate({ names: ["127.0.0.1"] })
  let relay = await serveRelay({ host: "127.0.0.1", tls: { cert: made.cert, key: made.key }, allow: [id] })
  let endpoint = await Endpoint.create({ secretKey, relayUrl: relay.url })
  await endpoint.ticket()
  expect(await clientsReach(relay, 1)).toEqual([id])
  relay.setAllow([])
  expect(await clientsReach(relay, 0)).toEqual([])
  await endpoint.close()
  await relay.close()
})

test("an HTTPS relay's URL carries the pin of its self-signed certificate", async () => {
  let made = generateCertificate({ names: ["127.0.0.1"] })
  let relay = await serveRelay({ host: "127.0.0.1", tls: { cert: made.cert, key: made.key } })
  expect(relay.url.startsWith("https://")).toBe(true)
  expect(relay.url.endsWith(`:${relay.port}/?pin=${made.pin}`)).toBe(true)
  await relay.close()
  let unpinned = await serveRelay({ host: "127.0.0.1", tls: { cert: made.cert, key: made.key, pinned: false } })
  expect(unpinned.url.includes("pin=")).toBe(false)
  await unpinned.close()
  // A generateCertificate result passes as it is; its pin string is not read.
  let direct = await serveRelay({ host: "127.0.0.1", tls: made })
  expect(direct.url.endsWith(`:${direct.port}/?pin=${made.pin}`)).toBe(true)
  await direct.close()
  let proxied = await serveRelay({ token: TOKEN_A, url: "https://relay.example.test/" })
  expect(proxied.url).toBe(`https://${TOKEN_A}@relay.example.test/`)
  await proxied.close()
})

// A CA-signed relay is unpinned, so it renews to any key: here a leaf of
// another CA. The endpoint connected before stays connected; one that
// trusts only the new CA reaches the renewed relay. The URL stays.
test("setCertificate renews an HTTPS relay's certificate and keeps its url", async () => {
  let oldCa = generateCertificate({ names: ["old-ca.test"], isCa: true })
  let newCa = generateCertificate({ names: ["new-ca.test"], isCa: true })
  let relay = await serveRelay({
    host: "127.0.0.1",
    tls: generateCertificate({ names: ["127.0.0.1"], issuer: oldCa }),
  })
  let url = relay.url
  expect(url.includes("pin=")).toBe(false)
  let before = await Endpoint.create({ relayUrl: url, relayCaCerts: [oldCa.cert] })
  await before.ticket()
  expect(await clientsReach(relay, 1)).toEqual([before.id])
  relay.setCertificate(generateCertificate({ names: ["127.0.0.1"], issuer: newCa }))
  expect(relay.url).toBe(url)
  let after = await Endpoint.create({ relayUrl: url, relayCaCerts: [newCa.cert] })
  expect(parseTicket(await after.ticket()).relayUrl).toBe(url)
  expect(relay.clients()).toEqual([before.id, after.id].sort())
  await after.close()
  await before.close()
  await relay.close()
})

test("setCertificate keeps a pinned relay's key and needs tls", async () => {
  let made = generateCertificate({ names: ["127.0.0.1"] })
  let pinned = await serveRelay({ host: "127.0.0.1", tls: made })
  let url = pinned.url
  expect(() => pinned.setCertificate(generateCertificate({ names: ["127.0.0.1"] }))).toThrow(
    "setCertificate: cert: another key than the pin in this relay's url names",
  )
  expect(() => pinned.setCertificate({ ...made, pinned: false } as never)).toThrow("pinned stays as the server")
  pinned.setCertificate(made)
  expect(pinned.url).toBe(url)
  await pinned.close()
  let plain = await serveRelay({ host: "127.0.0.1" })
  expect(() => plain.setCertificate(made)).toThrow("setCertificate: a plain HTTP relay has no certificate")
  await plain.close()
})

test("serveRelay throws on an invalid option", () => {
  let made = generateCertificate({ names: ["127.0.0.1"] })
  expect(() => serveRelay({ token: "short" })).toThrow("token")
  expect(() => serveRelay({ token: "not/url-safe-0123456789" })).toThrow("token")
  expect(() => serveRelay({ token: [] })).toThrow("token")
  expect(() => serveRelay({ host: "not a host" })).toThrow("host")
  expect(() => serveRelay({ allow: ["not-an-id"] })).toThrow("allow")
  expect(() => serveRelay({ url: "http://relay.example.test/?pin=00" })).toThrow("url")
  expect(() => serveRelay({ url: "ftp://relay.example.test/" })).toThrow("url")
  expect(() => serveRelay({ tls: { cert: made.cert, key: made.key, caCerts: [made.cert] } as never })).toThrow(
    "caCerts",
  )
  expect(() => serveRelay({ tls: { cert: made.cert, key: made.key, anyClientCert: true } as never })).toThrow(
    "anyClientCert",
  )
  expect(() => serveRelay({ tls: { cert: made.cert } as never })).toThrow("cert and key are required")
  expect(() => serveRelay({ tls: { cert: made.cert, key: made.key }, url: "http://relay.example.test/" })).toThrow(
    "url",
  )
  expect(() => serveRelay({ host: "::1", tls: { cert: made.cert, key: made.key } })).toThrow("IPv6")
  expect(() =>
    serveRelay({ tls: { cert: made.cert, key: made.key, pinned: false }, url: "http://relay.example.test/" }),
  ).toThrow("url: a tls relay needs an https:// url")
  expect(() => serveRelay({ url: "https://relay.example.test/a|b" })).toThrow("url")
})

// Scan and connect against a self-signed HTTPS relay: Alice names it by its
// URL, pin included; Bob has no relay and only her ticket, cut to the relay
// (a loopback relay's ticket carries the local address too), so he adopts
// the relay with its token and pin and checks its key.
test("an endpoint without a relay connects through a pinned HTTPS relay a ticket names", async () => {
  let made = generateCertificate({ names: ["127.0.0.1"] })
  let relay = await serveRelay({ host: "127.0.0.1", tls: { cert: made.cert, key: made.key } })
  let alice = await Endpoint.create({ protocols: [PROTOCOL], relayUrl: relay.url })
  let ticket = await alice.ticket()
  expect(parseTicket(ticket).relayUrl).toBe(relay.url)
  expect(parseTicket(ticket).addresses.length).toBeGreaterThan(0)
  let status = alice.relayStatus()
  expect(status?.connected).toBe(true)
  expect(status?.url).toBe(`https://127.0.0.1:${relay.port}/`)
  let bob = await Endpoint.create()
  let accepted = alice.accept(PROTOCOL)[Symbol.asyncIterator]().next()
  let a = await bob.connect(relayOnlyTicket(ticket), PROTOCOL)
  let writer = a.writable.getWriter()
  await writer.write(GREETING)
  await writer.close()
  let b = (await accepted).value as P2pStream
  expect(b.remoteId).toBe(bob.id)
  expect(await new Response(b.readable).text()).toBe(GREETING)
  a.close()
  b.close()
  await bob.close()
  await alice.close()
  await relay.close()
})

test("a pinned relayUrl works beside relayCaCerts", async () => {
  let ca = generateCertificate({ names: ["relay-ca.test"], isCa: true })
  let made = generateCertificate({ names: ["127.0.0.1"] })
  let relay = await serveRelay({ host: "127.0.0.1", tls: { cert: made.cert, key: made.key } })
  let endpoint = await Endpoint.create({ relayUrl: relay.url, relayCaCerts: [ca.cert] })
  expect(parseTicket(await endpoint.ticket()).relayUrl).toBe(relay.url)
  await endpoint.close()
  await relay.close()
})

test("Endpoint.create rejects a pin on http, a malformed pin, a password, a fragment and bad relayCaCerts", async () => {
  let made = generateCertificate({ names: ["127.0.0.1"] })
  await expect(Endpoint.create({ relayUrl: `http://${TOKEN_A}@127.0.0.1:1/?pin=${made.pin}` })).rejects.toThrow(
    "https://",
  )
  await expect(Endpoint.create({ relayUrl: "https://127.0.0.1:1/?pin=00" })).rejects.toThrow("pin must be")
  await expect(Endpoint.create({ relayUrl: "https://127.0.0.1:1/?quic=7842" })).rejects.toThrow("unknown query")
  await expect(Endpoint.create({ relayCaCerts: ["not pem"] })).rejects.toThrow("relayCaCerts")
  let password = await Endpoint.create({ relayUrl: "https://sekrit:hunter2@127.0.0.1:1/" }).then(
    () => "",
    (e) => String(e),
  )
  expect(password.includes("password")).toBe(true)
  expect(password.includes("sekrit") || password.includes("hunter2")).toBe(false)
  await expect(Endpoint.create({ relayUrl: "https://127.0.0.1:1/#x" })).rejects.toThrow("fragment")
  await expect(Endpoint.create({ relayUrl: `wss://${TOKEN_A}@127.0.0.1:1/` })).rejects.toThrow("not http or https")
  await expect(Endpoint.create({ relayUrl: `http://${TOKEN_A}@127.0.0.1:1/a|b` })).rejects.toThrow(
    "separates a ticket's fields",
  )
  await expect(Endpoint.create({ relayUrl: `https://${TOKEN_A}@[::1]:1/` })).rejects.toThrow("IPv6")
  await (await Endpoint.create({ relayUrl: `http://${TOKEN_A}@[::1]:1/` })).close()
  let id = "116072673e30625053b0a67b3b8abf3de89f0a7aa99737b259f802423a1cf9ab"
  expect(() => parseTicket(`${id}|http://127.0.0.1:1/?pin=${made.pin}|`)).toThrow("https://")
})

// A certificate that fails the check: ticket() rejects with the reason
// after its 3 s wait (one call per test), and relayStatus() carries it.
test("ticket rejects with the certificate error on a wrong pin", async () => {
  let made = generateCertificate({ names: ["127.0.0.1"] })
  let other = generateCertificate({ names: ["127.0.0.1"] })
  let relay = await serveRelay({ host: "127.0.0.1", tls: { cert: made.cert, key: made.key } })
  let endpoint = await Endpoint.create({ relayUrl: relay.url.replace(made.pin, other.pin) })
  await expect(endpoint.ticket()).rejects.toThrow("does not match the pin")
  let status = endpoint.relayStatus()
  expect(status?.connected).toBe(false)
  expect(status?.error?.includes("does not match the pin")).toBe(true)
  await endpoint.close()
  await relay.close()
})

test("ticket rejects with the certificate error without the relay's CA", async () => {
  let ca = generateCertificate({ names: ["relay-ca.test"], isCa: true })
  let made = generateCertificate({ names: ["127.0.0.1"], issuer: { cert: ca.cert, key: ca.key } })
  let relay = await serveRelay({ host: "127.0.0.1", tls: { cert: made.cert, key: made.key } })
  let endpoint = await Endpoint.create({ relayUrl: relay.url })
  await expect(endpoint.ticket()).rejects.toThrow("invalid peer certificate")
  expect(endpoint.relayStatus()?.error?.includes("invalid peer certificate")).toBe(true)
  await endpoint.close()
  await relay.close()
})

// Accept every stream on `endpoint` until it closes.
async function acceptAll(endpoint: Endpoint) {
  for await (let stream of endpoint.accept(PROTOCOL)) stream.close()
}

// `ticket` cut to `id|relay|`: no direct address, so the relay is the only
// way to the peer.
function relayOnlyTicket(ticket: string) {
  return ticket.slice(0, ticket.lastIndexOf("|") + 1)
}

// Dial and write the greeting, so the peer sees the stream.
async function dial(from: Endpoint, ticket: string, opts?: { adoptRelay?: boolean }) {
  let stream = await from.connect(ticket, PROTOCOL, opts)
  let writer = stream.writable.getWriter()
  await writer.write(GREETING)
  writer.releaseLock()
  return stream
}

// Bob adopts a relay from Alice's ticket. The relay restarts on its port
// with a new token: a new ticket names the same relay with the new token,
// which Bob refuses (he keeps the token he adopted it with). The relay is on
// loopback, so its tickets carry the local address too. Carol does not
// adopt the relay: the ticket cut to the relay is refused before the dial,
// the ticket as written reaches Alice directly.
test("a known relay keeps its token, and adoptRelay: false adopts nothing", async () => {
  let relay = await serveRelay({ host: "127.0.0.1", token: TOKEN_A })
  let port = relay.port
  let bare = `http://127.0.0.1:${port}/`
  let alice = await Endpoint.create({ protocols: [PROTOCOL], relayUrl: relay.url })
  // Cut to the relay, so the dials below can only land through it.
  let first = relayOnlyTicket(await alice.ticket())
  expect(alice.relays()).toEqual([{ url: bare, configured: true, home: true }])
  let accepting = acceptAll(alice)
  let bob = await Endpoint.create()
  expect(bob.relays()).toEqual([])
  ;(await dial(bob, first)).close()
  expect(bob.relays().map((r) => [r.url, r.configured])).toEqual([[bare, false]])
  // A relay Bob knows is used without adopting.
  ;(await dial(bob, first, { adoptRelay: false })).close()
  expect(bob.relays().length).toBe(1)
  // The relay goes first: Alice's close would otherwise wait about 3 s on
  // Bob's relayed connections.
  await relay.close()
  await alice.close()
  await accepting

  let restarted = await serveRelay({ host: "127.0.0.1", port, token: TOKEN_B })
  let alice2 = await Endpoint.create({ protocols: [PROTOCOL], relayUrl: restarted.url })
  let ticket = await alice2.ticket()
  let refused = bob.connect(ticket, PROTOCOL)
  await expect(refused).rejects.toThrow(`Relay ${bare}: the ticket carries another token`)
  await expect(refused).rejects.toThrow("new endpoint")
  expect(parseTicket(ticket).addresses.length).toBeGreaterThan(0)
  let carol = await Endpoint.create()
  let unreachable = await carol.connect(relayOnlyTicket(ticket), PROTOCOL, { adoptRelay: false }).then(
    () => "",
    (e) => String(e),
  )
  expect(unreachable).toContain(
    `Relay ${bare}: not known to this endpoint and adoptRelay is false, and the ticket carries no direct address`,
  )
  expect(unreachable.includes(TOKEN_B)).toBe(false)
  let accepting2 = acceptAll(alice2)
  ;(await dial(carol, ticket, { adoptRelay: false })).close()
  expect(carol.relays()).toEqual([])
  await carol.close()
  await bob.close()
  await alice2.close()
  await accepting2
  await restarted.close()
})

// A relay nobody serves: ticket({ relay: false }) does not wait for it and
// writes the default-route IPv4 alone; addresses: "all" writes every direct
// address, and another addresses value throws. A relay-only endpoint has none, so it
// rejects at once.
test("ticket without the relay is direct-only, and refused on a relay-only endpoint", async () => {
  let endpoint = await Endpoint.create({ relayUrl: `http://${TOKEN_A}@127.0.0.1:1/` })
  let parts = parseTicket(await endpoint.ticket({ relay: false }))
  expect(parts.relayUrl).toBe(null)
  expect(parts.addresses.length).toBeGreaterThan(0)
  // The default-route IPv4 (or every IPv4 without a default route), never
  // more than addresses: "all" writes, and no IPv6.
  let all = parseTicket(await endpoint.ticket({ relay: false, addresses: "all" })).addresses
  expect(parts.addresses.every((addr) => all.includes(addr) && !addr.startsWith("["))).toBe(true)
  expect(() => endpoint.ticket({ addresses: "lan" as "all" })).toThrow('addresses must be "auto" or "all", not "lan"')
  expect(endpoint.relayStatus()?.connected).toBe(false)
  expect(endpoint.relays()).toEqual([{ url: "http://127.0.0.1:1/", configured: true, home: false }])
  await endpoint.close()
  let relayOnlyEndpoint = await Endpoint.create({ relayUrl: `http://${TOKEN_A}@127.0.0.1:1/`, paths: "relay" })
  await expect(relayOnlyEndpoint.ticket({ relay: false })).rejects.toThrow("paths \"relay\"")
  await relayOnlyEndpoint.close()
})
