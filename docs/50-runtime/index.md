---
nav: Runtime
---

# Flux

Flux is the JavaScript runtime underneath SolidRT: QuickJS plus a set of
capability modules, embeddable and small. It fills the role Bun or Deno fill
elsewhere, and it is general purpose by design. SolidRT uses it to run UI,
but nothing in Flux is UI-specific: it runs a server, a build script, or a
command-line tool just as happily.

This site is built by a Flux script.

## Two kinds of API

Web-standard APIs are global, with the names and shapes you already know:
`fetch`, `Request`, `Response`, `Headers`, `console`, `setTimeout` and
`setInterval`, `queueMicrotask`, `performance`, `WebSocket`, `TextEncoder`
and `TextDecoder`, `atob` and `btoa`, `AbortController` and `AbortSignal`.
What is not there is as deliberate as what is: no `URL`, no `crypto`, no
`Blob`, `FormData` or streams. A single known app rarely needs them, and
each is a module away when it does.

Everything else is an explicit `flux:*` module import. Capabilities are
named, not ambient:

```js
import { file, dir } from "flux:fs"
import { serve } from "flux:http"

let config = await file("config.json").json()

serve({
  port: 3000,
  routes: {
    "/hello/:name": (req) => `Hello, ${req.params.name}`,
  },
})
```

## Modules

| Module | What it does |
| --- | --- |
| `flux:fs` | Files and directories. |
| `flux:path` | Path joining and containment checks. |
| `flux:http` | HTTP and WebSocket servers, with routing. |
| `flux:net` | TCP and UDP sockets. |
| `flux:p2p` | Direct peer-to-peer connections, no server in the middle. |
| `flux:mdns` | Local network service discovery. |
| `flux:sqlite` | SQLite, on a dedicated thread. |
| `flux:subprocess` | Spawn and drive processes. |
| `flux:process` | Arguments, platform, memory usage, signal handlers. |
| `flux:wasm` | Run WebAssembly modules, interpreted. Portable across every target; a small constant factor over JavaScript on tight compute, nowhere near browser wasm speed. |
| `flux:ffi` | Call into native libraries. |
| `flux:isolate` | Run a module on its own thread and call it like an object. |
| `flux:image` | Decode and encode images. |
| `flux:svg` | Parse an SVG document into draw data. |

A GUI build of Flux, which is what SolidRT runs on, adds the device and
rendering modules: `flux:rendertree` (the native tree `@solidrt/core` drives),
`flux:gpu` (textures, shaders, draw targets), `flux:camera`,
`flux:microphone`, `flux:audio` and `flux:video`. The `create*` primitives in
Core wrap them with reactivity; the modules are the imperative layer
underneath.

Where a standard exists, Flux keeps its vocabulary and simplifies the
semantics to what a single known application needs, rather than what the
whole web needs. The simplifications are documented rather than hidden.

## Capabilities, not platforms

Not every module works everywhere: a headless Flux build has no camera,
microphone, audio or GPU. Ask by feature name, never by guessing from the
OS:

```js
if (Flux.capabilities.includes("camera")) { /* ... */ }
```

## Running it

The Flux binary runs a JavaScript file directly. To go from TypeScript
sources to something it can run, bundle for the bare runtime:

```sh
srt bundle --flux src/main.ts   # -> src/main.flux.js
srt pack --flux src/main.ts     # standalone executable
```

Both are described in [Tools](/tools/).

## Reference

One page per declaration file, in three groups:
[Modules](/runtime/modules/) for the `flux:*` capabilities,
[Standards](/runtime/standards/) for the web-standard globals, and
[GUI](/runtime/gui/) for the render tree, the devices and the GPU surface.

The pages show the declarations themselves. `@solidrt/flux-types` is written
with a doc comment on every member, so the same text your editor shows on
hover is the reference here.
