// The compression streams: CompressionStream and DecompressionStream over
// gzip, deflate (zlib) and deflate-raw.
import { expect, test } from "flux:test"
import { serve, type ServeOptions } from "flux:http"

// The most bytes one piece of output holds (forge::compression's
// OUTPUT_CHUNK_BYTES): what the cap test measures against.
const OUTPUT_CHUNK_BYTES = 64 * 1024

const TEXT = "hello, compression streams"
// TEXT as gzip (no header fields), as gzip with a file name field, and as
// zlib, made by other tools.
const GZIP = Uint8Array.from([
  31, 139, 8, 0, 0, 0, 0, 0, 2, 255, 203, 72, 205, 201, 201, 215, 81, 72, 206, 207, 45, 40, 74, 45, 46, 206, 204, 207,
  83, 40, 46, 41, 74, 77, 204, 45, 6, 0, 158, 95, 108, 197, 26, 0, 0, 0,
])
const GZIP_NAMED = Uint8Array.from([
  31, 139, 8, 8, 0, 0, 0, 0, 2, 255, 104, 101, 108, 108, 111, 46, 116, 120, 116, 0, 203, 72, 205, 201, 201, 215, 81, 72,
  206, 207, 45, 40, 74, 45, 46, 206, 204, 207, 83, 40, 46, 41, 74, 77, 204, 45, 6, 0, 158, 95, 108, 197, 26, 0, 0, 0,
])
const ZLIB = Uint8Array.from([
  120, 156, 203, 72, 205, 201, 201, 215, 81, 72, 206, 207, 45, 40, 74, 45, 46, 206, 204, 207, 83, 40, 46, 41, 74, 77,
  204, 45, 6, 0, 134, 166, 10, 50,
])

const FORMATS: CompressionFormat[] = ["gzip", "deflate", "deflate-raw"]

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

function concat(chunks: Uint8Array[]): Uint8Array {
  let all = new Uint8Array(chunks.reduce((n, c) => n + c.length, 0))
  let at = 0
  for (let c of chunks) {
    all.set(c, at)
    at += c.length
  }
  return all
}

async function compress(format: CompressionFormat, ...chunks: Uint8Array[]): Promise<Uint8Array> {
  return concat(await collect(streamOf(...chunks).pipeThrough(new CompressionStream(format))))
}

async function decompress(format: CompressionFormat, ...chunks: Uint8Array[]): Promise<Uint8Array> {
  return concat(await collect(streamOf(...chunks).pipeThrough(new DecompressionStream(format))))
}

// `bytes` as one-byte chunks: every header, trailer and symbol boundary
// then lands between chunks.
function bytewise(bytes: Uint8Array): Uint8Array[] {
  return Array.from(bytes, (b) => Uint8Array.of(b))
}

test("each format round-trips through a CompressionStream and a DecompressionStream", async () => {
  let enc = new TextEncoder()
  let dec = new TextDecoder()
  let input = enc.encode(TEXT.repeat(200))
  for (let format of FORMATS) {
    let packed = await compress(format, input.slice(0, 1000), input.slice(1000))
    expect(packed.length).toBeLessThan(input.length / 10)
    expect(dec.decode(await decompress(format, packed))).toBe(TEXT.repeat(200))
  }
  // The containers are told apart by their first bytes: the gzip magic, the
  // zlib header, and neither for the bare stream.
  let gzip = await compress("gzip", input)
  let zlib = await compress("deflate", input)
  let raw = await compress("deflate-raw", input)
  expect(Array.from(gzip.slice(0, 3))).toEqual([0x1f, 0x8b, 8])
  expect(zlib[0]).toBe(0x78)
  expect(raw[0] === 0x1f || raw[0] === 0x78).toBe(false)
  // An empty input is a complete (empty) stream.
  expect((await decompress("gzip", await compress("gzip"))).length).toBe(0)
})

test("gzip and zlib bytes made by other tools decompress, chunked anywhere", async () => {
  let dec = new TextDecoder()
  expect(dec.decode(await decompress("gzip", GZIP))).toBe(TEXT)
  expect(dec.decode(await decompress("gzip", ...bytewise(GZIP_NAMED)))).toBe(TEXT)
  expect(dec.decode(await decompress("deflate", ...bytewise(ZLIB)))).toBe(TEXT)
  // The bare deflate stream inside the zlib container.
  expect(dec.decode(await decompress("deflate-raw", ZLIB.slice(2, ZLIB.length - 4)))).toBe(TEXT)
})

test("a fetched body decompresses through pipeThrough", async () => {
  await serving(
    {
      fetch() {
        return new Response(GZIP)
      },
    },
    async (base) => {
      let res = await fetch(base + "/asset.gz")
      let text = await new Response(res.body.pipeThrough(new DecompressionStream("gzip"))).text()
      expect(text).toBe(TEXT)
    },
  )
})

test("a writer feeds a CompressionStream any buffer shape, chunk by chunk", async () => {
  let enc = new TextEncoder()
  let cs = new CompressionStream("deflate")
  let out = collect(cs.readable)
  let writer = cs.writable.getWriter()
  let bytes = enc.encode(TEXT)
  // A Uint8Array, an ArrayBuffer, and another view of a buffer.
  await writer.write(bytes.slice(0, 5))
  await writer.write(bytes.slice(5, 10).buffer)
  await writer.write(new DataView(bytes.buffer, 10))
  await writer.close()
  let packed = concat(await out)
  expect(new TextDecoder().decode(await decompress("deflate", packed))).toBe(TEXT)
})

test("an unknown format and a chunk that is no buffer are TypeErrors", async () => {
  expect(() => new CompressionStream("br" as CompressionFormat)).toThrow(TypeError)
  expect(() => new CompressionStream("br" as CompressionFormat)).toThrow("unsupported format \"br\"")
  expect(() => new DecompressionStream(undefined as unknown as CompressionFormat)).toThrow(TypeError)
  let cs = new CompressionStream("gzip")
  // A write into a transform waits for its reader, so read first.
  let readable = collect(cs.readable)
  let writer = cs.writable.getWriter()
  await expect(writer.write("text" as unknown as Uint8Array)).rejects.toThrow(TypeError)
  await expect(writer.write("text" as unknown as Uint8Array)).rejects.toThrow("ArrayBuffer")
  // The error took the readable side with it.
  await expect(readable).rejects.toThrow("ArrayBuffer")
})

test("corrupt input errors both sides with a TypeError", async () => {
  // Four bytes of the deflate body zeroed.
  let corrupt = GZIP.slice().fill(0, 20, 24)
  let out = decompress("gzip", corrupt)
  await expect(out).rejects.toThrow(TypeError)
  await expect(out).rejects.toThrow("DecompressionStream: the gzip data is corrupt")
  // Bytes that are not gzip at all.
  await expect(decompress("gzip", ZLIB)).rejects.toThrow("not a gzip stream")
  // The error reaches a writer on the other side too.
  let ds = new DecompressionStream("deflate")
  let writer = ds.writable.getWriter()
  let readable = collect(ds.readable)
  await expect(writer.write(GZIP)).rejects.toThrow("corrupt")
  await expect(readable).rejects.toThrow("corrupt")
  await expect(writer.closed).rejects.toThrow("corrupt")
})

test("input cut short errors when the writable side closes", async () => {
  for (let format of FORMATS) {
    let packed = await compress(format, new TextEncoder().encode(TEXT))
    let out = decompress(format, packed.slice(0, packed.length - 3))
    await expect(out).rejects.toThrow(TypeError)
    await expect(out).rejects.toThrow(`the ${format} data is cut short`)
  }
  // The close itself reports it to a writer.
  let ds = new DecompressionStream("gzip")
  let writer = ds.writable.getWriter()
  let readable = collect(ds.readable)
  await writer.write(GZIP.slice(0, 30))
  await expect(writer.close()).rejects.toThrow("cut short")
  await expect(readable).rejects.toThrow("cut short")
})

test("data after the end of the stream is an error, a second gzip member included", async () => {
  await expect(decompress("gzip", GZIP, Uint8Array.of(0))).rejects.toThrow("trailing data after the end of the gzip stream")
  await expect(decompress("gzip", concat([GZIP, GZIP]))).rejects.toThrow(TypeError)
  await expect(decompress("deflate", ZLIB, ZLIB)).rejects.toThrow("trailing data after the end of the deflate stream")
})

test("output comes in pieces of at most 64 KiB, each as the reader asks", async () => {
  // Four MiB of zeros: a few KiB compressed, so one written chunk.
  let plain = new Uint8Array(4 * 1024 * 1024)
  let packed = await compress("gzip", plain)
  expect(packed.length).toBeLessThan(plain.length / 100)
  let ds = new DecompressionStream("gzip")
  let writer = ds.writable.getWriter()
  let settled = false
  let write = writer.write(packed).then(() => (settled = true))
  let closed = writer.close()
  let reader = ds.readable.getReader()
  let first = await reader.read()
  expect(first.value!.length).toBe(OUTPUT_CHUNK_BYTES)
  // The write is still in progress: the rest waits for the reader.
  expect(settled).toBe(false)
  let total = first.value!.length
  let pieces = 1
  while (true) {
    let { value, done } = await reader.read()
    if (done) break
    expect(value!.length).toBeLessThanOrEqual(OUTPUT_CHUNK_BYTES)
    total += value!.length
    pieces++
  }
  await write
  await closed
  expect(settled).toBe(true)
  expect(total).toBe(plain.length)
  expect(pieces).toBeGreaterThanOrEqual(plain.length / OUTPUT_CHUNK_BYTES)
})

test("cancelling the readable side stops a decompression mid-way", async () => {
  let plain = new Uint8Array(4 * 1024 * 1024)
  let packed = await compress("deflate-raw", plain)
  let out = streamOf(packed).pipeThrough(new DecompressionStream("deflate-raw"))
  let reader = out.getReader()
  expect((await reader.read()).value!.length).toBe(OUTPUT_CHUNK_BYTES)
  await reader.cancel()
  reader.releaseLock()
  expect(await collect(out)).toEqual([])
})
