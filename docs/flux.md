# Flux

Flux is an embeddable, cross-platform JavaScript runtime developed for SolidRT. Two standalone binaries are provided: `flux`, which runs JavaScript files, and `fluxc`, which compiles JavaScript to bytecode.

See [Performance model](../guide/performance.md) for how the interpreter-based runtime affects performance characteristics.

---

## Standard APIs

These APIs work as they do on the web. No imports needed.

### fetch

```js
let res = await fetch("https://api.example.com/data")
let data = await res.json()
```

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

---

## Flux global

The `Flux` global is available in all SolidRT apps. Add `@solidrt/flux-types` to your project for TypeScript types.

### Flux.file

```ts
Flux.file(path: string): FluxFile
```

Returns a file handle. Reading the file is lazy - no I/O happens until you call a body method.

```js
let file = Flux.file("data.json")
let text = await file.text()
let bytes = await file.bytes()
let obj = await file.json()
let exists = await file.exists()
let stat = await file.stat()   // { size, type, mtime }
```

### Flux.write

```ts
Flux.write(path: string, data: string | Uint8Array): Promise<void>
```

Writes data to a file, creating it if it does not exist.

```js
await Flux.write("output.txt", "hello")
await Flux.write("output.bin", new Uint8Array([1, 2, 3]))
```

### Flux.dir

```ts
Flux.dir(path: string): FluxDir
```

Returns a directory handle.

```js
let dir = Flux.dir("./assets")
let entries = await dir.entries()
// entries: [{ name, type }, ...]
let exists = await dir.exists()
```

Each entry has a `name` (filename only, not full path) and a `type`: `"file"`, `"directory"`, `"symlink"`, or `"other"`.

### Flux.serve

```ts
Flux.serve(options: { port: number, fetch?: (req: Request) => Response | string | Promise<Response | string> }): void
```

Starts an HTTP server on the given port. The `fetch` handler receives a standard `Request` and should return a `Response` or a string.

```js
Flux.serve({
  port: 3000,
  fetch(req) {
    return new Response("hello")
  }
})
```

### Flux.on / Flux.once

```ts
Flux.on(event: string, callback: (data: any) => void): () => void
Flux.once(event: string, callback: (data: any) => void): () => void
```

Subscribe to runtime events. Both return a cleanup function.

```js
let off = Flux.on("some-event", (data) => {
  console.log(data)
})

off() // to unsubscribe
```

`Flux.once` fires the callback only on the first occurrence, then unsubscribes automatically.