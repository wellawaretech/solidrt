---
nav: Runtime
---

# Flux

Flux is the JavaScript runtime below SolidRT. It is QuickJS and a set of
capability modules. It is small, and you can embed it in other programs.
It has the same function that Bun or Deno have in other environments. It
is a general-purpose runtime. SolidRT uses it to run a UI, but no part of
Flux is specific to UI. It can also run a server, a build script or a
command-line tool.

A Flux script builds this site.

## Two types of API

The web-standard APIs are global. They have the names and the shapes that
you know from the web:

- `fetch`, `Request`, `Response` and `Headers`.
- `ReadableStream`, `WritableStream` and `TransformStream`.
- `CompressionStream` and `DecompressionStream`.
- `console`.
- `setTimeout`, `setInterval`, `setImmediate`, `requestIdleCallback` and
  `queueMicrotask`.
- `performance`.
- `WebSocket`.
- `TextEncoder` and `TextDecoder`, with `TextEncoderStream` and
  `TextDecoderStream`.
- `atob` and `btoa`.
- `AbortController` and `AbortSignal`.

Some APIs are not there, on purpose: `URL`, `crypto`, `Blob` and
`FormData`. One known app does not usually need them. When an app needs
one, a module can supply it.

All other APIs are explicit imports of `flux:*` modules. Each capability
has a name. A program gets a capability only when it imports it:

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
| `flux:fs` | Files, directories, canonical paths, and the files that match a glob pattern. |
| `flux:path` | Join paths, check if a path contains a different path, split a path (`basename` / `dirname` / `extname`), find the path from one location to a different location, and match glob patterns. |
| `flux:http` | HTTP and WebSocket servers, with routing. |
| `flux:net` | TCP and UDP sockets. |
| `flux:p2p` | Direct peer-to-peer connections, with no server between the peers. |
| `flux:mdns` | Service discovery on the local network. |
| `flux:sqlite` | SQLite. It runs on its own thread. |
| `flux:subprocess` | Start and control processes. |
| `flux:process` | Arguments, environment, platform, executable path, memory usage, signal handlers, a check if a process runs, and `exit`. |
| `flux:wasm` | Run WebAssembly modules. A build has one of two engines. wasmi is an interpreter, and it runs on all targets. On compute, it is 4x to 88x faster than JavaScript. wasmtime (the `wasm-native` feature) compiles a module to machine code. On compute, it is 18x to 960x faster than JavaScript. |
| `flux:isolate` | Run a module on its own thread, and call it as an object. |
| `flux:image` | Decode and encode images. Transcode and encode compressed textures (KTX2). |
| `flux:svg` | Parse an SVG document into draw data. |
| `flux:test` | Tests for a flux program: `test` and `expect`. Only the `flux` binary has this module. It runs a test file with `--test`, and each test runs in its own engine. `sol test` runs on this module. |

A GUI build of Flux adds the device modules and the rendering modules.
SolidRT runs on a GUI build. These are the modules that it adds:

- `flux:rendertree`: the native tree that `@solidrt/core` controls.
- `flux:gpu`: textures, shaders and draw targets.
- `flux:spatial`: a native transform hierarchy that supplies draw entries.
- `flux:camera`, `flux:microphone`, `flux:audio` and `flux:video`.
- `flux:font`: the glyph engine. The shaper of the runtime shapes a
  registered font. The engine rasterizes the glyphs into an atlas texture,
  and a consumer draws from that texture.
- `flux:dialog`: the open, save and folder pickers of the platform. The
  pickers resolve to `flux:fs` handles. The `dialog` capability is in the
  list only on platforms that can show a picker.

A GUI build also adds one web-standard global that needs a windowing
platform: `navigator`. It has `navigator.clipboard` (`readText`/`writeText`,
text only).

The `create*` primitives in Core add reactivity to these modules. The
modules are the imperative layer below the primitives.

When a standard exists, Flux uses the names of the standard. But Flux
makes the semantics simpler: it gives what one known application needs,
not what the full web needs. The documentation tells you about each
simplification. Flux does not hide them.

## Native code

Some code needs more than the JavaScript interpreter. There are two routes
for that code, and the sandbox decides which route to use:

- `flux:wasm`, for portable compute. One `.wasm` module has the same bytes
  on all targets, and it runs in a sandbox. A proven C, C++ or Rust library
  that has a wasm build runs here. You do not have to port it.
- A custom runtime, for code that uses a native library, a platform SDK or
  engine state. A custom runtime is your own build of the runtime. It
  registers your module next to the built-in modules. You make one build
  for each target where the app runs. The app stays a usual portable
  `.solapp`. Only the runtime below it is different.

[Performance](/guides/performance/) shows where the other types of work
belong.

On purpose, app code cannot load a shared library. A shared library is
specific to one platform. Thus, the app payload would also be specific to
one platform. Also, on Android, the only native code that can run is the
code in the APK. Thus, the library must be part of a build anyway.

### A custom runtime

The runtime is a set of Rust crates. The stock `solidrt` binary is one call
into `lattice`, the crate that binds these crates together. A custom
runtime is a cargo project, `runtime/`, next to the app. It makes the same
call with its modules. `sol init` writes this project:

```sh
sol init my-app --runtime   # a new app with its runtime
sol init --runtime          # a runtime added to the app in the current folder
```

It writes only files that belong to the project:

```
runtime/
  Makefile              one line: include ../node_modules/@solidrt/cli/runtime/runtime.mk
  Cargo.toml            the crate, its two binaries, lattice at the release tag
  Cargo.lock            the release's, settled by the first build; commit it
  src/lib.rs            modules(), fonts() and program(): the modules, the fonts, the embedded screens
  src/hello.rs          runtime:hello, the starter module (hello(): string)
  src/bin/solidrt.rs    the production runtime: lattice::main(program())
  src/bin/solidrt-go.rs the player, the same program behind the `go` feature
  types/hello.d.ts      declare module "runtime:hello", included by tsconfig.json
tests/runtime.test.tsx  imports runtime:hello; runs on the app's own player
```

It also writes `"solidrt": { "runtime": "runtime/dist" }` in package.json.
`sol run`, `sol test`, `sol pack` and `sol android` get the app runtime
from this folder.

The one line in the Makefile is the full build. All other parts of the
build, except the crate, come from the `@solidrt/cli` package and change
with it. Thus, to upgrade, change these items to the new version together:

- The lattice tag.
- The `@solidrt` packages.
- The Solid pins in package.json: solid-js and the two @solidjs packages,
  which `sol init` writes.

No file that `sol init` wrote becomes out of date. Then use these
commands:

```sh
make -C runtime           # the player and the runtime for this host
make -C runtime android   # the player and runtime .so for ANDROID_ABI (arm64-v8a)
make -C runtime help      # every goal and flag
```

Each goal is one cargo build, or one `cargo ndk` run for Android. The goal
puts the result into `runtime/dist/`, in the layout that sol reads:

```
dist/<platform>/solidrt-go, solidrt     platform: linux-x64-gnu, linux-arm64-gnu,
                                        darwin-arm64, win32-x64-msvc (the npm
                                        platform package name, not the Rust triple)
dist/android/<abi>/libmain.so           the player .so, with libSDL3.so, libimpeller.so
dist/android-runtime/<abi>/libmain.so   the runtime .so, the same beside it
dist/fonts.txt                          the aliases of the program's font set, one per
                                        line (`solidrt --fonts`), written by the host goals
```

On Windows and macOS, the build copies the ANGLE libraries from the
platform package next to the binaries.

On Android, you do not supply the APKs. Sol makes them from the stock
APKs, and puts each `.so` file from the folder into them. Thus, a module
can bring its own shared library, for example a C++ library or a vendor
SDK. Put the library in the folder, and it goes into the APK. The build
puts the C++ runtime of the NDK, `libc++_shared.so`, into the folder only
when a staged library links it. All C++ modules link it.

Each staged library must have a 16 KB page alignment, because Android 15
and later refuse other libraries. The build links each library with this
alignment and checks the result. Sol checks again before it makes an APK.

On the device, the derived player is a separate app, `<appId>.player`,
with the label "<displayName> Player". Thus, it installs next to the stock
Player and next to the players of your other projects. It does not replace
them. It also has a name that is different from your packed app. Thus, the
`"appId"` in package.json must be an Android application id, in
reverse-DNS format (`com.example.app`). Without this id, sol does not make
the player. On the desktop, no step needs this id. The derived runner
keeps the stock placeholders. `sol pack --apk` replaces them with the
values of the app.

If you did not build a target, sol uses the stock binary for that target
and shows a notice. This is possible only if the app imports none of your
modules. When the bundle imports a `runtime:*` module, a missing binary for
the target is an error. The error gives the path where sol looked for the
binary. `sol run`, `sol test`, `sol pack` and `sol android` all give this
error, because the stock binary would fail at that import.

The lattice tag in Cargo.toml and the `@solidrt` packages of the app have
the same version. The runtime embeds two screens: the player and the
failure screen (`apps/player` in the checkout). At release, these screens
go into the `@solidrt/cli` package, under `player/`, for the runtime of
that release. `lib.rs` includes them from there. If the tag is different
from the installed `@solidrt/cli`, the build stops and gives the two
versions. Thus, you cannot do only half of an upgrade. `lattice::VERSION`
is that tag. The client reports it, and each log line contains it.

#### The fonts

The runtime contains the start set of fonts for each app on it. The player
and a packed app both register these fonts. The `solidrt.fonts` map of an
app goes on top of them:

- A file replaces the font under its alias, or adds a font.
- `false` removes a font.

`sol pack` includes only the files that the app binds. The stock set is
Noto Sans, Noto Serif and Noto Sans Mono, under the role aliases `sans`,
`serif` and `mono`. To change the set, edit `fonts()` in `lib.rs`:

```rust
// runtime/src/lib.rs
pub fn fonts() -> lattice::Fonts {
  lattice::Fonts::stock()
    .without("mono")                                        // no mono role: it resolves like sans
    .with("sans", include_bytes!("../fonts/Brand.ttf"))     // the brand face is the default
    .with("display", include_bytes!("../fonts/Display.ttf")) // selectable as fontFamily "display"
}
```

`Fonts::none()` starts with an empty set. Use it for a runtime whose apps
bring all the faces that they draw. Each alias has one face. `with` on an
alias that is already in the set replaces the face. `without` on an alias
that is not in the set causes a panic at startup, because the set is the
code of the program. If you remove a role from the runtime, you also
remove it for its apps. No step adds a stock font back.

The host goals write the set to `dist/fonts.txt`. They get the set from
the binary that they staged (`solidrt --fonts` prints the aliases, one per
line). Sol reads this file:

- If the `solidrt.fonts` of an app sets `true` or `false` on an alias that
  the runtime does not bind, the command fails.
- `sol pack` reports the fonts that the app binds and the fonts that the
  runtime contains.

If this file is not there (because you did not build the runtime on this
host), sol accepts the alias. At startup, the runtime then tells which
fonts it registered. It also gives a warning for a dropped alias that it
did not bind.

#### Write a module

A module is an rquickjs `ModuleDef` (`lattice::flux::rquickjs`).
`modules()` registers it under a `runtime:*` name, on each engine that the
runtime builds:

```rust
// runtime/src/lib.rs
mod physics;

pub fn modules() -> lattice::Modules {
  lattice::Modules::new().add("runtime:physics", physics::PhysicsModule)
}
```

`runtime:` is the scheme of all modules that a runtime adds. `flux:` and
`sol:` are the schemes of the platform. Thus, the import shows who supplies
the module. Also, a built-in module that SolidRT adds later cannot have the
same name as your module. If several apps link a crate, the crate puts a
scope in its name, for example `runtime:acme/physics`. Thus, two crates
cannot use the same name.

The name after the scheme can contain lowercase letters, digits, `-`, `_`
and `.`, and a maximum of one `/`. `Modules::add` panics on these names:

- A name with other characters.
- A name with a platform scheme.
- A name that is already registered.

`Flux.capabilities` lists the module by its full name, here
`runtime:physics`. Thus, an app checks for it in the same way as for a
built-in module, and it cannot confuse the two. The dev client reports the
module to the dev server, together with the stock capabilities.

An app bundle does not resolve `runtime:*` imports. The runtime resolves
them, as it does for `flux:*` and `sol:*` imports. Thus, no build
configuration names your modules. If a test imports one of your modules,
`sol test` runs the test on the player of your runtime. Put the types of
the module in a `declare module "runtime:physics"` file in
`runtime/types/`.

lattice is the only dependency. It supplies rquickjs as
`lattice::flux::rquickjs`, with its macros, and tokio as
`lattice::flux::tokio`. Thus, the runtime does not pin these crates. The
macros find their crate by the name in your manifest. Your manifest has no
`rquickjs` line, thus you must give each macro the alias:

- Put `use lattice::flux::rquickjs;` one time in each module body.
- Then use `#[rquickjs::class(crate = "rquickjs")]`,
  `#[rquickjs::methods(crate = "rquickjs")]`,
  `#[rquickjs::function(crate = "rquickjs")]` and
  `#[rquickjs::module(crate = "rquickjs")]`.
- Use `#[qjs(crate = "rquickjs")]` below a
  `derive(Trace, JsLifetime, FromJs, IntoJs)`.

A `#[rquickjs::module]` body is a separate scope. It must have the `use`
line again. A function in it must also have the option, because the module
does not give the option to it. The module that the macro makes is
`js_<name>`. It is a `ModuleDef` for `Modules::add`.

Some modules must do work at the rate of the frames. Examples are a
physics world with a fixed timestep, or a device that you read one time
for each frame. Such a module registers a tick with
`lattice::flux::gui::frame::on_advance`. The tick runs each frame with the
app time, before the callbacks of the frame. Thus, the pause and the step
of the dev clock also control the tick, and a headless render replays it.
If the tick returns true, it demands the next frame, as an animation does
while it runs.

A module that uses the state of the renderer (for example a texture or a
camera) gets the alloy `Context` from
`lattice::flux::gui::alloy_context(ctx)`. It gets the alloy types through
`lattice::flux::alloy`.

A module that reads camera frames (for example for inference, hand
tracking or OCR) uses `camera_frame_tap(textureId)` to get a tap on the
open camera. The key is the texture id that the app has as `cam.texture`.
The module waits on the tap from its own thread:

- `wait_newer` gives the newest upright RGBA8 frame, with the capture
  timestamp from SDL.
- If the module is slower than the camera, it gets only the newest frame.
- The wait ends with `Closed` when the camera closes or the engine
  reloads.

A module that reads microphone audio (for example for speech recognition
or a keyword spotter) uses `microphone_tap(id)` to get its own reader on
the open session. The key is the id that the app has as the `id` of the
session. The module waits on the reader from its own thread:

- `wait` blocks until a chunk is in the buffer.
- `read_into` gives all the samples since the last read of this reader.
  Thus, the module and the `read()` of the app both get all the samples.
- If a reader is slower than the 10 s window of the session, it gets the
  number of samples that it lost.
- The wait ends with `Closed` when the session closes or the engine
  reloads.

A camera module sends its results with the frame tick, because a camera
that streams demands frames. A microphone does not demand frames. Thus, a
microphone module sends its results as an async iterable
(`attach_async_iterator`) or as a promise (`with_in_flight`). These wake JS
without a frame.

Bindings use the marshalling toolkit of flux, which `lattice::flux`
exports. The plugins of flux use the same toolkit:

- `bytes_of(&ctx, &array, "name")` gives the bytes of a JS buffer in
  place. `elements_of` gives the typed view, and `elements_mut_of` gives a
  view that you can write into. These are valid only during the call. When
  JS runs again, the buffer can change. Thus, copy the bytes that you must
  keep after the call.
- `with_in_flight(&ctx, "name", future)` is the async binding. It returns
  the promise for a future of `Result<T, String>`. Until the future
  settles, the engine counts it as work in flight (a reload and the settle
  of the test host wait for it). An `Err` rejects the promise with its
  message.
- `with_standing` is the same, for a wait with no limit, for example a
  socket read. Nothing waits for it.
- `Step`, `iter_result` and `attach_async_iterator` make an object into a
  Rust-backed async iterable (`for await`).
- `string_opt` reads a field of an options object.
- `OptArg<T>` is an optional argument that accepts `undefined` as absent.

With `use lattice::flux;`, a binding has one of two shapes.
`lattice/examples/custom_runtime.rs` runs the two shapes:

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

Export a binding with `Function::new(ctx.clone(), checksum)`. The two
bindings are named functions on purpose. In rquickjs, a closure cannot
take both `Ctx<'_>` and a value parameter, because the lifetimes do not
unify. Thus, a binding with arguments must be a named generic function. A
binding with several arguments takes one options object (a `FromJs`
struct), not a parameter list.

A module that brings a native library must select its threads for that
library. On Android, a library built with `-fopenmp -static-openmp` has
its own libomp. Most prebuilt NDK inference libraries are built in this
way. This libomp stops when the last thread that ran OpenMP ends. Then, if
a new thread starts OpenMP again, the libomp aborts (in
`__kmp_create_affinity_none_places`). Run such a library on one thread
that lives as long as the process. Do not use a thread for each job or for
each object.

#### Build the runtime

The build flags are the flags of the stock program
(`make -C runtime help`):

- `PROFILE` is `release` by default. This is the fast build. It keeps the
  symbols of the desktop binaries for the dev loop.
- `PROFILE=release-opt` is for a release build: fat LTO, one codegen unit,
  and stripped. This build takes much more time.
- `TEST=0` builds the player without the test host.

The build strips the Android libraries at all profiles, because the APK
contains the folder, and sol puts the libraries into the APK as they are.
The player `.so` goes from 70 MB to 52 MB, and the prebuilt impeller goes
from 228 MB to 7 MB. The symbols stay in `target/` for ndk-stack.

The capabilities are not flags. They are the cargo features of lattice,
which you select in the `Cargo.toml` of the runtime. The usual line that
`sol init` writes uses the default set of lattice. This is the set of the
stock runtime:

- `video`: playback.
- `video-encode`: recording, the `record` function of `flux:video`. It
  includes `video`.
- `ktx2`: compressed textures.
- `http`: `fetch`, the `WebSocket` client, and `flux:http`, with TLS.
  Without it, an app cannot load an image or a video from a URL.
- `p2p`: `flux:p2p`, and the `p2p` option of `serve` in `flux:http`.
- `p2p-relay`: `serveRelay` of `flux:p2p`, the self-hosted relay.
- `wasm-native`: `flux:wasm` on wasmtime, which compiles a module to
  machine code. Some targets do not support wasmtime (armeabi-v7a, x86
  Android, iOS). On these targets, the build uses wasmi. `wasm` alone is
  wasmi, the interpreter, on all targets.

A runtime that needs fewer capabilities changes the line. The line must
then name all the features that the runtime keeps:

```toml
lattice = { git = "...", tag = "v...", default-features = false, features = ["video", "wasm-native"] }
```

The player (`solidrt-go`) has all capabilities, independent of the list of
the runtime. Thus, during development, you do not see an import that a
reduced runtime does not have. You see the problem only on the device.
There, `Flux.capabilities` gives the correct list.

A release has four builds: the player and the runtime, for the host and
for Android. Each build compiles all of lattice. These are the times on a
laptop with eight parallel jobs:

- The player: 5 minutes.
- The runtime: 9 minutes. It has its own target dir, and it shares
  nothing.
- The Android player: 5.5 minutes. It shares the target dir of the desktop
  player, thus it gets the host-side crates from there.

After lattice is built, a change to only the module rebuilds in
approximately fifteen seconds. During development, set `GO_TARGET_DIR` and
`RUNTIME_TARGET_DIR` in a second project to use the cache of a project
that is already built. `make -C runtime clean` removes the `target/` and
`dist/` folders of the runtime.

These items belong to the checkout:

- The toolchain pin.
- The cargo configuration (the Android 16 KB page link flag, the rpaths,
  the profiles).
- The cmake toolchains for Android and MSVC.

The `@solidrt/cli` package contains them under `runtime/`, and the build
uses them from there:

- The Makefile sets `RUSTUP_TOOLCHAIN` from the pin. If the toolchain is
  not installed, rustup installs it. The Android goals also add their
  target.
- The Makefile gives the configuration to cargo with `--config`.
- The Makefile gives the cmake files to `cargo ndk`, with the NDK
  environment (`NDK_HOME`, default `/opt/android-ndk`).

The project does not repeat these items. Only the editor does not use
make. The rust-analyzer of an IDE uses the default toolchain, unless the
project has its own `rust-toolchain.toml`. This file is a copy of the file
in the package.

The lock file is part of the project. `sol init` makes
`runtime/Cargo.lock` from the lock file of the release. The first build
adjusts it to the manifest of the runtime. When you commit it, it sets the
crates that a build on a different computer uses. The JavaScript engine is
one of these crates. When `CI` is set, the Makefile builds with
`--locked`. Use `LOCKED=1` to do this on all computers, or `LOCKED=0` to
stop it. Thus, if the lock file does not agree with `Cargo.toml`, the
build fails. It does not resolve the crates again.

The engine is important because `sol pack` compiles the app with its own
`fluxc`, and the runtime must load that bytecode. The host build writes
the fingerprint of the engine in `dist/engine.txt` (`solidrt --engine`).
`sol pack` refuses a runtime if its fingerprint is different from the
fingerprint of the compiler.

In the checkout, `runtime/` is the stock program. It is this crate with no
modules, and it has the same one-line Makefile.
`lattice/examples/custom_runtime.rs` is the smallest example of a module.
It has a tick, a bytes binding and an async binding. It runs as all custom
runtimes run.

## Capabilities, not platforms

Not all modules work on all platforms. For example, a headless Flux build
has no camera, microphone, audio or GPU. To find if a capability is
available, use its feature name. Do not guess from the OS:

```js
if (Flux.capabilities.includes("camera")) { /* ... */ }
```

## Run a program

The Flux binary runs a JavaScript file directly. To run TypeScript
sources, bundle them for the bare runtime:

```sh
sol bundle --flux src/main.ts   # -> dist/bundle/main.flux.js
sol pack --flux src/main.ts     # standalone executable
```

[Tools](/tools/) describes the two commands.

## Reference

The reference has one page for each declaration file. The pages are in
three groups:

- [Modules](/runtime/modules/): the `flux:*` capabilities.
- [Standards](/runtime/standards/): the web-standard globals.
- [GUI](/runtime/gui/): the render tree, the devices and the GPU surface.

The pages show the declarations. `@solidrt/flux-types` has a doc comment on
each member. Thus, the text that your editor shows on hover is also the
text of this reference.
