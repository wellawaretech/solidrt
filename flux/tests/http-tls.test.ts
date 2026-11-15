// flux:http serving HTTPS: serve({ tls }) fetched through a createHttpClient
// client that trusts the private CA, the client certificate's key through
// server.peerKey, and the tls option errors. Loopback only, with the forge
// fixtures (their README says how they were made).
import { expect, test } from "flux:test"
import { createHttpClient, serve } from "flux:http"
import CA from "../../forge/src/tests/data/tls/ca.pem" with { type: "text" }
import SERVER from "../../forge/src/tests/data/tls/server.pem" with { type: "text" }
import SERVER_KEY from "../../forge/src/tests/data/tls/server-key.pem" with { type: "text" }
import CLIENT from "../../forge/src/tests/data/tls/client.pem" with { type: "text" }
import CLIENT_KEY from "../../forge/src/tests/data/tls/client-key.pem" with { type: "text" }
import CLIENT_PIN from "../../forge/src/tests/data/tls/client.pin" with { type: "text" }

test("serve with tls answers https through a private CA", async () => {
  let server = serve({
    host: "127.0.0.1",
    tls: { cert: SERVER, key: SERVER_KEY },
    fetch: (req) => `${req.url} ${req.headers.get("host")}`,
  })
  let client = createHttpClient({ caCerts: [CA] })
  try {
    expect(server.url).toBe(`https://127.0.0.1:${server.port}/`)
    // The client offers h2 first, so this is an HTTP/2 request: the
    // handler still sees the path and a Host header, as over HTTP/1.1.
    let res = await fetch(server.url + "x?y=1", { client })
    expect(res.status).toBe(200)
    expect(await res.text()).toBe(`/x?y=1 127.0.0.1:${server.port}`)
  } finally {
    server.close()
    client.close()
  }
})

test("routes match over https", async () => {
  let server = serve({
    host: "127.0.0.1",
    tls: { cert: SERVER, key: SERVER_KEY },
    routes: { "/users/:id": (req) => "user " + req.params.id },
  })
  let client = createHttpClient({ caCerts: [CA] })
  try {
    let res = await fetch(server.url + "users/7", { client })
    expect(await res.text()).toBe("user 7")
  } finally {
    server.close()
    client.close()
  }
})

test("peerKey is the client certificate's key with anyClientCert", async () => {
  let server = serve({
    host: "127.0.0.1",
    tls: { cert: SERVER, key: SERVER_KEY, anyClientCert: true },
    fetch: (req, srv) => srv.peerKey(req) ?? "none",
  })
  let client = createHttpClient({ caCerts: [CA], cert: CLIENT, key: CLIENT_KEY })
  try {
    let res = await fetch(server.url, { client })
    expect(await res.text()).toBe(CLIENT_PIN)
  } finally {
    server.close()
    client.close()
  }
})

test("peerKey is null on a plain server and for a constructed Request", async () => {
  let server = serve({ host: "127.0.0.1", fetch: (req, srv) => String(srv.peerKey(req)) })
  try {
    let res = await fetch(server.url)
    expect(await res.text()).toBe("null")
    expect(server.peerKey(new Request("http://localhost/"))).toBe(null)
  } finally {
    server.close()
  }
})

test("bad tls options throw before anything binds", () => {
  expect(() => serve({ tls: { cert: SERVER, key: SERVER_KEY, caCerts: [CA], anyClientCert: true } })).toThrow(
    "exclude each other",
  )
  expect(() => serve({ tls: { cert: "not a pem", key: SERVER_KEY } })).toThrow("serve: tls: cert")
})
