# Build a custom runtime

An advanced guide, for developers who are comfortable with Rust and with
building native code, on the platforms they ship to. It starts with the easy
part, a runtime of your own with only the features you want, and builds on
that by extending it with native code of your own. The app stays the same
app: this adds a Rust project beside it, and takes nothing away.

## When you need one

The stock runtime covers most apps: UI, 2D and 3D, camera, audio, video,
files, HTTP, SQLite, peer-to-peer. Reach for your own when you want:

- a leaner binary, with only the features the app uses,
- your own fonts built into every app on the runtime,
- or, going further, your own native code: a library (an inference engine, a
  codec, a physics solver) or a platform SDK (a vendor's camera or sensor SDK
  on Android).

If the code is portable compute that compiles to WebAssembly, you may not need
the last one: `flux:wasm` runs the same `.wasm` file on every platform.

Nothing here is an eject. You do not move out of the project you have, and the
app code does not change. Apps that never needed more never see any of this.

## What you need

Bun still runs the app. A custom runtime is a Rust project, so building it
adds:

- Rust, through [rustup](https://rustup.rs/). The project pins its toolchain,
  and rustup installs the pinned version the first time you build.
- `make` and a C toolchain, since the runtime compiles the native libraries
  it is built on.
- For Android: the Android NDK (point `NDK_HOME` at it) and `cargo-ndk`.

You only need these on the machine that builds the runtime. The people who
run your app need nothing.

## 1. Set up a runtime

In a new project:

```sh
bun create solidrt my-app --runtime
```

Or in the project you already have, from its folder:

```sh
bun run sol init --runtime
```

This adds a `runtime/` folder and points `package.json` at the folder the
build will fill. The files are yours. The build itself is one line in
`runtime/Makefile` that includes the build base shipped in `@solidrt/cli`, so
an upgrade never leaves your copy stale.

## 2. Choose your features

The capabilities of the runtime are features of the `lattice` crate, chosen
on its line in `runtime/Cargo.toml`. The line the scaffold writes takes the
stock set. To leave something out, turn the defaults off and name everything
you keep:

```toml
lattice = {
  git = "https://github.com/wellawaretech/solidrt",
  tag = "v...",
  default-features = false,
  features = ["video", "wasm-native"],
}
```

The features:

- `video`: video playback.
- `video-encode`: video recording (`record` of `flux:video`). Implies `video`.
- `ktx2`: compressed textures.
- `p2p-relay`: the self-hosted relay (`serveRelay` of `flux:p2p`).
- `wasm-native`: `flux:wasm` compiled to machine code where the target allows
  it, interpreted elsewhere. `wasm` alone is the interpreter everywhere.

The player you develop against carries every feature whatever the runtime
lists, so `bun run dev` does not catch an import of something you trimmed. That
shows up on the device, where `Flux.capabilities` tells the truth.

The fonts every app on the runtime starts from are chosen the same way, in
`fonts()` in `runtime/src/lib.rs`: drop the stock faces or add your own. See
[The fonts](/runtime/#the-fonts).

## 3. Build and run it

```sh
make -C runtime
```

This builds the player and the runtime for this machine into `runtime/dist/`.
The first build compiles everything and takes several minutes. Then the app
runs on it, with nothing to configure:

```sh
bun run dev
bun run test
```

`sol run`, `sol test`, `sol pack` and `sol android` all find your runtime
through `package.json`.

## 4. Ship it

For what you ship, build with the optimized profile:

```sh
make -C runtime PROFILE=release-opt
```

It is much slower to build and gives a smaller, faster binary: fat LTO, one
codegen unit, stripped. For a phone or tablet, build the Android runtime too:

```sh
make -C runtime android
```

Then `bun run android` installs your own player on the device, and
`bun run pack --apk` packs the app with your runtime inside. The player is
named after your app id, so it sits beside the stock Player and the players of
your other projects instead of replacing them.

Commit `runtime/Cargo.lock`. It fixes the crates a build on another machine
resolves, so two builds of the same project give the same runtime.
[Building and shipping it](/runtime/#building-and-shipping-it) has the
profiles, the build times, and the lock in full.

## 5. Extend it with your own code

The scaffold wrote a starter module, `runtime:hello`, with a test
(`tests/runtime.test.tsx`) that imports it, so `bun run test` already proves
the whole path works. In app code a module is an ordinary import:

```tsx
import { hello } from "runtime:hello"

<text>{hello()}</text>
```

A module is a Rust file in `runtime/src/`. Open `runtime/src/hello.rs` and
add a second function beside `hello`. This one sums the bytes of a buffer:

```rust
use lattice::flux;
use lattice::flux::rquickjs::module::{Declarations, Exports, ModuleDef};
use lattice::flux::rquickjs::{Ctx, Function, Result, TypedArray};

/// checksum(bytes: Uint8Array): number
fn checksum<'js>(ctx: Ctx<'js>, bytes: TypedArray<'js, u8>) -> Result<f64> {
  let data = flux::bytes_of(&ctx, &bytes, "checksum")?;
  Ok(data.iter().map(|&b| b as u64).sum::<u64>() as f64)
}
```

Declare it and export it in the module:

```rust
fn declare(decl: &Declarations<'_>) -> Result<()> {
  decl.declare("hello")?;
  decl.declare("checksum")?;
  Ok(())
}

fn evaluate<'js>(ctx: &Ctx<'js>, exports: &Exports<'js>) -> Result<()> {
  exports.export("hello", Function::new(ctx.clone(), || "Hello from the runtime".to_string())?)?;
  exports.export("checksum", Function::new(ctx.clone(), checksum)?)?;
  Ok(())
}
```

Describe it for TypeScript in `runtime/types/hello.d.ts`:

```ts
declare module "runtime:hello" {
  export function hello(): string
  /** The sum of the bytes. */
  export function checksum(bytes: Uint8Array): number
}
```

Test it in `tests/runtime.test.tsx`:

```tsx
import { checksum } from "runtime:hello"

test("checksum sums the bytes", async () => {
  expect(checksum(new Uint8Array([1, 2, 3]))).toBe(6)
})
```

Then `make -C runtime` and `bun run test`. After the first build, a change to
a module alone rebuilds in about fifteen seconds.

A new module is a new file with a `mod` line and one `add` line in
`runtime/src/lib.rs`:

```rust
mod physics;

pub fn modules() -> lattice::Modules {
  lattice::Modules::new()
    .add("runtime:hello", hello::HelloModule)
    .add("runtime:physics", physics::PhysicsModule)
}
```

A module can also read buffers in place, return promises for work on another
thread, tick every frame, and reach the camera, the microphone, and the
renderer. [Writing a module](/runtime/#writing-a-module) has each shape, and
the traps that come with native libraries.

## Where to go from here

- [Writing a module](/runtime/#writing-a-module) for every binding shape, the
  frame tick, and camera and microphone taps.
- [Building and shipping it](/runtime/#building-and-shipping-it) for profiles,
  capability flags, build times, and the lock.
- [The fonts](/runtime/#the-fonts) for building a typeface set into the
  runtime.
