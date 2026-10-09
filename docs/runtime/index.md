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
`fetch`, `Request`, `Response`, `Headers`, `ReadableStream`,
`WritableStream` and `TransformStream`, `console`, `setTimeout` and
`setInterval`, `setImmediate`, `requestIdleCallback`, `queueMicrotask`,
`performance`, `WebSocket`, `TextEncoder` and `TextDecoder` with
`TextEncoderStream` and `TextDecoderStream`, `atob` and `btoa`,
`AbortController` and `AbortSignal`.
What is not there is as deliberate as what is: no `URL`, no `crypto`, no
`Blob` or `FormData`. A single known app rarely needs them, and each is a
module away when it does.

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
`flux:microphone`, `flux:audio`, `flux:video` and `flux:font` (the glyph
engine: a registered font shaped on the runtime's own shaper, its glyphs
rasterized into an atlas texture a consumer draws from), plus one web-standard
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
- A custom runtime for anything that reaches a native library, a platform
  SDK or engine state: your own build of the runtime with your module
  registered beside the built-in ones, for each target the app ships on.
  The app stays a plain portable `.solapp`; what differs is the runtime it
  runs on.

There is deliberately no route that loads a shared library from app code. A
shared library is per platform, so it would make the app payload per
platform, and on Android the only native code that may run is what arrived
inside the APK, so it would be part of a build anyway.

### A custom runtime

The runtime is a set of Rust crates, and the stock `solidrt` binary is one
call into `lattice`, the crate that binds them. A custom runtime is a cargo
project beside the app, `runtime/` say, that makes the same call with its
modules:

```toml
# runtime/Cargo.toml
[package]
name = "my-runtime"
version = "0.0.0"
edition = "2021"

[workspace]

[lib]
crate-type = ["cdylib", "lib"]   # the Android .so, and what the bins link

[[bin]]
name = "solidrt"
path = "src/bin/solidrt.rs"

[[bin]]
name = "solidrt-go"
path = "src/bin/solidrt-go.rs"
required-features = ["go"]

[features]
go = ["lattice/go", "lattice/test"]   # the dev client, with the test host

[dependencies]
lattice = { git = "https://github.com/wellawaretech/solidrt", tag = "v0.0.68", default-features = false, features = ["compile", "video", "ktx2", "wasm-native"] }
rquickjs = { version = "=0.14.0", features = ["macro"] }
```

```rust
// runtime/src/lib.rs
mod physics;

pub fn modules() -> lattice::Modules {
  lattice::Modules::new().add("flux:physics", physics::PhysicsModule)
}

// The Android .so's entry; nothing on other targets.
lattice::android_entry!(crate::modules());
```

```rust
// runtime/src/bin/solidrt.rs, and solidrt-go.rs the same
fn main() {
  lattice::main(my_runtime::modules());
}
```

A module is an rquickjs `ModuleDef` (`lattice::flux::rquickjs`), registered
under a `flux:*` name on every engine the runtime builds. `Flux.capabilities`
lists it by its short name, `physics` here, so an app checks for it the way
it checks a built-in, and the dev client reports it to the dev server beside
the stock capabilities. A module that paces work by the frame (a physics
world stepping on a fixed timestep, a device read per frame) registers a
tick with `lattice::flux::gui::frame::on_advance`: it runs every frame with
the app time, before the frame's callbacks, so the dev clock's pause and
step hold and drive it and a headless render replays it; returning true
demands the next frame, the way a running animation does. A module that
works with the renderer's state, a texture or a camera, gets alloy's
`Context` from `lattice::flux::gui::alloy_context(ctx)` and names alloy's
types through `lattice::flux::alloy`.

Build the two binaries one cargo run each, `--features go` for the player
and none for the runtime (features unify within one build), and stage them
in a directory laid out like a checkout's `dist/`: `<triple>/solidrt` and
`<triple>/solidrt-go`, with the ANGLE libraries from the platform package
beside them on Windows and macOS, and for Android `android/<abi>/libmain.so`
and `android-runtime/<abi>/libmain.so` from `cargo ndk` builds of the
cdylib, `--features go` for the first. Name that directory in the app's
package.json:

```json
"solidrt": { "runtime": "runtime/dist" }
```

and `sol run`, `sol test`, `sol pack` and `sol android` take the app runtime
from it: the binaries directly, the Android APKs derived from the stock ones
with your `.so` files swapped in, so the Android shell is never yours to
carry. A target you did not build falls back to the stock binary, with a
notice.

A dependency does not carry its workspace's settings, so the project repeats
four of the checkout's: the toolchain pin (`rust-toolchain.toml`, the channel
and the Android targets), the `[profile]` overrides, the Android link flag
for 16 KB pages in `.cargo/config.toml` (`-C link-arg=-Wl,-z,max-page-size=16384`
per Android target; sol refuses a lib under it), and `SOLIDRT_VERSION` in the
build environment, which `lattice::VERSION` and every log line report. For
Android, cmake builds the native pieces (SDL, the video codecs) and finds
the NDK through the environment the checkout's `lattice/Makefile.android`
sets for `cargo ndk`: `ANDROID_NDK_HOME` and `ANDROID_NDK_ROOT`, a copy of
`lattice/android/cmake/android-abi.toolchain.cmake` (the NDK toolchain with
the ABI defaulted from the environment, which a build that passes no ABI of
its own needs) as `CMAKE_TOOLCHAIN_FILE_<rust target with underscores>`, and
`CMAKE_ANDROID_ARCH_ABI`. The module's types live in
the project (`types/physics.d.ts`, a `declare module "flux:physics"`,
included by `tsconfig.json`).

`lattice/examples/custom_runtime.rs` in the checkout is the shape at its
smallest: one module with a tick, run as any custom runtime is.

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
