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

`Request`, `Response`, and `Headers` are also available as globals.

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

### performance

```js
let ms = performance.now()  // ms since process start
```

### TextEncoder / TextDecoder

```js
let bytes = new TextEncoder().encode("hello")
let str = new TextDecoder().decode(bytes)
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
let bytes = await file("img.png").bytes()
let obj   = await file("data.json").json()
let stat  = await file("data.txt").stat()   // { size, type, mtime }
await file("out.txt").write("hello")

let entries = await dir("./assets").list()  // [{ name, type }, ...]
```

Each entry has a `name` (filename only) and a `type`: `"file"`, `"directory"`, `"symlink"`, or `"other"`.

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

server.stop()
```

### flux:sqlite

```js
import { Database } from "flux:sqlite"

let db   = await Database.connect("app.db")
let stmt = db.query("SELECT * FROM users WHERE id = ?")
let rows = await stmt.all(42)
let row  = await stmt.first(42)
await stmt.run(42)
db.close()
```