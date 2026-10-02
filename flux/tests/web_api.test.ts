// The web API objects (Headers, Request, Response, body, TextEncoder and
// TextDecoder) in isolation, with no server involved.
import { expect, test } from "flux:test"

test("Headers are case-insensitive and multi-valued", () => {
  let h = new Headers({ "Content-Type": "text/plain", "X-A": "1" })
  expect(h.get("content-type")).toBe("text/plain")
  expect(h.get("X-A")).toBe("1")
  expect(h.has("x-a")).toBe(true)
  expect(h.has("nope")).toBe(false)
  h.set("X-A", "2")
  expect(h.get("x-a")).toBe("2")
  h.append("X-A", "3")
  expect(h.get("x-a")).toBe("2, 3")
  h.delete("x-a")
  // A missing header is null (WHATWG Headers.get semantics).
  expect(h.get("x-a")).toBeNull()
  expect(h.get("missing")).toBeNull()
})

test("a Headers init copies an instance and rejects a non-string value", () => {
  let a = new Headers({ "X-A": "1" })
  let b = new Headers(a)
  a.set("X-A", "changed")
  // b copied a's entries at construction; a's later mutation stays in a.
  expect(b.get("x-a")).toBe("1")
  // A non-string value is a caller bug and throws (never stringified or
  // silently dropped).
  // @ts-expect-error a number is not a header value; the constructor has to say so at runtime too
  expect(() => new Headers({ "X-N": 5 })).toThrow("must be a string")
})

test("a buffered body iterates as one chunk", async () => {
  // A buffered body (`new Response("...")`) iterates like a streamed one: a
  // single Uint8Array chunk holding the whole payload. An empty body ends at
  // once, with no empty chunk.
  let dec = new TextDecoder()
  let chunks: string[] = []
  for await (let c of new Response("buffered body").body!) {
    chunks.push(c instanceof Uint8Array ? dec.decode(c) : typeof c)
  }
  expect(chunks).toEqual(["buffered body"])
  let empty = 0
  for await (let _ of new Response("").body!) empty++
  expect(empty).toBe(0)
})

test("the ArrayBuffer transfer family is removed", () => {
  // The vendored quickjs-ng transfer() corrupts externally backed buffers
  // (okf/upstream/quickjs-ng-transfer-external-buffers.md), so context setup
  // removes all three variants; ordinary ArrayBuffer use is untouched.
  let names = ["transfer", "transferToImmutable", "transferToFixedLength"]
  expect(names.map((n) => (ArrayBuffer.prototype as unknown as Record<string, unknown>)[n] === undefined)).toEqual([true, true, true])
  let buf = new ArrayBuffer(4)
  new Uint8Array(buf)[0] = 7
  expect(buf.byteLength).toBe(4)
  expect(new Uint8Array(buf)[0]).toBe(7)
})

test("a Response carries status, headers and body", async () => {
  let r = new Response("hello", { status: 201, statusText: "Created", headers: { "X-T": "v" } })
  expect([r.status, r.statusText, r.ok]).toEqual([201, "Created", true])
  expect(r.headers.get("x-t")).toBe("v")
  expect(await r.text()).toBe("hello")
})

test("a Response defaults to 200 and ok covers the 2xx range", () => {
  let ok = new Response()
  expect([ok.status, ok.ok]).toEqual([200, true])
  let bad = new Response("x", { status: 404 })
  expect([bad.status, bad.ok]).toEqual([404, false])
})

test("Response.json sets the content type", async () => {
  let r = Response.json({ a: 1, b: "two" })
  expect([r.status, r.headers.get("content-type")]).toEqual([200, "application/json"])
  expect(await r.json()).toEqual({ a: 1, b: "two" })
})

test("a Request uppercases the method and carries a body", async () => {
  let req = new Request("http://x/y", { method: "post", body: "data", headers: { "X-H": "h" } })
  expect([req.method, req.url]).toEqual(["POST", "http://x/y"])
  expect(req.headers.get("x-h")).toBe("h")
  expect(await req.text()).toBe("data")
})

test("a Request body reads as JSON", async () => {
  let req = new Request("http://x", { method: "POST", body: JSON.stringify({ n: 5 }) })
  expect(await req.json()).toEqual({ n: 5 })
})

test("a body is consumed once", async () => {
  let r = new Response("once")
  expect(await r.text()).toBe("once")
  // The second read throws on the call (not as a rejection), with the
  // message as a bare string.
  expect(() => r.text()).toThrow(/^Body already consumed$/)
})

test("a Response rejects an invalid body type", () => {
  // @ts-expect-error a number is not a body; the constructor has to say so at runtime too
  expect(() => new Response(123)).toThrow("must be string")
})

test("TextEncoder encodes UTF-8", () => {
  let enc = new TextEncoder()
  expect(enc.encoding).toBe("utf-8")
  // "A" is one byte; the euro sign is three (E2 82 AC = 226,130,172).
  let bytes = enc.encode("A€")
  expect(bytes.length).toBe(4)
  expect(Array.from(bytes)).toEqual([65, 226, 130, 172])
})

test("TextDecoder streams a split multibyte sequence", () => {
  let bytes = new TextEncoder().encode("a€b") // 61, E2 82 AC, 62
  let dec = new TextDecoder()
  // Split mid euro-sign: the first chunk ends one byte into it.
  let p1 = dec.decode(bytes.slice(0, 2), { stream: true })
  let p2 = dec.decode(bytes.slice(2), { stream: true })
  expect(p1 + p2).toBe("a€b")
  expect(dec.encoding).toBe("utf-8")
})

test("TextDecoder replaces an invalid byte, or throws when fatal", () => {
  // Non-fatal: an invalid byte becomes the replacement char U+FFFD.
  expect(new TextDecoder().decode(new Uint8Array([0xff]))).toBe("�")
  // Fatal: the same input throws instead.
  expect(() => new TextDecoder("utf-8", { fatal: true }).decode(new Uint8Array([0xff]))).toThrow()
})

test("TextDecoder strips a BOM by default and takes utf-8 labels only", () => {
  let bom = new Uint8Array([0xef, 0xbb, 0xbf, 0x68, 0x69]) // BOM + "hi"
  // A leading BOM is stripped by default.
  expect(new TextDecoder().decode(bom)).toBe("hi")
  // ...and kept with ignoreBOM (length 3: U+FEFF, h, i).
  expect(new TextDecoder("utf-8", { ignoreBOM: true }).decode(bom).length).toBe(3)
  // A non-utf-8 label is rejected.
  expect(() => new TextDecoder("utf-16")).toThrow("utf-8")
})
