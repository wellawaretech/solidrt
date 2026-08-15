# Flux

Flux is an embeddable, cross-platform JavaScript runtime developed for SolidRT. Three standalone binaries are provided:

- `flux` - run a JS source file
- `fluxc` - compile JS to bytecode (stdin -> stdout)
- `fluxrt` - self-contained runtime with bytecode appended to the binary

See [Performance model](../guide/performance.md) for how the interpreter-based runtime affects performance characteristics.

---

## Standard APIs

These APIs work as they do on the web. No imports needed.

### fetch

```js
let res = await fetch("https://api.example.com/data")
let data = await res.json()
```

`Request`, `Response`, and `Headers` are also available as globals. Bodies
read once via `text()`, `bytes()`, `arrayBuffer()`, or `json()`, or stream
with `for await (let chunk of res.body)`.

The `headers` option takes a plain object or a `Headers` instance; values
must be strings. A body must be a string, `Uint8Array`, or async-iterable.
Anything else throws (where the web would stringify it): a number header
value or a plain-object body is treated as a caller bug, never coerced or
silently dropped.

#### Caching

Caching is explicit and per call. By default `fetch` never caches (like
Node/Bun/Deno): API traffic stays predictable with zero configuration.

```js
// Serve from disk if stored, otherwise fetch and store. No freshness, no
// TTL: the entry lives until evicted by the size cap. Asset mode.
let img = await fetch(url, { cache: "force-cache" })

// Fetch fresh and overwrite the stored entry.
let img = await fetch(url, { cache: "reload" })
```

What this is not:

- Server cache headers (`cache-control`, `expires`, `etag`) are ignored
  entirely. The caller decides, not the server.
- No freshness model, no TTL, no revalidation, no `Vary`. An unversioned URL
  cached with `force-cache` never updates until evicted or `reload`ed;
  versioned URLs are the normal way to handle updatable assets.
- Only GET requests with 2xx responses are cached, keyed by URL. On other
  methods the `cache` option is ignored.
- `"default"`, `"no-store"`, and `"no-cache"` are accepted and all mean a
  plain network request; unknown values throw.

The store is a size-capped LRU disk cache in a directory the embedding
runtime configures (a GUI runtime uses the app's pref path); an evicted
entry is simply refetched next time. The bare flux runtime configures no
directory, so scripts run without a disk store: the `cache` option is
accepted and every request goes to the network.

Requests identify themselves with a `User-Agent` of `FluxRT/<version>`
(an embedding runtime replaces this with its own product token).

Cached fetches are also polite: a small per-host concurrency limit keeps
asset floods from swamping a server (misses queue; disk hits are not
throttled), and a 429 response backs off and retries - the whole host pauses
(honoring `Retry-After` when sent, jittered exponential backoff otherwise)
and the request is retried a few times before the 429 is returned. Plain
fetches have none of this - API calls, long-polls, and streams do not queue
behind asset traffic, and a caller that wants backoff on API calls implements
its own policy.

### Timers

```js
let id = setTimeout(cb, ms)
clearTimeout(id)

let id = setInterval(cb, ms)
clearInterval(id)
```

Headless flux runs timers on the wall clock. An embedder can instead put a
context's timers on a virtual timeline (`install_virtual_time` +
`advance_virtual_time` from Rust): deadlines then live on the embedder's
clock, nothing fires until an advance moves past it, and one advance is one
task-queue turn (a due interval fires once; a timer registered by a fired
callback waits for the next advance). The GUI runtime does this, advancing
once per frame with the paced frame timestamp - timers are frame-quantized
there, freeze when the runtime clock pauses, and replay deterministically
in playback.

### Console

```js
console.log("info")
console.warn("warning")
console.error("error")
```

Arguments are joined by spaces. Strings print as-is, Error objects print as
`name: message` plus their stack, other values are JSON-stringified.

### performance

```js
let ms = performance.now()      // ms since a monotonic origin
let t0 = performance.timeOrigin // wall-clock ms when the runtime started
```

Headless: ms since process start. In a GUI runtime: the paced frame
timeline (the same clock the frame timestamps and timers march on), so it
freezes with the runtime clock; `Date.now()` is the wall-clock escape
hatch. Because `now()` is paced, `timeOrigin + now()` is not the current
wall-clock time.

### TextEncoder / TextDecoder

```js
let bytes = new TextEncoder().encode("hello")
let str = new TextDecoder().decode(bytes)
```

### atob / btoa

Base64 over binary strings (each char code is one raw byte, no UTF-8 step).
`btoa` throws on a code point above 255. `atob` is forgiving per WHATWG:
ASCII whitespace is ignored and missing `=` padding is tolerated; anything
else that is not valid base64 throws.

```js
let encoded = btoa("\x00\xff")   // "AP8="
let bytes = atob(encoded)        // read back with charCodeAt
```

---

## Flux global

The `Flux` global is available in all SolidRT apps.

### Flux.on / Flux.once

```ts
Flux.on(event: string, callback: (data: any) => void): () => void
Flux.once(event: string, callback: (data: any) => void): () => void
```

Subscribe to events emitted by the host. Both return an unsubscribe function.

```js
let off = Flux.on("some-event", (data) => {
  console.log(data)
})

off() // unsubscribe
```

`Flux.once` fires the callback only on the first occurrence, then unsubscribes automatically.

---

## flux: modules

### flux:audio

Sound playback (gui-enabled runtime only). `play` decodes (Ogg/Vorbis or WAV)
and starts in one call; `load` decodes once into a clip whose every `play()`
is a fresh overlapping playback; `loadPcm` takes raw samples with no container
(the typed array is the format: `Uint8Array` = u8, `Int16Array` = s16,
`Float32Array` = f32, interleaved when `channels: 2`); `stream` decodes a
large track on demand from a `file()` (single playback at a time). A playback
has live controls: `setGain(g)` (>= 0, 1.0 = clip level), `setPan(p)` (-1 left
to 1 right, clamped, equal-power; unpanned mono is ~3 dB louder than `pan: 0`),
and `ended()` for reclaiming voice pools without duration bookkeeping.

```js
import { play, load, loadPcm, stream, stop } from "flux:audio"

play(bytes, { loop: false, gain: 0.8, pan: -0.5 })  // fire-and-forget

let clip = load(bytes)             // decode once
let p = clip.play({ gain: 0.5 })   // cheap overlapping playback
p.setPan(0.7)                      // live, as the source moves
p.setGain(0.2)
p.ended()                          // finished (naturally or stopped)?
p.stop()
clip.unload()

let tone = loadPcm(new Float32Array(samples), 44100, { channels: 1 })

let music = stream(file("assets/track.ogg"))  // decode on demand
music.play({ loop: true })

stop()  // stop everything
```

### flux:fs

```js
import { file, dir } from "flux:fs"

let text  = await file("data.txt").text()
let bytes = await file("img.png").bytes()   // arrayBuffer() for an ArrayBuffer
let obj   = await file("data.json").json()
let stat  = await file("data.txt").stat()   // { size, type, mtime }
await file("out.txt").write("hello")

let entries = await dir("./assets").entries()  // [{ name, type }, ...]
await dir("./out/img").create()             // mkdir -p; ok if it exists
```

Each entry has a `name` (filename only) and a `type`: `"file"`, `"directory"`, `"symlink"`, or `"other"`.

Relative paths resolve against the process cwd, with one exception: when the
embedder has set the assets mount (a SolidRT app running an installed or
packed version), paths under `assets/` resolve read-only into that version's
immutable `assets/` tree instead - loose files on disk, or byte ranges inside
a single-file packed executable (reads, ranged reads, and seekable streaming
work identically in both forms); writes under `assets/` then error. Plain
flux scripts have no mount and see cwd behavior throughout.

### flux:http

```js
import { serve } from "flux:http"

let server = serve({
  port: 3000,
  fetch(req) {
    return new Response("hello")
  },
  routes: {
    "/health": new Response("ok"),
    "GET /users/:id": (req) => new Response(req.params.id),
  },
  error(err) {
    return new Response("error", { status: 500 })
  },
})

server.close()
```

### flux:image

The CPU image codec: `decodeImage` turns encoded bytes (png, jpeg, webp, gif,
bmp, ico) into tightly-packed RGBA8 pixels plus dimensions; `encodeImage` is
the reverse and round-trips its output. Both are synchronous and throw on bad
input. `format` defaults to `"png"` (lossless, keeps alpha); `"jpeg"` drops
the alpha channel and takes `quality` in 0..1 (default 0.9, ignored for png).

```js
import { decodeImage, encodeImage } from "flux:image"

let img = decodeImage(bytes)                 // { data, width, height }
let png = encodeImage(img)
let jpg = encodeImage(img, { format: "jpeg", quality: 0.8 })
```

### flux:isolate

Call a module's exports on another thread. `isolate(id)` is a handle on an
isolate module (in a SolidRT project a module with the `"use isolate"`
directive, id = its path relative to the source root without extension; under
standalone `flux` the file `<id>.js` next to the entry). Every property of the
handle is an async function that runs the export of that name in a second
flux runtime: its own heap, its own event loop, the non-gui `flux:*` modules
and standard globals. This is where a long synchronous `flux:ffi` or
`flux:wasm` call, or a heavy JS computation, goes so it does not stall the
main loop.

```js
// worker.js ("use isolate" in a SolidRT project)
import { open } from "flux:ffi"
let lib = open("libfoo.so", { ... })          // module state lives in the isolate
export function decode(buf) { return lib.symbols.decode(buf) }
export function sum(n) { let s = 0; for (let i = 0; i < n; i++) s += i; return s }
```

```js
// main.js
import { isolate } from "flux:isolate"
let worker = isolate("worker", { args: ["--fast"] })   // args: the child's flux:process argv

let s = await worker.sum(1_000_000)          // first call spawns the child; main stays free
let out = await worker.decode(bytes)         // same instance, same module state
worker.terminate()                           // kill now, even mid-computation
```

Arguments and results are copied across (shared-nothing): null, booleans,
numbers, strings, byte buffers (any typed-array view arrives as a
`Uint8Array`), arrays and plain objects; anything else throws a `TypeError`
as an argument and rejects the call as a result. Calls start in order and run
concurrently, as the same functions would in-process: the child is one
thread, so a sync export runs to completion before anything else, while an
async export lets other calls run at each `await` (an export that must not
interleave with itself serialises inside the module). A throw in the export
rejects that call and the isolate keeps
running; an uncaught error that ends the child (a failed module load) rejects
pending and later calls with a message naming it. Each `isolate()` call is
its own instance, so two handles are two runtimes. Children die with their
parent runtime, so an exit or reload never leaks a background thread. In
TypeScript, `isolate<typeof import("./worker")>("worker")` types the handle
(`import type` keeps the module out of the main bundle).

Streams: an `async function*` export is pulled item by item with `for await`.
Each step is one round trip (backpressure by construction), `break` runs the
generator's `finally` in the isolate, a throw in the generator rejects the
pending step, and a never-ending generator is a subscription. What
the call returns is still a Promise, but one that is also an async iterator;
awaiting a stream call rejects, iterating a plain call rejects.

```js
// worker.js
export async function* progress(total) {
  for (let done = 0; done < total; done++) {
    await step()
    yield { done, total }
  }
}

// main.js
for await (let p of worker.progress(100)) console.log(p.done)
```

An open stream keeps both runtimes alive until it ends, is broken out of, or
the child is terminated, like an uncleared interval.

Not yet: zero-copy `ArrayBuffer` transfer (bytes are copied in and out),
cancellation of a running plain call (`terminate()` is the only interrupt).

### flux:process

Process-level surface: arguments, host platform, memory usage, OS signals.
`argv` is the arguments the app was started with, empty when there are none.
App arguments only - no executable path, no script path (deliberately simpler
than Node/Bun's two leading entries): `flux script.js a b` gives `["a", "b"]`,
and a packed `fluxrt` binary passes everything after the executable.

```js
import { argv, platform, arch, memoryUsage, on } from "flux:process"

argv           // ["a", "b"] for `flux script.js a b`
platform       // "linux", "darwin", "win32", "android", ...
arch           // "x64", "arm64", ...
memoryUsage()  // { rss } - resident set size in bytes
let off = on("SIGINT", (signal) => { /* ... */ })
```

A signal listener registered with `on`/`once` keeps the process alive until it
unsubscribes (`once` fires at most once and removes itself; `on` returns the
unsubscribe function). Signals are Unix only; elsewhere listeners are a no-op.

### flux:sqlite

```js
import { Database } from "flux:sqlite"

let db   = await Database.open("app.db")
let stmt = db.query("SELECT * FROM users WHERE id = ?")
let rows = await stmt.all(42)
let row  = await stmt.first(42)
await stmt.run(42)
db.close()
```

### flux:svg

Parses an SVG document string into plain draw data (sandboxed: no network,
file, or data-URI access). Geometry comes out as absolute path data with all
transforms baked in; paints resolve to `#rrggbbaa` strings or absolute-space
gradient objects. Draw keys match the path element props, so a draw spreads
onto a `<d-path>` unchanged. `opts.color` drives `currentColor` as a packed
`0xRRGGBBAA` number; the `@solidrt/core` re-export `parseSvg` accepts any CSS
color string instead. Unsupported and skipped: clipPath, masks, filters,
patterns, embedded images, SVG text.

```js
import { parseSvg } from "flux:svg"

let doc = parseSvg(src, { color: 0x336699ff })
doc.width           // intrinsic (viewBox) size
doc.draws           // [{ d, color, drawStyle, fillRule?, strokeWidth?, ... }]
```

### flux:wasm

A generic WebAssembly host (a wasmi interpreter; there is no `WebAssembly`
global, this module is the entire wasm surface). Runs precompiled `.wasm`
binaries, or wat text encoded as bytes. Everything is synchronous on the JS
thread. Imports must be scalar-signature functions only - no imported memory,
globals, or tables - so default emscripten output is rejected while
`emcc -sSTANDALONE_WASM=1 --no-entry` fits. i32/f32/f64 marshal as number,
i64 as BigInt.

```js
import { Module } from "flux:wasm"

let mod = new Module(bytes)             // Uint8Array | ArrayBuffer
mod.imports                             // [{ module, name, params, results }]
let instance = mod.instantiate({
  env: { mul: (a, b) => a * b },        // host functions, keyed like the standard
})
instance.exports                        // [{ name, kind, params?, results? }]
instance.call("run", 6)                 // scalar / undefined / array by result count
instance.callIndirect(fp, 1, 2)         // call table[fp] (a guest function pointer)
instance.memory                         // ArrayBuffer over linear memory, or undefined
instance.readMemory(ptr, len)           // copy out as a fresh Uint8Array
instance.writeMemory(ptr, bytes)        // copy in
```

`instance.memory` aliases the guest's linear memory, so reads and writes are
copy-free - `new Uint8Array(instance.memory, ptr, len)` hands guest bytes to
`uploadTexture` without any intermediate allocation. It follows the web's
`WebAssembly.Memory.buffer` contract: the buffer stays valid until the guest
grows its memory, which detaches it; read `memory` again for a fresh buffer.
