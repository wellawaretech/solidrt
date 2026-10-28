# Build a custom runtime

This is an advanced guide. It is for developers who know Rust and know how
to build native code for their target platforms. The guide starts with the
easy part: a runtime of your own, with only the features that you want.
Then it shows how to add your own native code to that runtime. Your app
does not change. The guide adds a Rust project next to the app, and removes
nothing.

## When to use one

The stock runtime is sufficient for most apps. It has UI, 2D and 3D,
camera, audio, video, files, HTTP, SQLite, and peer-to-peer. Use your own
runtime when you want one of these:

- A smaller binary, with only the features that the app uses.
- Your own fonts in each app on the runtime.
- As a more advanced step, your own native code. This can be a library (an
  inference engine, a codec, a physics solver) or a platform SDK (the
  camera or sensor SDK of a vendor on Android).

If your code is portable compute code that compiles to WebAssembly, the
last item is possibly not necessary. `flux:wasm` runs the same `.wasm` file
on all platforms.

None of these steps is an eject. You stay in the project that you have, and
the app code does not change. If the stock runtime is sufficient for an
app, this guide has no effect on that app.

## Necessary software

Bun continues to run the app. A custom runtime is a Rust project. To build
it, you must also have:

- Rust, from [rustup](https://rustup.rs/). The build pins the Rust
  version. When you build for the first time, rustup installs the pinned
  version.
- `make`, `cmake` and a C toolchain. The runtime compiles the native
  libraries that it uses.
- On x64, `nasm`. The `video` feature uses it.
- On Linux, libclang and the development packages of SDL. The
  [CI setup](https://github.com/wellawaretech/solidrt/blob/main/.github/actions/linux-system-deps/action.yml)
  lists them.
- On Windows, MSVC and MSYS2. Run `make` from an MSYS2 shell. In MSYS2,
  install `make`, `diffutils` and `nasm`.
- For Android: the Android NDK and `cargo-ndk`. Set `NDK_HOME` to the NDK
  path.

This software is necessary only on the machine that builds the runtime. The
users of your app do not have to install anything.

## 1. Make a runtime

For a new project, use this command:

```sh
bun create solidrt my-app --runtime
```

For a project that you have, use this command in the project folder:

```sh
bun run sol init --runtime
```

This command adds a `runtime/` folder. It also sets `package.json` to point
to the folder for the build output. You own these files. The build is one
line in `runtime/Makefile`. That line includes the build base in
`@solidrt/cli`. Thus, after an upgrade, your copy is always current.

## 2. Select the features

The capabilities of the runtime are features of the `lattice` crate. You
select them on the `lattice` line in `runtime/Cargo.toml`. The initial line
selects the stock set. To remove a feature, set the defaults to off. Then
write the name of each feature that you keep:

```toml
lattice = {
  git = "https://github.com/wellawaretech/solidrt",
  tag = "v...",
  default-features = false,
  features = ["video", "wasm-native"],
}
```

These are the features:

- `video`: video playback.
- `video-encode`: video recording (`record` of `flux:video`). It includes
  `video`.
- `ktx2`: compressed textures.
- `http`: HTTP with TLS (`fetch`, the `WebSocket` client, and `flux:http`).
  Without it, an app cannot load an image or a video from a URL.
- `p2p`: peer-to-peer connections (`flux:p2p`, and the `p2p` option of
  `serve` in `flux:http`). It also contains TLS, because the connections
  are encrypted. A runtime without TLS removes `http`, `p2p` and
  `p2p-relay`.
- `p2p-relay`: the self-hosted relay (`serveRelay` of `flux:p2p`).
- `wasm-native`: `flux:wasm` on wasmtime, which compiles a module to
  machine code. Some targets do not support wasmtime (armeabi-v7a, x86
  Android, iOS). On these targets, the build uses wasmi. `wasm` alone is
  wasmi, the interpreter, on all targets.

The player that you use during development has all features, whatever the
runtime lists. Thus, `bun run dev` does not find an import of a feature
that you removed. You see that error only in the packed app, on the
desktop and on Android. There, `Flux.capabilities` shows the correct
features.

You select the initial fonts of each app on the runtime in the same way.
Use `fonts()` in `runtime/src/lib.rs` to remove the stock faces or add your
own. Refer to [The fonts](/runtime/#the-fonts).

## 3. Build and run it

```sh
make -C runtime
```

This command builds the player and the runtime for this machine into
`runtime/dist/`. The first build compiles all code and takes several
minutes. After the build, the app runs on the new runtime. No configuration
is necessary:

```sh
bun run dev
bun run test
```

`sol run`, `sol test`, `sol pack`, and `sol android` find your runtime
through `package.json`.

## 4. Prepare a release

For a release, build with the optimized profile:

```sh
make -C runtime PROFILE=release-opt
```

This build takes much more time. It gives a smaller and faster binary: fat
LTO, one codegen unit, stripped. For a phone or tablet, also build the
Android runtime:

```sh
make -C runtime android
```

Before you use Android, set `appId` in `package.json`, for example
`com.example.myapp`. The value must be an Android application id.

Then `bun run android` installs your own player on the device.
`bun run pack --apk` packs the app with your runtime in it. The id of the
player is your app id with `.player` added, for example
`com.example.myapp.player`. Thus, it installs next to the stock Player and
the players of your other projects. It does not replace them.

Commit `runtime/Cargo.lock`. This file sets the crate versions that a build
on a different machine uses. Thus, two builds of the same project give the
same runtime. For full data about the profiles, the build times, and the
lock file, refer to
[Build the runtime](/runtime/#build-the-runtime).

## 5. Add your own code

Step 1 wrote a starter module, `runtime:hello`. It also wrote a test
(`tests/runtime.test.tsx`) that imports the module. Thus, `bun run test`
already shows that the full path works. In app code, you import a module as
usual:

```tsx
import { hello } from "runtime:hello"

<text>{hello()}</text>
```

A module is a Rust file in `runtime/src/`. Open `runtime/src/hello.rs`. Add
a second function next to `hello`. This function calculates the sum of the
bytes in a buffer:

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

Declare the function in the module. Then export it:

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

Write the TypeScript types for it in `runtime/types/hello.d.ts`:

```ts
declare module "runtime:hello" {
  export function hello(): string
  /** The sum of the bytes. */
  export function checksum(bytes: Uint8Array): number
}
```

Step 1 wrote `tests/runtime.test.tsx`. This file already imports `test`
and `expect` from `@solidrt/test`. Add `checksum` to the import from
`runtime:hello`, and add this test:

```tsx
test("checksum sums the bytes", async () => {
  expect(checksum(new Uint8Array([1, 2, 3]))).toBe(6)
})
```

Then run `make -C runtime` and `bun run test`. After the first build, a
change to only a module rebuilds in approximately fifteen seconds.

To add a new module, make a new file. Then add a `mod` line and an `add`
line in `runtime/src/lib.rs`:

```rust
mod physics;

pub fn modules() -> lattice::Modules {
  lattice::Modules::new()
    .add("runtime:hello", hello::HelloModule)
    .add("runtime:physics", physics::PhysicsModule)
}
```

A module can also do these tasks:

- Read buffers in place.
- Return promises for work on a different thread.
- Tick in each frame.
- Access the camera, the microphone, and the renderer.

[Write a module](/runtime/#write-a-module) shows each shape. It also
shows the traps that come with native libraries.

## More information

- [Write a module](/runtime/#write-a-module): all binding shapes, the
  frame tick, and the camera and microphone taps.
- [Build the runtime](/runtime/#build-the-runtime): profiles,
  capability flags, build times, and the lock file.
- [The fonts](/runtime/#the-fonts): how to build a typeface set into the
  runtime.
