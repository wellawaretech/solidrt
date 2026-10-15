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
`WritableStream` and `TransformStream`, `CompressionStream` and
`DecompressionStream`, `console`, `setTimeout` and
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
project beside the app, `runtime/`, that makes the same call with its
modules. `sol init` writes one:

```sh
sol init my-app --runtime   # a new app with its runtime
sol init --runtime          # a runtime added to the app in the current folder
```

What it writes is the project's own and nothing else:

```
runtime/
  Makefile              one line: include ../node_modules/@solidrt/cli/runtime/runtime.mk
  Cargo.toml            the crate, its two binaries, lattice at the release tag
  src/lib.rs            modules(), fonts() and program(): the modules, the fonts, the embedded screens
  src/hello.rs          runtime:hello, the starter module (hello(): string)
  src/bin/solidrt.rs    the production runtime: lattice::main(program())
  src/bin/solidrt-go.rs the player, the same program behind the `go` feature
  types/hello.d.ts      declare module "runtime:hello", included by tsconfig.json
tests/runtime.test.tsx  imports runtime:hello; runs on the app's own player
```

and in package.json `"solidrt": { "runtime": "runtime/dist" }`, the folder
`sol run`, `sol test`, `sol pack` and `sol android` take the app runtime
from, and `"appId"`, which a runtime needs as an Android application id
(reverse-DNS, `com.example.app`): the runtime's player is named after it,
so `sol init` asks for one when package.json has none. The build is the
one line in the Makefile: everything a runtime's
build needs beyond the crate comes from the `@solidrt/cli` package and
moves with it, so an upgrade is the lattice tag, the `@solidrt` packages
and the Solid pins in package.json (solid-js and the two @solidjs
packages, which `sol init` writes) bumped together, and no file the
scaffold wrote goes stale. From there:

```sh
make -C runtime           # the player and the runtime for this host
make -C runtime android   # the player and runtime .so for ANDROID_ABI (arm64-v8a)
make -C runtime help      # every goal and flag
```

Each goal is one cargo build (or `cargo ndk` run for Android), staged into
`runtime/dist/` in the layout sol reads:

```
dist/<platform>/solidrt-go, solidrt     platform: linux-x64-gnu, linux-arm64-gnu,
                                        darwin-arm64, win32-x64-msvc (the npm
                                        platform package name, not the Rust triple)
dist/android/<abi>/libmain.so           the player .so, with libSDL3.so, libimpeller.so
dist/android-runtime/<abi>/libmain.so   the runtime .so, the same beside it
dist/fonts.txt                          the aliases of the program's font set, one per
                                        line (`solidrt --fonts`), written by the host goals
```

On Windows and macOS the ANGLE libraries from the platform package are
copied beside the binaries. On Android the APKs are never yours to carry:
sol derives them from the stock ones with every `.so` in the folder swapped
in, so a module that brings a shared library of its own (a C++ library, a
vendor SDK) puts it there and it ships. The NDK's C++ runtime,
`libc++_shared.so`, is staged when a staged lib links it, as any C++
module does, and left out otherwise.
Every staged lib must be 16 KB page aligned (Android 15+ refuses others);
the build links it so and checks the result, and sol checks again before
it derives an APK. The derived player is an app of its own on the device,
`<appId>.player` labelled "<displayName> Player", so it installs beside
the stock Player and beside the players of your other projects instead of
replacing them, and sits apart from your packed app under its own name;
the derived runner keeps the stock placeholders, which `sol pack --apk`
rewrites to the app's.

A target you did not build falls back to the stock binary with a notice,
as long as the app imports none of your modules; once its bundle imports a
`runtime:*` module, a missing binary for the target is an error naming the
path sol looked for, from `sol run`, `sol test`, `sol pack` and `sol
android` alike, since the stock binary would fail at that import.

The lattice tag in Cargo.toml and the app's `@solidrt` packages are at one
version: the two screens the runtime embeds (the player and the failure
screen, `apps/player` in the checkout) are bundled at release into the
`@solidrt/cli` package under `player/`, made for the runtime of that
release, and `lib.rs` includes them from there. The build refuses a tag
that differs from the installed `@solidrt/cli` and names both versions, so
an upgrade cannot be half done. `lattice::VERSION`, what the client reports
and every log line carries, is that tag.

#### The fonts

The runtime carries the fonts every app on it starts from, registered
in the player and in a packed app alike; an app's `solidrt.fonts` map
layers over them (a file replaces the font under its alias or adds one,
`false` drops one), and `sol pack` ships only the files the app binds.
The stock set is Noto Sans, Noto Serif and Noto Sans Mono under the role
aliases `sans`, `serif` and `mono`; `fonts()` in `lib.rs` edits it:

```rust
// runtime/src/lib.rs
pub fn fonts() -> lattice::Fonts {
  lattice::Fonts::stock()
    .without("mono")                                        // no mono role: it resolves like sans
    .with("sans", include_bytes!("../fonts/Brand.ttf"))     // the brand face is the default
    .with("display", include_bytes!("../fonts/Display.ttf")) // selectable as fontFamily "display"
}
```

`Fonts::none()` starts empty, for a runtime whose apps bring every face
they draw. One face per alias: `with` on an alias already in the set
replaces it, and `without` on one that is not panics at startup, since
the set is the program's own code. Dropping a role from the runtime drops
it for its apps too: nothing ships a stock font back in.

The host goals list the set in `dist/fonts.txt` from the binary they
staged (`solidrt --fonts` prints the aliases, one per line), and sol reads
it: a `true` or `false` in an app's `solidrt.fonts` on an alias the
runtime does not bind fails the command, and `sol pack` reports which
fonts the app binds and which the runtime carries. Without the listing
(the runtime not built on this host) sol lets the alias through, and the
runtime says at startup which fonts it registered and warns on a dropped
alias it did not bind.

#### Writing a module

A module is an rquickjs `ModuleDef` (`lattice::flux::rquickjs`), registered
in `modules()` under a `runtime:*` name on every engine the runtime builds:

```rust
// runtime/src/lib.rs
mod physics;

pub fn modules() -> lattice::Modules {
  lattice::Modules::new().add("runtime:physics", physics::PhysicsModule)
}
```

`runtime:` is the scheme of every module a runtime adds; `flux:` and `sol:`
are the platform's, so the import says who ships the module and a built-in
SolidRT adds later cannot collide with it. A crate that several apps link
scopes its name, `runtime:acme/physics`, so two crates do not both claim
one. The name after the scheme is lowercase letters, digits, `-`, `_` and
`.`, with at most that one `/`; `Modules::add` panics on any other name, on
a platform scheme and on a name registered twice. `Flux.capabilities` lists
the module by its full name, `runtime:physics` here, so an app checks for
it the way it checks a built-in and cannot mistake one for the other, and
the dev client reports it to the dev server beside the stock capabilities.
An app bundle leaves every `runtime:*` import to the runtime, as it does
`flux:*` and `sol:*`, so no build configuration names your modules; a test
that imports one runs on your runtime's player under `sol test`. The
module's types are a `declare module "runtime:physics"` file in
`runtime/types/`.

lattice is the one dependency: rquickjs comes as `lattice::flux::rquickjs`,
its macros included, and tokio as `lattice::flux::tokio`, so the runtime
pins neither. The macros look their crate up by the name in your manifest,
so with no `rquickjs` line each one is told the alias instead:
`use lattice::flux::rquickjs;` once per module body, then
`#[rquickjs::class(crate = "rquickjs")]`, `#[rquickjs::methods(crate =
"rquickjs")]`, `#[rquickjs::function(crate = "rquickjs")]`,
`#[rquickjs::module(crate = "rquickjs")]`, and `#[qjs(crate = "rquickjs")]`
under a `derive(Trace, JsLifetime, FromJs, IntoJs)`. A `#[rquickjs::module]`
body is a scope of its own: it repeats the `use`, and a function inside it
carries the option too, since the module does not pass it on. The module it
makes is `js_<name>`, a `ModuleDef` for `Modules::add`.

A module that paces work by the frame (a physics world stepping on a fixed
timestep, a device read per frame) registers a tick with
`lattice::flux::gui::frame::on_advance`: it runs every frame with the app
time, before the frame's callbacks, so the dev clock's pause and step hold
and drive it and a headless render replays it; returning true demands the
next frame, the way a running animation does. A module that works with the
renderer's state, a texture or a camera, gets alloy's `Context` from
`lattice::flux::gui::alloy_context(ctx)` and names alloy's types through
`lattice::flux::alloy`. A module that reads camera frames (inference, hand
tracking, OCR) takes a tap on the open camera with
`camera_frame_tap(textureId)`, keyed by the texture id the app holds as
`cam.texture`, and waits on it from a thread of its own: `wait_newer` hands
it the newest upright RGBA8 frame with SDL's capture timestamp, newest wins
when the module is slower than the camera, and the wait ends with `Closed`
when the camera closes or the engine reloads. A module that reads
microphone audio (speech recognition, a keyword spotter) takes a reader of
its own on the open session with `microphone_tap(id)`, keyed by the id the
app holds as the session's `id`, and waits on it from a thread of its own:
`wait` blocks until a chunk is buffered, `read_into` drains every sample
since the reader's last read, so a module and the app's own `read()` both
hear everything, and a reader slower than the session's 10 s window is
told how many samples it lost. The wait ends with `Closed` when the
session closes or the engine reloads. A camera module's results ride the
frame tick because a streaming camera demands frames; a microphone does
not, so its results come back as an async iterable
(`attach_async_iterator`) or a promise (`with_in_flight`), which wake JS
without a frame.

Bindings use flux's marshalling toolkit, exported through `lattice::flux`
the way flux's own plugins use it. `bytes_of(&ctx, &array, "name")` is a
JS buffer's bytes in place (`elements_of` the typed view, `elements_mut_of`
one to write into), valid during the call and no longer: once JS runs again
the buffer may change, so bytes that outlive the call are copied.
`with_in_flight(&ctx, "name", future)` is the async binding: it returns the
promise for a future of `Result<T, String>`, holds the engine as work in
flight until it settles (a reload and the test host's settle wait for it),
and rejects an `Err` with its message; `with_standing` is the same for a
wait with no bound, a socket read, which nothing waits for. `Step`,
`iter_result` and `attach_async_iterator` make an object a Rust-backed async
iterable (`for await`), `string_opt` reads a field of an options object,
`OptArg<T>` is an optional argument that takes `undefined` for absent. With
`use lattice::flux;`, the two shapes a binding takes
(`lattice/examples/custom_runtime.rs`, where they run):

```rust
/// checksum(bytes: Uint8Array): number
fn checksum<'js>(ctx: Ctx<'js>, bytes: TypedArray<'js, u8>) -> rquickjs::Result<f64> {
  let data = flux::bytes_of(&ctx, &bytes, "checksum")?;
  Ok(data.iter().map(|&b| b as u64).sum::<u64>() as f64)
}

/// digest(bytes: Uint8Array): Promise<number>, the sum on a thread of its own
fn digest<'js>(ctx: Ctx<'js>, bytes: TypedArray<'js, u8>) -> rquickjs::Result<impl IntoJs<'js>> {
  let data = flux::bytes_of(&ctx, &bytes, "digest")?.to_vec();
  Ok(flux::with_in_flight(&ctx, "digest", async move {
    let (send, recv) = flux::tokio::sync::oneshot::channel();
    std::thread::spawn(move || {
      let _ = send.send(data.iter().map(|&b| b as u64).sum::<u64>());
    });
    recv.await.map(|sum| sum as f64).map_err(|_| "digest: the worker ended early".to_string())
  }))
}
```

exported with `Function::new(ctx.clone(), checksum)`. Both are named
functions on purpose: in rquickjs a closure cannot take both `Ctx<'_>` and a
value parameter, the lifetimes do not unify, so a binding with arguments is
a named generic function, and one with several arguments takes one options
object (a `FromJs` struct) rather than a parameter list.

A module that brings a native library decides its threads with the library
in mind. On Android, a library built with `-fopenmp -static-openmp` (most
prebuilt NDK inference libraries) carries a libomp of its own that shuts
down when the last thread that ran OpenMP exits, and aborts when a new
thread starts it again (in `__kmp_create_affinity_none_places`). Run such a
library on one thread that lives as long as the process, never a thread per
job or per object.

#### Building and shipping it

The build flags are the stock program's (`make -C runtime help`): `PROFILE`
is `release` by default, the fast build that keeps the desktop binaries'
symbols for the dev loop, and `release-opt` for what you ship: fat LTO,
one codegen unit, stripped, much slower. The Android libs are staged
stripped at every profile, since the folder is what the APK carries and
sol swaps the libs in as they are: the player `.so` goes from 70 MB to 52
MB, the prebuilt impeller from 228 MB to 7 MB; the symbols stay in
`target/` for ndk-stack. `TEST=0` builds the player without the test host.

The capabilities are not flags but lattice's cargo features, chosen in
the runtime's `Cargo.toml`. The plain line the scaffold writes takes
lattice's default set, the stock runtime's: `video` (playback), `ktx2`
(compressed textures) and `wasm-native` (`flux:wasm` compiled to machine
code where the target allows it, interpreted elsewhere; `wasm` alone is
the interpreter everywhere). A runtime that needs less trims the line and
names everything it keeps:

```toml
lattice = { git = "...", tag = "v...", default-features = false, features = ["video", "wasm-native"] }
```

The player (`solidrt-go`) carries every capability whatever the runtime
lists, so dev does not catch an import a trimmed runtime lacks: that
surfaces on the device, where `Flux.capabilities` tells the truth.

Four builds make a release, the player and the runtime for the host and for
Android, each compiling all of lattice. On a laptop with eight parallel
jobs: 5 minutes for the player, 9 for the runtime (a target dir of its
own, nothing shared), 5.5 for the Android player (it shares the desktop
player's target dir, so the host-side crates come from there). Once
lattice is built, a change to the module alone rebuilds in about fifteen
seconds. Point a second project at a built one's cache with `GO_TARGET_DIR` and
`RUNTIME_TARGET_DIR` while developing; `make -C runtime clean` removes
the runtime's `target/` and `dist/`.

The toolchain pin, cargo's config (the Android 16 KB page link flag, the
rpaths, the profiles) and the cmake toolchains for Android and MSVC are the
checkout's own, shipped in the `@solidrt/cli` package under `runtime/` and
reached from there: the Makefile sets `RUSTUP_TOOLCHAIN` from the pin
(rustup installs it when missing, and the Android goals add their target),
passes the config with `--config`, and names the cmake files to `cargo
ndk` with the NDK environment (`NDK_HOME`, default `/opt/android-ndk`).
Nothing in the project repeats them. The editor is the one place that does
not go through make: an IDE's rust-analyzer uses the default toolchain
unless the project carries a `rust-toolchain.toml` of its own, a copy of
the package's.

`runtime/` in the checkout is the stock program: this crate with no
modules, the same one-line Makefile. `lattice/examples/custom_runtime.rs`
is a module at its smallest: one with a tick, a bytes binding and an async
one, run as any custom runtime is.

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
