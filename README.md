# SolidRT

[![CI](https://github.com/wellawaretech/solidrt/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/wellawaretech/solidrt/actions/workflows/ci.yml)
[![version](https://img.shields.io/npm/v/@solidrt/cli?label=version)](https://www.npmjs.com/package/@solidrt/cli)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue)](LICENSE)
[![Platforms](https://img.shields.io/badge/platforms-Linux%20%7C%20macOS%20%7C%20Windows%20%7C%20Android-informational)](#project-status)

The Solid Runtime - a modern toolkit for building cross-platform applications with SolidJS.

What if you want to build an application that runs everywhere? The options thin out fast. React Native is primarily iOS and Android. Electron is desktop only, and ships a browser to get there. Flutter runs everywhere, but you write Dart and buy into Google's ecosystem to do it. Most of the remaining options hand rendering to the system webview, which is a browser again: a different one on every operating system, and a different one after the next OS update.

Most frameworks that promise "everywhere" are a renderer with a UI library on top. SolidRT is the full package: one runtime, identical on every platform, with everything an application needs built in. Development is built for humans and agents working together: both see and drive the same live application, on every connected device at once, and a built-in test framework runs it headless, in CI as on your desk. On top of that come a themable set of standard components, 2D and 3D extensions in the same tree as the interface, and a runtime you can extend with native modules of your own.

Not a webview, not a browser engine, and not a renderer waiting for you to fill in the rest. Write SolidJS, ship native applications. Runs today on Linux, macOS, Windows, and Android.

The thinking behind it is written up in [HTML: New Beginnings](https://wellawaretech.com/blog/html-new-beginnings/).

## Quick start

SolidRT development runs on [Bun](https://bun.sh), so install that first. Nothing else is needed: the runtime binary for your platform comes with the CLI.

Bun is a development-time tool only. Applications run on the SolidRT runtime itself, which embeds its own JavaScript engine - a shipped application carries no Bun.

```sh
bun create solidrt@latest my-app
cd my-app
bun run dev
```

`bun run dev` starts the dev server and a local client in one go. The scaffold brings its own `@solidrt/cli`, so no global install is needed.

To preview the same app on another machine or device, install the tooling there with `bun add -g @solidrt/cli@latest` - that pulls the runtime binary for the platform and gives you the `sol` command - then connect it to your dev server:

```sh
sol client --server <host>:<port>
```

For a connected Android device, `sol android` installs and launches the client for you. Run `sol` without arguments for the full command list.

## How it works

**Alloy, the rendering and platform layer.** A Rust core that owns the render tree, so JavaScript describes what to draw and never touches the drawing itself. The render tree is SolidRT's own: layout (flexbox and grid), text layout, compositing with damage tracking and repaint boundaries, and hit testing, with GPU targets, camera and video frames, and 3D scenes as nodes in the same tree. Impeller, Flutter's renderer, draws the vector content, and the GPU is directly reachable, with vertex and fragment shaders as first-class citizens. SDL handles the platform: windowing, input, and the rest of the surface an application needs from the operating system.

**Built for 2D and 3D, not just decorated with them.** A spatial core carries both, in the same tree as the interface. The 2D extension binds your atlases together and draws thousands of sprites in a single draw call. The 3D extension is a library in the spirit of Three.js, running directly on the GPU with no web platform in between: scene graphs, materials, cameras. Fragment and vertex shaders compile at runtime, from source, while your application is running. That makes it a foundation for real 3D work: visualization, simulation, modeling, anything where the scene is the application rather than an ornament on top of it.

**Flux, the JavaScript engine.** Built on QuickJS and embeddable on its own, Flux gives your components access to the system: WebAssembly, SQLite, HTTP, peer-to-peer, filesystem, subprocesses, camera, microphone and video, enough to build a video call. The same on every platform, with no plugin per platform. Isolates make parallel work simple: call a function in another context and await the result, or consume an async generator, while the UI keeps rendering.

**Updates without a new binary.** An application is JavaScript and assets on a native runtime, so a new version is a download, not a new binary: no store round trip for every fix. Frameworks that compile your logic into the binary ship a new binary every time. Over-the-air delivery is on the roadmap; dev pushes already install versions the same way.

**A runtime of your own.** The stock runtime is one call into Lattice, the crate that binds Alloy, Forge and Flux. When it is not enough, `sol init --runtime` puts a small cargo project beside your app that makes the same call with native Rust modules of your own, and `sol run`, `sol test`, `sol pack` and `sol android` pick it up from there.

**One dev server, every device at once.** Connect any client to your development server: an Android phone, a tablet, a TV, a Windows or macOS machine, a Raspberry Pi, or several windows at different sizes on your own desk. No simulators, no emulators, no platform-specific project setup. Agents reach all of them over MCP, so "why is this slow on that device" becomes a question you can hand off rather than one you chase.

**Testing, built in.** `sol test` runs your app headless on the real runtime: real layout, real rendering, and the real input pipeline, with taps, drags, keys, gamepads and back going through hit testing and focus as physical input does. Tests are deterministic, with no sleeps and no retries, and need no display, so they run on a plain CI runner as they do on your desk. The system boundary is faked, not your code: a test decides what the camera sees, answers file dialogs, and delivers links as the OS would. Tests name nodes by what the user sees, and a failure lists what was there instead.

## Agents see the application, not a screenshot

SolidRT ships MCP tooling wired into every layer, and every project is set up for it from the start. An agent can read the live render tree, snapshot any part of it, inject debug commands into a running application, and drive it with real keyboard, mouse, and gamepad events. It controls time as well: slow the application down, pause it, and step forward one frame at a time, so an animation or a transition can be inspected frame by frame instead of guessed at. That is the actual state of the application, at the level the renderer sees it. Agents can build, run, and test complete applications with minimal intervention. Developer tooling exposing the same view is on the way.

## Project status

SolidRT is in alpha and under active development. Expect APIs to change and new surface to land: it is being built in the open, and it is moving.

- **iOS is not supported yet.** There is no technical reason for that. Every platform runs the same rendering path, so iOS is work that has not been done rather than a problem that needs solving.
- **Agents are the best way to take it for a spin today.** The MCP integration already works extremely well: an agent can scaffold, build, run and drive a real application with little intervention.
- **The extension packages move fastest.** `@solidrt/components`, `@solidrt/2d`, `@solidrt/3d` and `@solidrt/router` are younger than the core and will change more than it does.
- **Over-the-air updates are on the roadmap.** The version store they install into is in place; signed delivery and rollback are still to come.
- **The focus is shifting towards the human experience.** Documentation and developer tooling, the parts of the workflow a person touches directly.

## Documentation

See [docs](docs/) for the full documentation: it is the website content, one
markdown file per page.

## Repository structure

**Rust crates**

- `alloy` - rendering layer: SDL, Impeller, and glow (GL), plus the render tree and layout engine (taffy)
- `forge` - what a headless process can do, OS-specific code included (HTTP, sqlite, fs, subprocess, p2p, audio and video decoding, ...): the capability cores Flux builds on
- `flux` - JavaScript runtime (QuickJS-based): exposes Alloy and Forge through `flux:*` modules, embeddable and standalone
- `lattice` - the runtime itself: binds Alloy, Forge and Flux, drives the window and the frame loop, loads and sandboxes the packed app, and adds the `sol:*` modules
- `runtime` - the stock program over Lattice (crate `solidrt`), with two binaries:
  - `solidrt` - what an application ships with
  - `solidrt-go` - the development client: the same runtime, connected to a dev server, with the player and the debug tooling on top. Also built as an Android app, with iOS to follow

**npm packages**

- `@solidrt/core` - links SolidJS with Lattice; the main package for application developers
- `@solidrt/cli` - developer tooling
- `@solidrt/test` - tests for a SolidRT app, run headless with `sol test`
- `@solidrt/flux-types` - TypeScript type definitions for the Flux runtime
- `create-solidrt` - project scaffolding (`bun create solidrt@latest`)
- `solidrt` - holds the name on npm; the code lives under the `@solidrt` scope

**npm extension packages**

- `@solidrt/components` - higher-level components built on the core primitives
- `@solidrt/2d` - instanced sprite layer: your atlases bound together, thousands of sprites in a single draw call
- `@solidrt/3d` - retained 3D scene graph: meshes, materials and a camera as Solid components
- `@solidrt/router` - screens as routes: a typed route tree, a stack, links in and the location out

**Platform packages**

- `@solidrt/linux-x64-gnu` - Linux x64 (glibc)
- `@solidrt/linux-arm64-gnu` - Linux arm64 (glibc)
- `@solidrt/darwin-arm64` - macOS arm64
- `@solidrt/win32-x64-msvc` - Windows x64 (MSVC)
- `@solidrt/android-arm64-v8a` - Android arm64
- `@solidrt/android-armeabi-v7a` - Android arm 32-bit
- `@solidrt/android-x86_64` - Android x86_64 (emulators, Chromebooks)

## Development

To work on SolidRT itself, see [DEVELOPMENT.md](DEVELOPMENT.md) for prerequisites, setup, and build instructions, and [CONTRIBUTING.md](CONTRIBUTING.md) for contribution guidelines.

## Sponsoring

SolidRT is free and open source. If it is useful to you or your company, consider [sponsoring the project on GitHub](https://github.com/sponsors/wellawaretech) - it helps keep development going.

Sponsorship funds platform support, developer tooling, rendering architecture, performance work, documentation, and the unglamorous maintenance that keeps a project alive between features. SolidRT is developed entirely in the open: everything sponsorship funds ships publicly, for everyone. There is nothing exclusive behind a tier.

## Acknowledgements

SolidRT stands on a lot of excellent work by other people.

The runtime is written in [Rust](https://www.rust-lang.org), on top of [tokio](https://tokio.rs) - every asynchronous capability the JavaScript side reaches for ends up there.

**Core technologies**

- [SolidJS 2.0](https://www.solidjs.com) - the reactivity system and the universal renderer SolidRT plugs into. 
- [QuickJS](https://github.com/quickjs-ng/quickjs) - the JavaScript engine every application runs on, embedded through the [rquickjs](https://crates.io/crates/rquickjs) bindings.
- [SDL3](https://libsdl.org) - windowing, input, audio and camera, on every platform SolidRT targets, through the [sdl3](https://crates.io/crates/sdl3) bindings.
- [Impeller](https://docs.flutter.dev/perf/impeller) - Flutter's 2D renderer, which draws the vector content of the render tree, via the [impellers](https://crates.io/crates/impellers) bindings.
- [glow](https://crates.io/crates/glow) - GL bindings, loaded from SDL's GL proc addresses.
- [taffy](https://crates.io/crates/taffy) - the flexbox layout engine behind every layout node.

**Various dependencies**

- [hyper](https://hyper.rs) and [reqwest](https://crates.io/crates/reqwest) - HTTP server and client, with [rustls](https://crates.io/crates/rustls) for TLS
- [fastwebsockets](https://crates.io/crates/fastwebsockets) - WebSocket server and client
- [SQLite](https://sqlite.org) - embedded database, through [rusqlite](https://crates.io/crates/rusqlite)
- [iroh](https://www.iroh.computer) - peer-to-peer connectivity and the dev-server tunnel
- [wasmtime](https://wasmtime.dev) and [wasmi](https://crates.io/crates/wasmi) - the WebAssembly engines behind `flux:wasm`: wasmtime compiles modules to machine code, wasmi interprets them where generated code cannot run

The full set is in `Cargo.toml` and `package.json`. Every one of these is worth a look in its own right.

## License

MIT. Copyright (c) 2026 Antoine van Wel.
