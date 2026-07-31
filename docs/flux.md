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
let ms = performance.now()  // ms since process start
```

### TextEncoder / TextDecoder

```js
let bytes = new TextEncoder().encode("hello")
let str = new TextDecoder().decode(bytes)
```

### atob / btoa

Base64 over binary strings (each char code is one raw byte, no UTF-8 step).
`btoa` throws on a code point above 255; `atob` ignores ASCII whitespace and
throws on anything else that is not valid base64.

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
