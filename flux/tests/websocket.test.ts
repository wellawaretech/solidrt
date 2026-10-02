// The WebSocket client, against flux's own serve.
import { expect, test } from "flux:test"
import { serve } from "flux:http"
import { listen } from "flux:net"

test("the client echoes text and binary and closes cleanly", async () => {
  let server = serve({
    port: 0,
    fetch(req, server) {
      if (server.upgrade(req)) return
      return "not a websocket"
    },
    websocket: {
      message(ws, message) {
        if (typeof message === "string") ws.send("echo: " + message)
        else ws.send(message)
      },
    },
  })
  try {
    let lines: string[] = []
    let ws = new WebSocket(`ws://127.0.0.1:${server.port}/chat`)
    lines.push(`connecting: ${ws.readyState} ${ws.url}`)
    // A send while connecting throws.
    expect(() => ws.send("too early")).toThrow()
    await new Promise<void>((resolve) => {
      ws.onopen = () => {
        lines.push(`open: ${ws.readyState}`)
        ws.send("hello")
      }
      ws.onmessage = (event) => {
        if (typeof event.data === "string") {
          lines.push(`text: ${event.data}`)
          ws.send(new Uint8Array([1, 2, 3]))
        } else {
          lines.push(`binary: ${event.data.length} ${event.data[0]} ${event.data[2]}`)
          ws.close(4100, "done")
        }
      }
      ws.onclose = (event) => {
        lines.push(`close: ${event.code} ${event.reason} ${event.wasClean} ${ws.readyState}`)
        resolve()
      }
    })
    expect(lines).toEqual([
      `connecting: 0 ws://127.0.0.1:${server.port}/chat`,
      "open: 1",
      "text: echo: hello",
      "binary: 3 1 3",
      "close: 4100 done true 3",
    ])
  } finally {
    server.close()
  }
})

test("a refused connection reports an error and an unclean 1006 close", async () => {
  // A port nothing listens on: bind one and release it.
  let listener = await listen(0)
  let port = Number(listener.localAddr.split(":").pop())
  listener.close()

  let lines: string[] = []
  let ws = new WebSocket(`ws://127.0.0.1:${port}/`)
  await new Promise<void>((resolve) => {
    ws.onerror = (event) => lines.push(`error: ${event.type} ${typeof event.message}`)
    ws.onclose = (event) => {
      lines.push(`close: ${event.code} ${event.wasClean} ${ws.readyState}`)
      resolve()
    }
  })
  expect(lines).toEqual(["error: error string", "close: 1006 false 3"])
})

test("the client rejects a bad url", () => {
  for (let url of ["wss://example.com/", "http://example.com/", "ws://"]) {
    expect(() => new WebSocket(url)).toThrow()
  }
})
