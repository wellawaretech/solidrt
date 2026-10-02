// flux:http: serve and fetch against each other over loopback. Every test
// serves on port 0 and closes its server at the end.
import { expect, test } from "flux:test"
import { serve, type ServeOptions } from "flux:http"

// Serve `options` on a free loopback port for the duration of `run`, which
// gets the base URL to fetch from.
async function serving(options: ServeOptions, run: (base: string, server: ReturnType<typeof serve>) => Promise<void>) {
  let server = serve({ port: 0, ...options })
  try {
    await run(`http://127.0.0.1:${server.port}`, server)
  } finally {
    server.close()
  }
}

test("serve and fetch round trip", async () => {
  await serving(
    {
      async fetch(req) {
        if (req.url === "/json") return Response.json({ ok: true, where: req.url })
        if (req.url === "/custom") {
          return new Response("made", { status: 418, headers: { "X-Brewed-By": "flux" } })
        }
        if (req.url === "/echo") {
          let body = await req.text()
          return new Response(req.method + ":" + body + ":" + (req.headers.get("x-demo") || "none"))
        }
        return "hello"
      },
    },
    async (base) => {
      // A string return is a 200 text/plain.
      let r1 = await fetch(base + "/")
      expect([r1.status, r1.ok, r1.headers.get("content-type")]).toEqual([200, true, "text/plain"])
      expect(await r1.text()).toBe("hello")

      // Response.json is a 200 application/json, parsed back to an object.
      let r2 = await fetch(base + "/json")
      expect([r2.status, r2.headers.get("content-type")]).toEqual([200, "application/json"])
      expect(await r2.json()).toEqual({ ok: true, where: "/json" })

      // A custom status and header round-trip; 418 resolves to its canonical
      // reason phrase and ok is false (not 2xx).
      let r3 = await fetch(base + "/custom")
      expect([r3.status, r3.statusText, r3.ok, r3.headers.get("x-brewed-by")]).toEqual([418, "I'm a teapot", false, "flux"])
      expect(await r3.text()).toBe("made")

      // A POST body and a request header reach the handler; the method is
      // uppercased.
      let r4 = await fetch(base + "/echo", { method: "POST", body: "hi", headers: { "X-Demo": "abc" } })
      expect(await r4.text()).toBe("POST:hi:abc")

      // A Headers instance works as the headers option (its entries live in
      // Rust, so it must be recognized, not iterated as a plain object).
      let r5 = await fetch(base + "/echo", { method: "POST", body: "hi", headers: new Headers({ "X-Demo": "inst" }) })
      expect(await r5.text()).toBe("POST:hi:inst")
    },
  )
})

test("fetch rejects an unsupported header or body value", () => {
  // Both throw at the call site (caller bugs, not environmental failures),
  // before any network activity, so no server is needed.
  // @ts-expect-error a plain object is no body; fetch has to say so at runtime too
  expect(() => fetch("http://127.0.0.1:9/", { method: "POST", body: { a: 1 } })).toThrow("Fetch body")
  // @ts-expect-error a number is no header value; fetch has to say so at runtime too
  expect(() => fetch("http://127.0.0.1:9/", { headers: { "X-N": 5 } })).toThrow("must be a string")
})

test("serve returns a handle and stops", async () => {
  let server = serve({ port: 0, fetch: () => "up" })
  // The handle exposes Bun-like introspection: the port is the bound one,
  // the host is what we bind, the url is derived.
  expect(server.port).toBeGreaterThan(0)
  expect(server.host).toBe("0.0.0.0")
  expect(server.url).toBe(`http://0.0.0.0:${server.port}/`)
  let base = `http://127.0.0.1:${server.port}`

  let r1 = await fetch(base + "/")
  expect(r1.status).toBe(200)
  expect(await r1.text()).toBe("up")

  server.close()
  // Let the accept loop drop the listener and the connections drain.
  await new Promise<void>((r) => setTimeout(() => r(), 200))

  // After close() the listener is closed and the pooled keep-alive
  // connection was gracefully shut down, so a fresh dial is refused.
  await expect(fetch(base + "/").then((r) => r.text())).rejects.toThrow()
})

test("serve picks a free port when none is given", async () => {
  let server = serve({ host: "127.0.0.1", fetch: () => "up" })
  try {
    // The OS picks one, and the handle reports the real one.
    expect(server.port).not.toBe(0)
    expect(server.url).toBe(`http://127.0.0.1:${server.port}/`)
    let r = await fetch(server.url)
    expect(r.status).toBe(200)
    expect(await r.text()).toBe("up")
  } finally {
    server.close()
  }
})

test("serve honors the host", async () => {
  await serving({ host: "127.0.0.1", fetch: () => "up" }, async (base, server) => {
    // The configured host is reflected back, not the "0.0.0.0" default.
    expect(server.host).toBe("127.0.0.1")
    expect(server.url).toBe(`http://127.0.0.1:${server.port}/`)
    let r = await fetch(base + "/")
    expect(r.status).toBe(200)
    expect(await r.text()).toBe("up")
  })
})

test("serve routes a thrown or rejected fetch through error()", async () => {
  await serving(
    {
      fetch(req) {
        if (req.url === "/throw") throw new Error("boom")
        if (req.url === "/reject") return Promise.reject(new Error("async boom"))
        return "ok"
      },
      // Receives the thrown value; its returned Response (status included)
      // is sent instead of the default 500.
      error(err) {
        return new Response("handled: " + err.message, { status: 502 })
      },
    },
    async (base) => {
      // A sync throw routes through error(); the custom status passes through.
      let r1 = await fetch(base + "/throw")
      expect([r1.status, await r1.text()]).toEqual([502, "handled: boom"])
      // A rejected promise reaches error() the same way.
      let r2 = await fetch(base + "/reject")
      expect([r2.status, await r2.text()]).toEqual([502, "handled: async boom"])
      // A successful request is untouched by error().
      let r3 = await fetch(base + "/")
      expect([r3.status, await r3.text()]).toEqual([200, "ok"])
    },
  )
})

test("serve answers a thrown fetch with a 500 without error()", async () => {
  await serving(
    {
      fetch() {
        throw new Error("nope")
      },
    },
    async (base) => {
      let r = await fetch(base + "/")
      expect([r.status, await r.text()]).toEqual([500, "Internal Server Error"])
    },
  )
})

test("serve passes the server handle to fetch", async () => {
  await serving(
    {
      host: "127.0.0.1",
      // The second arg is the Server handle: the same introspection as the
      // value serve() returned.
      fetch: (_req, srv) => srv.url + " port=" + srv.port,
    },
    async (base, server) => {
      let r = await fetch(base + "/")
      expect(await r.text()).toBe(`http://127.0.0.1:${server.port}/ port=${server.port}`)
    },
  )
})

test("serve routes", async () => {
  await serving(
    {
      routes: {
        "/": () => "root",
        "/version": Response.json({ v: 1 }), // static Response
        "/users/me": () => "me", // exact beats :id
        "/users/:id": (req) => "user " + req.params.id, // path param
        "/files/*": () => "wild", // trailing wildcard
      },
      fetch: (req) => "fallback " + req.url,
    },
    async (base) => {
      expect(await (await fetch(base + "/")).text()).toBe("root")
      // A static Response keeps its content type and body across requests.
      let v = await fetch(base + "/version")
      expect(v.headers.get("content-type")).toBe("application/json")
      expect(await v.json()).toEqual({ v: 1 })
      // The exact "/users/me" wins over "/users/:id".
      expect(await (await fetch(base + "/users/me")).text()).toBe("me")
      // :id is captured into req.params.
      expect(await (await fetch(base + "/users/42")).text()).toBe("user 42")
      // "/files/*" matches the remaining segments.
      expect(await (await fetch(base + "/files/a/b/c")).text()).toBe("wild")
      // An unmatched path falls through to fetch.
      expect(await (await fetch(base + "/nope")).text()).toBe("fallback /nope")
    },
  )
})

test("serve decodes route params", async () => {
  await serving(
    {
      routes: {
        "/users/:id": (req) => req.params.id!,
        "/files/:name": (req) => req.params.name!,
      },
    },
    async (base) => {
      // %20 is a space.
      expect(await (await fetch(base + "/users/john%20doe")).text()).toBe("john doe")
      // %2F stays inside the param value; still one segment matching :name.
      expect(await (await fetch(base + "/files/a%2Fb")).text()).toBe("a/b")
    },
  )
})

test("serve answers an unmatched route with a 404 without fetch", async () => {
  await serving({ routes: { "/hit": () => "hit" } }, async (base) => {
    let r1 = await fetch(base + "/hit")
    expect([r1.status, await r1.text()]).toEqual([200, "hit"])
    let r2 = await fetch(base + "/miss")
    expect([r2.status, await r2.text()]).toEqual([404, "Not Found"])
  })
})

test("serve routes per method", async () => {
  // A route value can be a per-method object; the request method picks the
  // handler, and an unlisted method is a 405 with an Allow header.
  await serving(
    {
      routes: {
        "/api": {
          GET: () => "got",
          POST: () => "posted",
        },
      },
    },
    async (base) => {
      expect(await (await fetch(base + "/api")).text()).toBe("got")
      expect(await (await fetch(base + "/api", { method: "POST" })).text()).toBe("posted")
      // An unlisted method is a 405 with Allow listing the registered
      // methods in order.
      let d = await fetch(base + "/api", { method: "DELETE" })
      expect([d.status, d.headers.get("allow"), await d.text()]).toEqual([405, "GET, POST", "Method Not Allowed"])
    },
  )
})

async function* chunks() {
  yield "Hello, "
  yield "streamed "
  yield "world"
}

test("serve streams an async iterable body", async () => {
  // A handler can return a Response whose body is an async generator; each
  // yielded chunk is streamed to the client (chunked transfer encoding).
  await serving(
    {
      fetch() {
        return new Response(chunks(), { headers: { "Content-Type": "text/plain" } })
      },
    },
    async (base) => {
      let r = await fetch(base + "/")
      expect(r.headers.get("content-type")).toBe("text/plain")
      // The client reassembles the streamed chunks into the full body.
      expect(await r.text()).toBe("Hello, streamed world")
    },
  )
})

test("serve receives a streamed request body", async () => {
  // fetch can stream a request body from an async generator; the server
  // collects the chunked body and sees the full payload.
  async function* parts() {
    yield "strea"
    yield "med "
    yield "request"
  }
  await serving(
    {
      async fetch(req) {
        return new Response(req.method + ":" + (await req.text()))
      },
    },
    async (base) => {
      let r = await fetch(base + "/upload", { method: "POST", body: parts() })
      expect(await r.text()).toBe("POST:streamed request")
    },
  )
})

test("fetch iterates a streamed response body", async () => {
  // The server streams a chunked response; the client consumes it lazily by
  // iterating `response.body` (an async iterable of Uint8Array chunks),
  // reassembling the payload as the chunks arrive.
  await serving(
    {
      fetch() {
        return new Response(chunks())
      },
    },
    async (base) => {
      let r = await fetch(base + "/")
      let dec = new TextDecoder()
      let text = ""
      for await (let chunk of r.body) {
        text += dec.decode(chunk, { stream: true })
      }
      text += dec.decode()
      expect(text).toBe("Hello, streamed world")
    },
  )
})

test("serve iterates a streamed request body", async () => {
  // The client streams a request body; the server reads it incrementally by
  // iterating `req.body` (an async iterable of Uint8Array chunks) rather
  // than the framework buffering the whole upload up front.
  async function* parts() {
    yield "incre"
    yield "mental "
    yield "upload"
  }
  await serving(
    {
      async fetch(req) {
        let dec = new TextDecoder()
        let text = ""
        for await (let chunk of req.body) {
          text += dec.decode(chunk, { stream: true })
        }
        text += dec.decode()
        return new Response("got:" + text)
      },
    },
    async (base) => {
      let r = await fetch(base + "/upload", { method: "POST", body: parts() })
      expect(await r.text()).toBe("got:incremental upload")
    },
  )
})
