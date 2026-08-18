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
`fetch`, `Request`, `Response`, `Headers`, `console`, `setTimeout`,
`WebSocket`, `TextEncoder`, `URL`.

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
| `flux:path` | Path manipulation. |
| `flux:http` | HTTP and WebSocket servers, with routing. |
| `flux:net` | TCP and UDP sockets. |
| `flux:p2p` | Direct peer-to-peer connections, no server in the middle. |
| `flux:mdns` | Local network service discovery. |
| `flux:sqlite` | SQLite, on a dedicated thread. |
| `flux:subprocess` | Spawn and drive processes. |
| `flux:process` | Arguments, environment, exit. |
| `flux:wasm` | Run WebAssembly modules, interpreted. Portable across every target; a small constant factor over JavaScript on tight compute, nowhere near browser wasm speed. |
| `flux:ffi` | Call into native libraries. |

Where a standard exists, Flux keeps its vocabulary and simplifies the
semantics to what a single known application needs, rather than what the
whole web needs. The simplifications are documented rather than hidden.

## Capabilities, not platforms

Not every module works everywhere: a phone has no `flux:ffi` with
executable pages, a headless build machine has no camera. Ask by feature
name, never by guessing from the OS:

```js
if (Flux.capabilities.includes("ffi")) { /* ... */ }
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
