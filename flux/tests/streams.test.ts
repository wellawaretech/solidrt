// The web streams (ReadableStream, WritableStream, TransformStream and the
// text streams) and the bodies that are made of them.
import { expect, test } from "flux:test"
import { serve, type ServeOptions } from "flux:http"

// Serve `options` on a free loopback port for the duration of `run`, which
// gets the base URL to fetch from.
async function serving(options: ServeOptions, run: (base: string) => Promise<void>) {
  let server = serve({ port: 0, ...options })
  try {
    await run(`http://127.0.0.1:${server.port}`)
  } finally {
    server.close()
  }
}

// A stream of the given chunks, closed after the last one.
function streamOf<T>(...chunks: T[]): ReadableStream<T> {
  return new ReadableStream<T>({
    start(controller) {
      for (let chunk of chunks) controller.enqueue(chunk)
      controller.close()
    },
  })
}

// Every chunk of `stream`, in order.
async function collect<T>(stream: ReadableStream<T>): Promise<T[]> {
  let out: T[] = []
  for await (let chunk of stream) out.push(chunk)
  return out
}

function upper(): TransformStream<Uint8Array, Uint8Array> {
  let dec = new TextDecoder()
  let enc = new TextEncoder()
  return new TransformStream({
    transform(chunk, controller) {
      controller.enqueue(enc.encode(dec.decode(chunk).toUpperCase()))
    },
  })
}

test("a reader locks the stream, reads it to the end and releases it", async () => {
  let stream = streamOf("a", "b")
  expect(stream.locked).toBe(false)
  let reader = stream.getReader()
  expect(stream.locked).toBe(true)
  // One reader at a time, as on the web.
  expect(() => stream.getReader()).toThrow("locked")
  expect(await reader.read()).toEqual({ value: "a", done: false })
  expect(await reader.read()).toEqual({ value: "b", done: false })
  expect(await reader.read()).toEqual({ value: undefined, done: true })
  await reader.closed
  reader.releaseLock()
  expect(stream.locked).toBe(false)
  // Released: the stream takes a new reader, and the old one reads nothing.
  let again = stream.getReader()
  expect(await again.read()).toEqual({ value: undefined, done: true })
  await expect(reader.read()).rejects.toThrow("released")
})

test("the source is pulled as the reader asks, up to the high-water mark", async () => {
  let pulls = 0
  let stream = new ReadableStream<number>({
    pull(controller) {
      pulls++
      controller.enqueue(pulls)
    },
  })
  // The default high-water mark queues one chunk ahead of the reader.
  await Promise.resolve()
  expect(pulls).toBe(1)
  let reader = stream.getReader()
  expect((await reader.read()).value).toBe(1)
  expect((await reader.read()).value).toBe(2)
  // Each read refills the queue to the mark, no further.
  expect(pulls).toBeLessThanOrEqual(3)
})

test("cancel runs the source's cancel with the reason and ends pending reads", async () => {
  let cancelled: unknown
  let stream = new ReadableStream({
    cancel(reason) {
      cancelled = reason
    },
  })
  let reader = stream.getReader()
  let pending = reader.read()
  await reader.cancel("enough")
  expect(cancelled).toBe("enough")
  expect(await pending).toEqual({ value: undefined, done: true })
  // A locked stream cannot be cancelled from outside its reader.
  let locked = streamOf(1)
  locked.getReader()
  await expect(locked.cancel()).rejects.toThrow("locked")
})

test("an errored source rejects reads and the closed promise", async () => {
  let stream = new ReadableStream({
    start(controller) {
      controller.enqueue("first")
      controller.error(new Error("source broke"))
    },
  })
  let reader = stream.getReader()
  // Chunks queued before the error are gone: an error empties the queue.
  await expect(reader.read()).rejects.toThrow("source broke")
  await expect(reader.closed).rejects.toThrow("source broke")
})

test("a writer sees backpressure through write() and ready", async () => {
  let written: string[] = []
  let release: (() => void) | undefined
  let stream = new WritableStream<string>({
    write(chunk) {
      written.push(chunk)
      // The sink accepts the chunk only when the test lets it.
      return new Promise<void>((resolve) => {
        release = resolve
      })
    },
  })
  let writer = stream.getWriter()
  expect(writer.desiredSize).toBe(1)
  let first = writer.write("one")
  // The chunk is queued: the writer is at its mark, ready is pending.
  expect(writer.desiredSize).toBe(0)
  let settled = false
  first.then(() => (settled = true))
  await Promise.resolve()
  await Promise.resolve()
  expect(written).toEqual(["one"])
  expect(settled).toBe(false)
  release!()
  await first
  expect(settled).toBe(true)
  expect(writer.desiredSize).toBe(1)
  await writer.ready
  // A second writer waits for the first to let go.
  expect(() => stream.getWriter()).toThrow("locked")
  writer.releaseLock()
  expect(stream.locked).toBe(false)
})

test("close runs the sink's close after the queued writes", async () => {
  let events: string[] = []
  let stream = new WritableStream<string>({
    async write(chunk) {
      await Promise.resolve()
      events.push("write " + chunk)
    },
    close() {
      events.push("close")
    },
  })
  let writer = stream.getWriter()
  writer.write("a")
  writer.write("b")
  await writer.close()
  expect(events).toEqual(["write a", "write b", "close"])
  await writer.closed
  await expect(writer.write("c")).rejects.toThrow("closing or closed")
})

test("abort runs the sink's abort after the write in flight, and rejects the queued writes", async () => {
  let aborted: Error | undefined
  let release: (() => void) | undefined
  let stream = new WritableStream<string>({
    write() {
      return new Promise<void>((resolve) => {
        release = resolve
      })
    },
    abort(reason) {
      aborted = reason
    },
  })
  let writer = stream.getWriter()
  let inFlight = writer.write("in flight")
  // The sink has started and taken the first write.
  await Promise.resolve()
  await Promise.resolve()
  let queued = writer.write("behind")
  let abort = writer.abort(new Error("stop"))
  await expect(writer.ready).rejects.toThrow("stop")
  // The write in flight is not interrupted: the sink's abort waits for it.
  expect(aborted).toBe(undefined)
  release!()
  await inFlight
  await abort
  expect(aborted!.message).toBe("stop")
  await expect(queued).rejects.toThrow("stop")
  await expect(writer.closed).rejects.toThrow("stop")
})

test("a TransformStream maps chunks and flushes at close", async () => {
  let doubled = new TransformStream<number, number>({
    transform(chunk, controller) {
      controller.enqueue(chunk * 2)
    },
    flush(controller) {
      controller.enqueue(-1)
    },
  })
  let out = collect(doubled.readable)
  let writer = doubled.writable.getWriter()
  await writer.write(1)
  await writer.write(2)
  await writer.close()
  expect(await out).toEqual([2, 4, -1])
  // Without a transform, chunks pass through unchanged.
  expect(await collect(streamOf("x", "y").pipeThrough(new TransformStream()))).toEqual(["x", "y"])
})

test("a write into a transform waits for the reader", async () => {
  let transformed = 0
  let ts = new TransformStream<number, number>({
    transform(chunk, controller) {
      transformed++
      controller.enqueue(chunk)
    },
  })
  let writer = ts.writable.getWriter()
  let write = writer.write(1)
  let settled = false
  write.then(() => (settled = true))
  await Promise.resolve()
  await Promise.resolve()
  // Nobody reads: the readable side has no room, the chunk waits unmapped.
  expect([transformed, settled]).toEqual([0, false])
  let reader = ts.readable.getReader()
  expect(await reader.read()).toEqual({ value: 1, done: false })
  await write
  expect([transformed, settled]).toEqual([1, true])
})

test("tee gives both branches every chunk and cancels the source once both are cancelled", async () => {
  let cancelled: unknown
  let source = new ReadableStream<string>({
    start(controller) {
      controller.enqueue("a")
      controller.enqueue("b")
      controller.close()
    },
    cancel(reason) {
      cancelled = reason
    },
  })
  let [left, right] = source.tee()
  expect(source.locked).toBe(true)
  expect(await collect(left)).toEqual(["a", "b"])
  expect(await collect(right)).toEqual(["a", "b"])

  let live = new ReadableStream({
    cancel(reason) {
      cancelled = reason
    },
  })
  let [one, two] = live.tee()
  // A branch's cancel settles only once the source is cancelled, which
  // takes both branches (the spec's composite cancel).
  let first = one.cancel("first")
  await Promise.resolve()
  expect(cancelled).toBe(undefined)
  await two.cancel("second")
  await first
  expect(cancelled).toEqual(["first", "second"])
})

test("an error in the source aborts the destination of a pipe chain", async () => {
  let aborted: unknown
  let source = new ReadableStream({
    start(controller) {
      controller.enqueue("ok")
      controller.error(new Error("upstream"))
    },
  })
  let sink = new WritableStream({
    abort(reason) {
      aborted = reason
    },
  })
  let pipe = source.pipeThrough(new TransformStream()).pipeTo(sink)
  await expect(pipe).rejects.toThrow("upstream")
  expect((aborted as Error).message).toBe("upstream")
  expect(sink.locked).toBe(false)
})

test("cancelling the readable end of a pipe chain cancels the source", async () => {
  let cancelled: unknown
  let source = new ReadableStream({
    pull(controller) {
      controller.enqueue("more")
    },
    cancel(reason) {
      cancelled = reason
    },
  })
  let end = source.pipeThrough(new TransformStream())
  let reader = end.getReader()
  expect((await reader.read()).value).toBe("more")
  await reader.cancel(new Error("done reading"))
  // The transform's writable side errors with the reason, which fails the
  // pipe, cancels the source behind it and releases it.
  while (source.locked) await new Promise<void>((resolve) => setTimeout(() => resolve(), 1))
  expect((cancelled as Error).message).toBe("done reading")
})

test("an error in the destination cancels the source", async () => {
  let cancelled: unknown
  let source = new ReadableStream({
    pull(controller) {
      controller.enqueue("x")
    },
    cancel(reason) {
      cancelled = reason
    },
  })
  let sink = new WritableStream({
    write() {
      throw new Error("sink broke")
    },
  })
  await expect(source.pipeTo(sink)).rejects.toThrow("sink broke")
  expect((cancelled as Error).message).toBe("sink broke")
})

test("pipeTo options and BYOB readers are refused", () => {
  let stream = streamOf(1)
  let sink = new WritableStream()
  expect(() => stream.pipeTo(sink, { preventClose: true } as never)).toThrow("preventClose")
  expect(() => (stream as { getReader(options: unknown): unknown }).getReader({ mode: "byob" })).toThrow("byob")
  expect(() => new ReadableStream({ type: "bytes" } as never)).toThrow("bytes")
  expect(stream.locked).toBe(false)
})

test("ReadableStream.from pulls one item per read and returns the iterator on cancel", async () => {
  let pulled = 0
  let returned = false
  async function* items() {
    try {
      while (true) {
        pulled++
        yield pulled
      }
    } finally {
      returned = true
    }
  }
  let stream = ReadableStream.from(items())
  let reader = stream.getReader()
  expect((await reader.read()).value).toBe(1)
  expect((await reader.read()).value).toBe(2)
  // Nothing is pulled ahead of a read.
  expect(pulled).toBe(2)
  await reader.cancel()
  expect(returned).toBe(true)
  expect(await collect(ReadableStream.from(["s", "t"]))).toEqual(["s", "t"])
})

test("a body is a ReadableStream, iterable with for await", async () => {
  let dec = new TextDecoder()
  let body = new Response("buffered body").body
  expect(body instanceof ReadableStream).toBe(true)
  let chunks: string[] = []
  for await (let c of body) chunks.push(dec.decode(c))
  expect(chunks).toEqual(["buffered body"])
  let req = new Request("http://x", { method: "POST", body: "upload" })
  expect(req.body instanceof ReadableStream).toBe(true)
  expect((await collect(req.body)).length).toBe(1)
})

test("a Response over a stream is drained by text() and served as it is produced", async () => {
  let enc = new TextEncoder()
  let r = new Response(streamOf<Uint8Array | string>(enc.encode("stream"), "ed"))
  expect(r.body instanceof ReadableStream).toBe(true)
  expect(await r.text()).toBe("streamed")
  // Drained once: the body is consumed, as a buffered one is.
  expect(() => r.text()).toThrow(/^Body already consumed$/)
  expect(() => r.body).toThrow(/^Body already consumed$/)
  // An async generator body is wrapped: the same shape, drained the same way.
  async function* parts() {
    yield "gen"
    yield "erated"
  }
  let g = new Response(parts())
  expect(g.body instanceof ReadableStream).toBe(true)
  expect(await g.text()).toBe("generated")
  // A stream that errors rejects the read.
  let broken = new Response(
    new ReadableStream({
      start(controller) {
        controller.error(new Error("body broke"))
      },
    }),
  )
  await expect(broken.text()).rejects.toThrow("body broke")
})

test("a fetched body piped through a TransformStream serves as a streamed Response", async () => {
  await serving(
    {
      async fetch(req) {
        if (req.url === "/source") return new Response(streamOf("hello, ", "streams"))
        // A proxy that transforms what it relays, as a Response over the
        // piped body; and one that relays a fetched Response as it is.
        let upstream = await fetch(`http://127.0.0.1:${req.headers.get("x-port")}/source`)
        if (req.url === "/relay") return upstream
        return new Response(upstream.body.pipeThrough(upper()))
      },
    },
    async (base) => {
      let port = base.slice(base.lastIndexOf(":") + 1)
      let r = await fetch(base + "/upper", { headers: { "X-Port": port } })
      expect(await r.text()).toBe("HELLO, STREAMS")
      let relayed = await fetch(base + "/relay", { headers: { "X-Port": port } })
      expect(await relayed.text()).toBe("hello, streams")
    },
  )
})

test("a ReadableStream is a fetch request body, read incrementally by the server", async () => {
  await serving(
    {
      async fetch(req) {
        let dec = new TextDecoder()
        let text = ""
        for await (let chunk of req.body) text += dec.decode(chunk, { stream: true })
        return new Response("got:" + text + dec.decode())
      },
    },
    async (base) => {
      let r = await fetch(base + "/upload", { method: "POST", body: streamOf("stream", "ed ", "upload") })
      expect(await r.text()).toBe("got:streamed upload")
    },
  )
})

test("ending a for await early cancels a fetched body", async () => {
  await serving(
    {
      fetch() {
        return new Response(
          new ReadableStream({
            pull(controller) {
              controller.enqueue("endless ")
            },
          }),
        )
      },
    },
    async (base) => {
      let r = await fetch(base + "/")
      let seen = 0
      for await (let _ of r.body) {
        if (++seen === 3) break
      }
      expect(seen).toBe(3)
      // The reader is released and the stream cancelled: the body is done.
      expect(r.body.locked).toBe(false)
      expect(await collect(r.body)).toEqual([])
    },
  )
})

test("TextDecoderStream decodes a UTF-8 sequence split across chunks", async () => {
  let bytes = new TextEncoder().encode("a\u20acb") // 61, E2 82 AC, 62
  let decoded = streamOf(bytes.slice(0, 2), bytes.slice(2, 3), bytes.slice(3)).pipeThrough(new TextDecoderStream())
  expect((await collect(decoded)).join("")).toBe("a\u20acb")
  let ds = new TextDecoderStream()
  expect([ds.encoding, ds.fatal, ds.ignoreBOM]).toEqual(["utf-8", false, false])
  expect(() => new TextDecoderStream("utf-16")).toThrow("utf-8")
})

test("TextEncoderStream encodes strings, holding a split surrogate pair", async () => {
  let encoded = streamOf("a", "\ud83d", "\ude00", "b").pipeThrough(new TextEncoderStream())
  let chunks = await collect(encoded)
  let all = new Uint8Array(chunks.reduce((n, c) => n + c.length, 0))
  let at = 0
  for (let c of chunks) {
    all.set(c, at)
    at += c.length
  }
  expect(new TextDecoder().decode(all)).toBe("a\u{1f600}b")
  // The pair split across two chunks encodes as one 4-byte sequence.
  expect(all.length).toBe(6)
})
