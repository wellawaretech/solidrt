---
title: WebAssembly
description: The route for app code that needs more than the JavaScript interpreter, front to back - what a module is written in and how it reaches a project, when it runs on the JS thread and when it runs in an isolate, the flux:wasm contract and why it stays narrow, which workloads this enables and which it cannot touch, and the engine underneath with the on-device compilation model and its measured cost. Read before touching flux:wasm, forge::wasm, or anything that compiles or runs a wasm module.
created: 2026-10-03
---

# WebAssembly

An app whose cost is interpreted compute has two places to go:

| route | what it is for | sandbox | artifacts |
|---|---|---|---|
| `flux:wasm` | portable app compute | yes | one |
| custom runtime build | engine internals, platform SDKs, a GPU API | no | your own build |

wasm is the default answer for "this module needs to be fast". A custom
runtime build is the answer for anything that has to touch engine state or
reach outside the sandbox. There is no third option that ships per-platform
binaries into an app: `flux:ffi` did, and was removed
([ffi-module-removal](../done/ffi-module-removal.md)).

## 1. Source in

**wasm is a compilation target, not a language.** Something has to be compiled
to it, and that choice decides whether the project keeps its bun-only
property.

- **Rust** is the recommended source language. `wasm32-unknown-unknown` is the
  most hermetic Rust target there is: rust-lld is bundled, so no system
  linker, no NDK, and no cross-compilation matrix. One target, one install,
  against the seven prebuilt shared libraries the native route needed.
- **C, C++ or Zig through `zig cc`** is about 50 MB and fully hermetic, so it
  could be managed on demand the way Android platform-tools already are.
  Cheap toolchain, harder language sell for app logic.
- **AssemblyScript** is the only option that keeps bun-only literally, since
  it installs with `bun add`. It is not TypeScript (its own GC, manual memory
  discipline, small stdlib) and being a GC'd language compiled to wasm it will
  not reach Rust's throughput. Worth knowing it exists; not the default.
- **TinyGo** works and is another toolchain for a language not otherwise used.

**Decision: a toolchain requirement is per module, not per project.** An app
that uses no wasm module still needs only bun, and that property is worth
protecting. Authoring a module is allowed to need rustup.

Two project shapes, and the second is how most app developers will meet this:

- **Authored in-repo**: a sub-project with its own toolchain and build command,
  emitting a `.wasm` into `assets/`, which the asset pipeline already collects
  wholesale. `sol` may build it when the toolchain is present and must never
  require it otherwise.
- **Consumed prebuilt**: a module published as an npm package carrying one
  `.wasm`, added with `bun add`. The app developer then needs no toolchain at
  all; only the module author does. This is the same distribution shape as the
  native platform packages with one artifact instead of seven, which is the
  whole reason wasm is worth the trouble.

What does not fit the module contract: default emscripten output, which
imports its memory, and `wasm-bindgen` output. `emcc -sSTANDALONE_WASM=1
--no-entry` produces a module that fits.

**Known gap**: the boundary is hand-marshalled. See section 3.

## 2. Running a module

**`flux:wasm` calls are synchronous.** `instance.call` returns values, not a
Promise, so a call freezes rendering and input for its duration
(`packages/core/agents/performance.md`). There is no async interface, and the
isolate is the only concurrency available.

That does not make the isolate the default, because it costs the property that
makes the per-frame case attractive:

| | on the JS thread | in an isolate |
|---|---|---|
| latency | synchronous, same frame | promise, a frame behind |
| data | zero-copy into live buffers | Sendable copies, shared-nothing |
| blocking | freezes the frame | never |
| fits | short per-frame compute | long or bursty work |

**The rule is call duration against copy cost.** A solver stepping a few
hundred bodies at 1 to 3 ms fits a 16.7 ms frame and should run directly, so
it can write poses straight into an instance stream mirror with one crossing
and no copies. Inference at 7 to 17 ms per frame cannot run on the JS thread
at all and belongs in an isolate, where latest-result-wins is acceptable and
the copy is negligible against the compute.

**Rejected: running every module in an isolate.** Uniform and simple, but it
removes the zero-copy path, and a promise result means a per-frame module is
always a frame stale by construction.

## 3. The flux:wasm contract

The surface is narrow on purpose: scalar-signature function imports only, no
imported memory, globals or tables. `Module` and `Instance`, `call`,
`callIndirect`, `memory` (an ArrayBuffer aliasing live guest bytes, no copy),
`readMemory` and `writeMemory`.

**Decision: the boundary stays narrow, and rich data rides as pointers.** The
scalar restriction is not an obstacle to structured data, because every
bindgen scheme reduces to i32 pointers into linear memory at the wasm
boundary anyway; `wasm-bindgen` keeps scalar signatures and marshals in JS.
Widening the interface was considered and rejected: the richness belongs in
generated glue over a narrow boundary, which is also the conclusion the FFI
work reached before it.

What the current API already supports, with no changes:

- pass or return a struct: write into `memory`, pass an i32 pointer
- strings: UTF-8 through memory
- host-side allocation: the guest exports an allocator and the host calls it
- guest callbacks carrying structured data: it passes pointers, the host reads
- guest function pointers: `callIndirect`
- several results: `call` returns an array

**Known gap: there is no glue generator.** An author writes raw `extern "C"`
exports and moves structs through linear memory by hand. The missing piece is
a Rust macro that exports entry points and emits a schema, plus codegen for a
typed `.d.ts` and a JS wrapper doing the pointer arithmetic, in the spirit of
UniFFI. This is tooling on top of the existing API, not an extension of it,
and it is the practical blocker for anyone outside the project writing a
module. No backlog item yet.

Three things that would be real extensions:

- **SIMD** is on since 2026-10-04, measured at +0.61 MB: wasmi's `simd`
  cargo feature in `forge/Cargo.toml`, which the engine config turns into
  the `simd` and `relaxed-simd` proposals once compiled in. v128 still never
  crosses the host boundary. SIMD is most of what makes wasm competitive for
  inference and physics, so the engine swap keeps it on in both modes.
- **Async with zero copy** is the one extension with a real motivation behind
  it: the engine runs the instance on its own thread while the host keeps a
  view into its linear memory, with explicit fencing so the app cannot read
  while a call is in flight. Feasible in principle, since a wasmtime `Store`
  can move between threads and the guest stays single-threaded. Only worth it
  if a per-frame module turns out to need more than a frame's slack. No
  backlog item yet.
- **wasm threads** (shared memory plus atomics) for multiple cores inside one
  module. A large commitment, awkward alongside ahead-of-time compilation, and
  nothing asks for it.

Not extensions: an allocator convention is a contract between generator and
guest, and precompiled artifacts are hidden inside the loader (section 5).

## 4. What this enables, and what it cannot touch

Enabled, all of it coarse-grained compute over buffers:

- a physics solver stepping into an instance stream mirror
- pathfinding, procedural generation, a simulation tick
- audio DSP, image and video processing
- CPU inference
- ports of existing C or C++. The canonical shape is a program with a
  framebuffer: input in as scalars, frames out through linear memory straight
  into `uploadTexture` with no copy. The lua64 cartridge is the in-tree proof.

**Not enabled: the dominant cost of a typical app.** Measured in
[ffi-crossing-costs](../notes/ffi-crossing-costs.md), a 3000-node mount is
118 ms of which about 108 ms is Solid's component and effect machinery plus
the core renderer's bookkeeping, interpreted by QuickJS. A module does not
touch any of that. Moving the renderer or reactivity into wasm is not an
option either: Solid is JS, and the rendertree bindings are JS-facing, so a
module would have to call back out through host imports to create a node.

So this changes the equation for apps with a compute core and changes nothing
for apps whose cost is UI churn.

How much the first half gains is measured in
[wasm-vs-js-throughput](../notes/wasm-vs-js-throughput.md): compute in a
module runs 18x to 960x faster than the same loop in QuickJS on the native
lane and 4x to 88x on the interpreted lane, while a host call out of the
module costs 3x to 7x a plain JavaScript call.

**Not enabled: engine state.** Camera frames reach JS only as a texture id,
scene writes go through the transform path with its damage marking and frame
latch, and a sandboxed module has no route to a GPU API. Vulkan-backed
inference is permanently outside wasm whatever the compiler does. All of that
is a custom runtime build.

**Not enabled: call-heavy designs.** Every host crossing is marshalled. One
coarse call per frame handing over a buffer, never N calls per entity.

## 5. Backend

**Until 2026-10-04**: wasmi 2.0.0 alone, a register-based interpreter with no
JIT, which is why `flux:wasm` was documented as a portability tool and not a
speed tool; 1.39 MB of the runtime. The wording was wrong, not the engine:
measured against QuickJS, even that lane runs compute 4x to 88x faster
([wasm-vs-js-throughput](../notes/wasm-vs-js-throughput.md)).

**Decided 2026-10-04** ([wasm-native-execution](../done/wasm-native-execution.md)):
two engines behind the one `forge::wasm` API, exactly one per build.
wasmtime with Cranelift wherever a native backend exists and runtime codegen
is allowed; wasmi, with SIMD, everywhere else. The Cranelift pipeline:

```
.wasm -> parse/validate -> CLIF -> optimize -> select for target
      -> emit -> place in memory -> run
```

Alongside it sits wasmtime's runtime proper: memories, tables, traps, host
trampolines, store lifecycle. The two engines are kept to one proposal set,
the one wasmi validates (SIMD and relaxed SIMD on; no exceptions, GC, threads
or stack switching), so a module that validates on one lane validates on
every lane and the lanes differ in speed only.

**Rejected: Pulley as the interpreted lane.** Pulley is a Cranelift target
whose machine is a software VM, so one engine could in principle cover both
lanes with one implementation of wasm semantics. Two facts killed it. Pulley
bytecode is the output of a Cranelift compile, so an interpreting device
still carries the compiler (roughly the desktop cost, not nothing; wasmtime
without Cranelift is a 0.43 MB loader that runs only artifacts compiled
elsewhere, and shipping those is a non-goal below) and still compiles at
install: 3.3 s for a 287 KB module on the armeabi-v7a TV, where wasmi
translates it in 79 ms. And it is not faster: on that TV it beats wasmi by 12
to 29 percent on integer and memory loops and loses by 1.4x to 2.1x on float,
host calls and SIMD, while on x86_64 (relevant only to a desktop dev switch)
it is 2 to 6x slower than wasmi, because its fast dispatch loop needs tail
calls stable Rust does not guarantee. Numbers and method in
[wasm-interpreter-throughput](../notes/wasm-interpreter-throughput.md). The
price of the decision is two engines' worth of bugs. A build carries one of
them, never both: a desktop exercises the interpreted lane by building with
the feature off, the same engine the small targets run.

**Compilation happens on the device, at install.** The payload stays one
portable `.solapp` carrying the `.wasm`; pack never compiles and never
cross-compiles. The compiled form is a cache, the same relationship a shader
cache has to a shader: `<app cache dir>/wasm/<engine hash>/<module
sha256>.cwasm`, the module hash being the one the manifest already stores per
asset and the engine hash a sha256 digest of wasmtime's compatibility hash
(version, target, tunables, features). It lives in the app's cache dir as an
embedder opt-in like every other disk cache, so bare `flux` never writes, and
it is fail-soft like `lattice/src/gl_libs.rs`: a cache that cannot be read or
written means compiling in memory each launch. A compile failure on a valid
module is an error, as a parse failure is today; the interpreter is a lane
for a target, not a fallback for a module. The store pre-warms every `.wasm`
asset of a version right after its hashes are verified and its dir committed,
and prunes the artifacts no held version references alongside its own
version prune; with artifacts at 2.5x to 3x the wasm and deduped by hash,
the cache is bounded by the store's retention and has none of its own.
Because one client hosts many apps, the compiler is a shared cost per
machine rather than per app.

Lanes per target:

| target | lane |
|---|---|
| desktop x64 and arm64, android-arm64 | wasmtime, Cranelift-compiled native |
| iOS | wasmi (store policy forbids runtime codegen) |
| android-armeabi-v7a | wasmi (no 32-bit ARM backend exists) |

The lane is a build fact: the `wasm-native` cargo feature on forge, set per
target by the `WASM_NATIVE` knob in `lattice/Makefile`, and off by target
where there is no backend. The feature selects the engine the build carries;
the other is compiled out. `Flux.capabilities` lists `wasm-native` on the
native lane. The engine is one shared handle per process, made by the
embedder and handed to the flux engine builder and to the store's install;
the plugin reads it off the engine config next to the cache dir.

**iOS has no in-process wasm at all through any system framework.** JIT has
never been enabled for JavaScriptCore API clients, and JSC's wasm
implementation requires JIT. The one JIT-compiled wasm engine on the platform
runs inside `WKWebView`'s content process, behind a process boundary with no
shared memory, which removes the per-frame and zero-copy properties that would
make it worth reaching for. Store policy independently permits downloaded code
only in an interpreter. So iOS interprets, or a project statically links a
wasm2c translation into its own signed app binary.

The app contract is unaffected, which is what keeps
[ios-port-readiness](../backlog/ios-port-readiness.md) intact: `flux:wasm`
runs a module everywhere and only the speed differs.

### Measured cost

Four `release-opt` builds of the `solidrt` runtime on the linux-x64-gnu
builder, one working-tree base, fat LTO with one codegen unit, stripped.
wasmtime 48.0.5 with `default-features = false` and the features `runtime`,
`std`, `pulley`, `cranelift`: no component model, async, GC, backtrace
symbolication or parallel compilation, so these are the lean numbers:

| configuration | size | vs today |
|---|---|---|
| wasmi, no simd (the base at the time) | 41.90 MB | - |
| wasmi + `simd` (today) | 42.51 MB | +0.61 MB |
| wasmi **and** wasmtime/Cranelift | 51.13 MB | +9.23 MB |
| wasmtime/Cranelift, no wasmi | 49.73 MB | **+7.8 MB** |

Compressed, measured on both binaries: +2.71 MB deflate, +1.93 MB zstd-19.
The delta compresses better than the binary average (0.347 and 0.247 against
0.423 and 0.333). That is the wire cost only; an APK stores native libraries
`Stored`, uncompressed, so the on-device footprint is the full amount.

**Almost nothing is reclaimed by LTO or by sharing.** An isolated probe crate
predicted the in-tree swap to within half a megabyte. The reason is what
Cranelift's bulk is: ISLE compiles the instruction-selection rules into
generated Rust, a decision trie per backend plus a second rule set for the
mid-end rewrites, and it is all genuinely reachable because any module can
contain any IR pattern. There is no dead code to prune and no duplication to
fold.

**Rejected: Winch**, wasmtime's baseline compiler. It uses no ISLE, no
egraph optimizer and no separate register allocator, so it would be a
fraction of Cranelift's footprint while still emitting native code, but it
exists for hosts where compiling fast matters more than running fast. This
route is about running fast, and with compile-once-at-install compile speed
buys nothing, so its size was not measured and the compiler is Cranelift.

### Deliberate non-goals

- **Shipping compiled artifacts.** Considered and left out: a single reusable
  `.solapp` is the point. It stays available later at no cost, because an
  ahead-of-time producer only pre-populates a cache slot the loader already
  handles. Two mechanisms with different triggers: a `.cwasm` pack flag for
  desktop and arm64 Android (triggered by the single-file packed executable,
  which pays the compiler per app, or by an AAB where bundletool splits per
  ABI on Play's side), and wasm2c plus static linking for iOS and anything
  forbidding runtime codegen.
- **A per-platform app payload.** The `.solapp` is the same bytes everywhere
  and the per-device difference is resolved on the device.

## Known limits and open items

| limit | item |
|---|---|
| packed and OTA apps compile a module at first use, not at install | [client-storage-updates](../plans/client-storage-updates.md) stage 4 |
| precompiled wasm in an AAB | [play-store-aab](../backlog/play-store-aab.md) |
| runtime size budget this competes for | [runtime-optimization](../backlog/runtime-optimization.md) |
| no glue generator | no item yet (section 3) |
| no async-with-zero-copy execution | no item yet (section 3) |
