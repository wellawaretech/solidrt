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
| `flux:fs` | Files, directories, canonical paths, the files a glob pattern matches. |
| `flux:path` | Path joining, containment checks, the `basename` / `dirname` / `extname` splits, the path from one place to another, glob matching. |
| `flux:http` | HTTP and WebSocket servers, with routing. |
| `flux:net` | TCP and UDP sockets. |
| `flux:p2p` | Direct peer-to-peer connections, no server in the middle. |
| `flux:mdns` | Local network service discovery. |
| `flux:sqlite` | SQLite, on a dedicated thread. |
| `flux:subprocess` | Spawn and drive processes. |
| `flux:process` | Arguments, environment, platform, executable path, memory usage, signal handlers, process liveness, `exit`. |
| `flux:wasm` | Run WebAssembly modules, interpreted. Portable across every target; a small constant factor over JavaScript on tight compute, nowhere near browser wasm speed. |
| `flux:isolate` | Run a module on its own thread and call it like an object. |
| `flux:image` | Decode and encode images; transcode and encode compressed textures (KTX2). |
| `flux:svg` | Parse an SVG document into draw data. |
| `flux:test` | Tests for a flux program: `test`, `expect`. On the `flux` binary only, which runs a test file with `--test`, every test in an engine of its own; what `sol test` runs on. |

A GUI build of Flux, which is what SolidRT runs on, adds the device and
rendering modules: `flux:rendertree` (the native tree `@solidrt/core` drives),
`flux:gpu` (textures, shaders, draw targets), `flux:spatial` (a native
transform hierarchy feeding draw entries), `flux:camera`,
`flux:microphone`, `flux:audio` and `flux:video`, plus one web-standard
global that needs a windowing platform: `navigator`, carrying
`navigator.clipboard` (`readText`/`writeText`, text only). The `create*`
primitives in Core wrap them with reactivity; the modules are the
imperative layer underneath.

Where a standard exists, Flux keeps its vocabulary and simplifies the
semantics to what a single known application needs, rather than what the
whole web needs. The simplifications are documented rather than hidden.

## Native code

Two routes exist for code that needs more than the JavaScript interpreter,
and the sandbox decides between them:

- `flux:wasm` for portable compute: one `.wasm` module, the same bytes on
  every target, sandboxed. A proven C, C++ or Rust library with a wasm build
  runs here without porting.
- A custom runtime build for anything that reaches a native library, a
  platform SDK or engine state. The runtime is a set of Rust crates, and the
  `solidrt` binary is a thin program over them; a custom build is your own
  binary over the same crates with your module registered beside the built-in
  ones, built for each target the app ships on. The app stays a plain
  portable `.solapp`; what differs is the runtime it runs on.

There is deliberately no route that loads a shared library from app code. A
shared library is per platform, so it would make the app payload per
platform, and on Android the only native code that may run is what arrived
inside the APK, so it would be part of a build anyway.

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
sol bundle --flux src/main.ts   # -> dist/bundle/main.flux.js
sol pack --flux src/main.ts     # standalone executable
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
