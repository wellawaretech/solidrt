// createHttpClient of flux:http: fetch through a client that trusts a
// private CA, presents a client certificate, or pins the server's key.
// The server is a flux:net listenTls answering one HTTP/1.1 request per
// connection, on loopback, with the forge fixtures.
import { expect, test } from "flux:test"
import { createHttpClient } from "flux:http"
import { type Conn, listenTls, type Listener } from "flux:net"
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
let portOf = (addr: string) => Number(addr.split(":").pop())

// Answer each connection's request with "ok", plus the client's key as a
// header, then close it.
async function respond(conn: Conn) {
  let reader = conn.readable.getReader()
  await reader.read()
  reader.releaseLock()
  let writer = conn.writable.getWriter()
  let head = `HTTP/1.1 200 OK\r\ncontent-length: 2\r\nx-peer-key: ${conn.peerKey ?? "none"}\r\nconnection: close\r\n\r\n`
  await writer.write(enc.encode(head + "ok"))
  await writer.close()
}

// An HTTPS server on 127.0.0.1 that answers every request, and its URL.
async function httpsServer(opts: Parameters<typeof listenTls>[1]): Promise<{ url: string; listener: Listener }> {
  let listener = await listenTls(0, { ...opts, host: "127.0.0.1", alpn: ["http/1.1"] })
  ;(async () => {
    for await (let conn of listener) respond(conn).catch(() => conn.close())
  })()
  return { url: `https://127.0.0.1:${portOf(listener.localAddr)}/`, listener }
}

test("a client with caCerts reaches a private-CA server, plain fetch does not", async () => {
  let { url, listener } = await httpsServer({ cert: SERVER, key: SERVER_KEY })
  let client = createHttpClient({ caCerts: [CA] })
  let res = await fetch(url, { client })
  expect(res.status).toBe(200)
  expect(await res.text()).toBe("ok")
  await expect(fetch(url)).rejects.toThrow("invalid peer certificate")
  client.close()
  listener.close()
})

test("mutual TLS presents the client certificate", async () => {
  let { url, listener } = await httpsServer({ cert: SERVER, key: SERVER_KEY, caCerts: [CA] })
  let client = createHttpClient({ caCerts: [CA], cert: CLIENT, key: CLIENT_KEY })
  let res = await fetch(url, { client })
  expect(res.headers.get("x-peer-key")).toBe(CLIENT_PIN)
  await res.text()
  client.close()
  listener.close()
})

test("a pin trusts a self-signed server by its key, and a wrong pin rejects", async () => {
  let { url, listener } = await httpsServer({ cert: SELF_SIGNED, key: SELF_SIGNED_KEY })
  let pinned = createHttpClient({ pin: SELF_SIGNED_PIN })
  expect(await (await fetch(url, { client: pinned })).text()).toBe("ok")
  let wrong = createHttpClient({ pin: CLIENT_PIN })
  await expect(fetch(url, { client: wrong })).rejects.toThrow("does not match the pin")
  pinned.close()
  wrong.close()
  listener.close()
})

test("a closed client rejects the fetch", async () => {
  let client = createHttpClient()
  client.close()
  await expect(fetch("https://127.0.0.1:1/", { client })).rejects.toThrow("the HttpClient is closed")
})

test("the options are checked when the client is made", () => {
  expect(() => createHttpClient({ pin: "abc" })).toThrow("createHttpClient: pin must be 64 hex characters")
  expect(() => createHttpClient({ caCerts: ["not a pem"] })).toThrow("createHttpClient: caCerts")
  expect(() => fetch("https://127.0.0.1:1/", { client: {} as any })).toThrow("client must be an HttpClient")
})
