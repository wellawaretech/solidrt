// The WebSocket constructor options: subprotocols, and a createHttpClient
// client for wss:// against serve({ tls }). Loopback only, with the forge
// fixtures (their README says how they were made).
import { expect, test } from "flux:test"
import { createHttpClient, serve } from "flux:http"
import { generateCertificate } from "flux:net"
import CA from "../../forge/src/tests/data/tls/ca.pem" with { type: "text" }
import SERVER from "../../forge/src/tests/data/tls/server.pem" with { type: "text" }
import SERVER_KEY from "../../forge/src/tests/data/tls/server-key.pem" with { type: "text" }

/** The pick of a server that answers a protocol no test offers. */
const UNOFFERED = "unoffered"

/**
 * A TLS server on 127.0.0.1 over `tls` that echoes text, and answers the
 * subprotocol offer (the request's `Sec-WebSocket-Protocol`) with `pick`:
 * by default the first protocol offered.
 */
function echoServer(
  tls: { cert: string; key: string },
  pick: (offered: string | null) => string | null = (offered) => offered?.split(",")[0]?.trim() ?? null,
) {
  return serve({
    host: "127.0.0.1",
    tls,
    fetch(req, server) {
      let picked = pick(req.headers.get("sec-websocket-protocol"))
      let headers: Record<string, string> = picked === null ? {} : { "Sec-WebSocket-Protocol": picked }
      if (server.upgrade(req, { headers })) return
      return "not a websocket"
    },
    websocket: {
      message(ws, message) {
        ws.send("echo: " + message)
      },
    },
  })
}

/**
 * Send `hello` once `ws` opens and close on the first message; the events
 * it saw, with `ws.protocol` at open.
 */
function exchange(ws: WebSocket): Promise<string[]> {
  let lines: string[] = []
  return new Promise((resolve) => {
    ws.onopen = () => {
      lines.push(`open: ${JSON.stringify(ws.protocol)}`)
      ws.send("hello")
    }
    ws.onmessage = (event) => {
      lines.push(`text: ${event.data}`)
      ws.close()
    }
    ws.onerror = (event) => lines.push(`error: ${event.message}`)
    ws.onclose = (event) => {
      lines.push(`close: ${event.code} ${event.wasClean}`)
      resolve(lines)
    }
  })
}

test("wss echoes through a client that trusts a private CA", async () => {
  let server = echoServer({ cert: SERVER, key: SERVER_KEY })
  let client = createHttpClient({ caCerts: [CA] })
  try {
    let ws = new WebSocket(`wss://localhost:${server.port}/echo`, { client })
    expect(ws.protocol).toBe("")
    expect(await exchange(ws)).toEqual(['open: ""', "text: echo: hello", "close: 1000 true"])
  } finally {
    server.close()
    client.close()
  }
})

test("wss echoes to a self-signed server through a pinned client", async () => {
  let made = generateCertificate({ names: ["localhost"] })
  let server = echoServer({ cert: made.cert, key: made.key })
  let client = createHttpClient({ pin: made.pin })
  try {
    // The pin replaces the name check: 127.0.0.1 is not in the certificate.
    let ws = new WebSocket(`wss://127.0.0.1:${server.port}/echo`, { client })
    expect(await exchange(ws)).toEqual(['open: ""', "text: echo: hello", "close: 1000 true"])
  } finally {
    server.close()
    client.close()
  }
})

test("the server picks a subprotocol and ws.protocol reports it", async () => {
  let server = echoServer({ cert: SERVER, key: SERVER_KEY })
  let client = createHttpClient({ caCerts: [CA] })
  try {
    let url = `wss://localhost:${server.port}/echo`
    let one = await exchange(new WebSocket(url, { protocols: "chat", client }))
    expect(one[0]).toBe('open: "chat"')
    let many = await exchange(new WebSocket(url, { protocols: ["json", "chat"], client }))
    expect(many[0]).toBe('open: "json"')
  } finally {
    server.close()
    client.close()
  }
})

test("the standard second argument offers subprotocols over ws://", async () => {
  let server = serve({
    host: "127.0.0.1",
    fetch(req, server) {
      let offered = req.headers.get("sec-websocket-protocol") ?? "none"
      if (server.upgrade(req, { headers: { "Sec-WebSocket-Protocol": offered.split(", ")[1] ?? "none" } })) return
      return "not a websocket"
    },
    websocket: { message: (ws, message) => ws.send("echo: " + message) },
  })
  try {
    let ws = new WebSocket(`ws://127.0.0.1:${server.port}/`, ["a", "b"])
    expect(await exchange(ws)).toEqual(['open: "b"', "text: echo: hello", "close: 1000 true"])
  } finally {
    server.close()
  }
})

test("a server that picks no subprotocol agrees on none", async () => {
  let server = echoServer({ cert: SERVER, key: SERVER_KEY }, () => null)
  let client = createHttpClient({ caCerts: [CA] })
  try {
    let ws = new WebSocket(`wss://localhost:${server.port}/echo`, { protocols: ["chat"], client })
    expect(await exchange(ws)).toEqual(['open: ""', "text: echo: hello", "close: 1000 true"])
  } finally {
    server.close()
    client.close()
  }
})

test("a pick the client did not offer fails the connection", async () => {
  let server = echoServer({ cert: SERVER, key: SERVER_KEY }, () => UNOFFERED)
  let client = createHttpClient({ caCerts: [CA] })
  try {
    let ws = new WebSocket(`wss://localhost:${server.port}/echo`, { protocols: ["chat"], client })
    expect(await exchange(ws)).toEqual([
      `error: connect failed: the server picked a subprotocol that was not offered: ${UNOFFERED}`,
      "close: 1006 false",
    ])
    expect(ws.protocol).toBe("")
    expect(ws.readyState).toBe(WebSocket.CLOSED)
  } finally {
    server.close()
    client.close()
  }
})

test("a closed client reports an error, then an unclean close", async () => {
  let server = echoServer({ cert: SERVER, key: SERVER_KEY })
  let client = createHttpClient({ caCerts: [CA] })
  client.close()
  try {
    let ws = new WebSocket(`wss://localhost:${server.port}/echo`, { client })
    expect(await exchange(ws)).toEqual(["error: WebSocket: the client is closed", "close: 1006 false"])
    expect(ws.readyState).toBe(WebSocket.CLOSED)
  } finally {
    server.close()
  }
})

test("the constructor checks its second argument", () => {
  let url = "ws://127.0.0.1:1/"
  expect(() => new WebSocket(url, "")).toThrow(SyntaxError)
  expect(() => new WebSocket(url, "a b")).toThrow("WebSocket: invalid subprotocol: a b")
  expect(() => new WebSocket(url, ["chat", "chat"])).toThrow("WebSocket: duplicate subprotocol: chat")
  expect(() => new WebSocket(url, { protocols: ["ok", "bad,token"] })).toThrow("invalid subprotocol")
  // @ts-expect-error: not a string, an array or an options object.
  expect(() => new WebSocket(url, 7)).toThrow(SyntaxError)
  // @ts-expect-error: an array of strings only.
  expect(() => new WebSocket(url, ["chat", 7])).toThrow(SyntaxError)
  // @ts-expect-error: the client must be an HttpClient.
  expect(() => new WebSocket(url, { client: {} })).toThrow("client must be an HttpClient")
})
