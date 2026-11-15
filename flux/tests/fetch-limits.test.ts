// The per-host fetch limits are the app's: the parent and an isolate share
// one cap and one 429 cooldown per host, and the cap is the app's to read
// and set (getHostLimit, setHostLimit), with a per-fetch opt-out (`slot:
// false`). The isolate module is flux/isolates/fetch-limits-worker.js, read
// through the assets mount.
import { expect, test } from "flux:test"
import { getHostLimit, serve, setHostLimit } from "flux:http"
import { isolate } from "flux:isolate"

// The slots of an HTTP/1.1 host (FETCHES_PER_HOST in flux).
const SLOTS = 6
// Fetches each side fires at once: more than the slots on its own.
const BURST = 10
// How long the server holds each request, so the two bursts overlap.
const HOLD_MS = 150
// The Retry-After the 429 carries, in seconds, and the pause it asks for.
const RETRY_AFTER_S = 1
const COOLDOWN_MS = RETRY_AFTER_S * 1000

// The exports of flux/isolates/fetch-limits-worker.js.
type Worker = {
  burst(url: string, n: number): Promise<number>
  post(url: string): Promise<number>
}

test("the parent and an isolate share the cap and the cooldown", async () => {
  let inFlight = 0
  let peak = 0
  // The server's receive times of the 429'd request and the one after it.
  let busyAt = 0
  let afterAt = 0
  let server = serve({
    host: "127.0.0.1",
    port: 0,
    routes: {
      "/slow": async () => {
        inFlight++
        peak = Math.max(peak, inFlight)
        await new Promise<void>((r) => setTimeout(() => r(), HOLD_MS))
        inFlight--
        return "ok"
      },
      "/busy": () => {
        busyAt = performance.now()
        return new Response("busy", { status: 429, headers: { "retry-after": String(RETRY_AFTER_S) } })
      },
      "/after": () => {
        afterAt = performance.now()
        return "ok"
      },
    },
  })
  let base = `http://127.0.0.1:${server.port}`
  let worker = isolate<Worker>("fetch-limits-worker")
  // The first call spawns the isolate (a thread, a runtime, the module's
  // evaluation), which can take longer than the parent's whole burst on a
  // loaded machine; an empty burst gets that out of the way, so the two
  // bursts below overlap and a peak of SLOTS is a witness of sharing, not
  // of the isolate arriving late.
  expect(await worker.burst(`${base}/slow`, 0)).toBe(0)

  let mine = Promise.all(Array.from({ length: BURST }, () => fetch(`${base}/slow`).then((res) => res.text())))
  let [theirs] = await Promise.all([worker.burst(`${base}/slow`, BURST), mine])
  expect(theirs).toBe(BURST)
  expect(peak).toBe(SLOTS)

  // The isolate's 429 pauses the parent's next fetch to the same host.
  expect(await worker.post(`${base}/busy`)).toBe(429)
  let res = await fetch(`${base}/after`)
  await res.text()
  expect(afterAt - busyAt).toBeGreaterThanOrEqual(COOLDOWN_MS)

  worker.terminate()
  server.close()
})

// Fetches fired at once against a capped host: more than the caps below.
const FEW = 3
// A cap below FEW, so a peak of it witnesses the cap.
const LOWERED = 2

// A server whose one route holds each request for HOLD_MS, counting the
// peak in flight.
function slowServer() {
  let inFlight = 0
  let stats = { peak: 0 }
  let server = serve({
    host: "127.0.0.1",
    port: 0,
    routes: {
      "/slow": async () => {
        inFlight++
        stats.peak = Math.max(stats.peak, inFlight)
        await new Promise<void>((r) => setTimeout(() => r(), HOLD_MS))
        inFlight--
        return "ok"
      },
    },
  })
  return { server, stats, url: `http://127.0.0.1:${server.port}/slow` }
}

// `n` fetches of `url` at once, each read to its end.
function burst(url: string, n: number, init?: RequestInit) {
  return Promise.all(Array.from({ length: n }, () => fetch(url, init).then((res) => res.text())))
}

test("the cap reads back, as the default and per host", () => {
  expect(getHostLimit()).toBe(SLOTS)
  expect(getHostLimit("example.com:443")).toBe(SLOTS)
  setHostLimit(LOWERED)
  expect(getHostLimit()).toBe(LOWERED)
  setHostLimit(Infinity, "https://example.com/x")
  expect(getHostLimit("example.com:443")).toBe(Infinity)
  expect(getHostLimit("http://example.com/")).toBe(LOWERED)
})

test("a lowered default caps the fetches to a host", async () => {
  let { server, stats, url } = slowServer()
  setHostLimit(LOWERED)
  await burst(url, FEW)
  expect(stats.peak).toBe(LOWERED)
  server.close()
})

test("an unlimited host takes no slot", async () => {
  let { server, stats, url } = slowServer()
  setHostLimit(1)
  setHostLimit(Infinity, url)
  await burst(url, FEW)
  expect(stats.peak).toBe(FEW)
  server.close()
})

test("a fetch with slot: false goes past the cap", async () => {
  let { server, stats, url } = slowServer()
  setHostLimit(1)
  await Promise.all([burst(url, 1), burst(url, FEW - 1, { slot: false })])
  expect(stats.peak).toBe(FEW)
  server.close()
})

test("the cap and the slot option are validated", () => {
  let badLimit = "setHostLimit: limit must be a positive integer within the runtime's maximum, or Infinity"
  expect(() => setHostLimit(0)).toThrow(badLimit)
  expect(() => setHostLimit(1.5)).toThrow(badLimit)
  expect(() => setHostLimit(-Infinity)).toThrow(badLimit)
  expect(() => setHostLimit(Number.MAX_VALUE)).toThrow(badLimit)
  expect(() => setHostLimit(NaN)).toThrow(badLimit)
  expect(() => setHostLimit("2" as any)).toThrow(badLimit)
  expect(() => (setHostLimit as any)()).toThrow(badLimit)
  expect(() => setHostLimit(2, "example.com")).toThrow('setHostLimit: host must be a "host:port" key or a URL')
  expect(() => getHostLimit("example.com:http")).toThrow('getHostLimit: host must be a "host:port" key or a URL')
  expect(() => getHostLimit(443 as any)).toThrow('getHostLimit: host must be a "host:port" key or a URL')
  expect(() => fetch("http://127.0.0.1:9/", { slot: "no" as any })).toThrow("fetch: slot must be a boolean")
  // A failed call leaves the cap as it was.
  expect(getHostLimit()).toBe(SLOTS)
})
