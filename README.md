# SolidRT

The Solid Runtime - a modern toolkit for building cross-platform applications with SolidJS.

What if you want to build an application that runs everywhere? The options thin out fast. React Native is primarily iOS and Android. Electron is desktop only, and ships a browser to get there. Flutter runs everywhere, but you write Dart and buy into Google's ecosystem to do it. Most of the remaining options hand rendering to the system webview, which is a browser again: a different one on every operating system, and a different one after the next OS update.

SolidRT takes a different path: JavaScript you already know, Flutter's renderer underneath, and a GPU that is genuinely open to you rather than sealed behind the framework. All of it on desktop and mobile, from one codebase.

Write SolidJS, ship native applications. SolidRT is not a webview and not a browser engine. Your components run on an embedded JavaScript engine and render straight to the GPU. Runs today on Linux, macOS, Windows, and Android.

## Quick start

SolidRT development runs on [Bun](https://bun.sh), so install that first. Nothing else is needed: the runtime binary for your platform comes with the CLI.

Bun is a development-time tool only. Applications run on the SolidRT runtime itself, which embeds its own JavaScript engine - a shipped application carries no Bun.

```sh
bun create solidrt@latest my-app
cd my-app
bun run dev
```

`bun run dev` starts the dev server and a local client in one go. The scaffold brings its own `@solidrt/cli`, so no global install is needed.

To preview the same app on another machine or device, install the tooling there with `bun add -g @solidrt/cli@latest` - that pulls the runtime binary for the platform and gives you the `srt` command - then connect it to your dev server:

```sh
srt client --server <host>
```

For a connected Android device, `srt android` installs and launches the client for you. Run `srt` without arguments for the full command list.

## How it works

**Alloy, the rendering and platform layer.** A Rust core that owns the render tree, so JavaScript describes what to draw and never touches the drawing itself. Rendering goes through Impeller, Flutter's renderer, alongside a layout engine and direct access to the GPU where shaders are first class citizens. SDL handles the platform: windowing, input, and the rest of the surface an application needs from the operating system.

**Built for 3D, not just decorated with it.** SolidRT ships a 3D library in the spirit of Three.js, running directly on the GPU with no web platform in between. Fragment and vertex shaders compile at runtime, from source, while your application is running. That makes it a foundation for real 3D work: visualization, simulation, modeling, anything where the scene is the application rather than an ornament on top of it.

**Flux, the JavaScript engine.** Built on QuickJS and embeddable on its own, Flux gives your components access to the system: FFI, WebAssembly, SQLite, networking, filesystem, and more. Isolates make parallel work simple: call a function in another context and await the result, or consume an async generator, while the UI keeps rendering.

**One dev server, every device at once.** Connect any client to your development server: an Android phone, a tablet, a TV, a Windows or macOS machine, a Raspberry Pi, or several windows at different sizes on your own desk. No simulators, no emulators, no platform-specific project setup. Agents reach all of them over MCP, so "why is this slow on that device" becomes a question you can hand off rather than one you chase.

## Agents see the application, not a screenshot

SolidRT ships MCP tooling wired into every layer, and every project is set up for it from the start. An agent can read the live render tree, snapshot any part of it, inject debug commands into a running application, and drive it with real keyboard, mouse, and gamepad events. It controls time as well: slow the application down, pause it, and step forward one frame at a time, so an animation or a transition can be inspected frame by frame instead of guessed at. That is the actual state of the application, at the level the renderer sees it. Agents can build, run, and test complete applications with minimal intervention. Developer tooling exposing the same view is on the way.

## Project status

SolidRT is in alpha and under active development. Expect APIs to change and new surface to land: it is being built in the open, and it is moving.

- **iOS is not supported yet.** There is no technical reason for that. Every platform runs the same rendering path, so iOS is work that has not been done rather than a problem that needs solving.
- **Agents are the best way to take it for a spin today.** The MCP integration already works extremely well: an agent can scaffold, build, run and drive a real application with little intervention.
- **The extension packages move fastest.** `@solidrt/components`, `@solidrt/2d` and `@solidrt/3d` are younger than the core and will change more than it does.
- **The focus is shifting towards the human experience.** Documentation and developer tooling, the parts of the workflow a person touches directly.

## Documentation

See [docs](docs/) for the full documentation: it is the website content, one
markdown file per page.

## Repository structure

**Rust crates**

- `alloy` - rendering layer: SDL, Impeller, and glow (GL), plus the render tree and layout engine (taffy)
- `forge` - engine-independent capability cores (HTTP, sqlite, p2p, fs, events, ...) that Flux builds on
- `flux` - JavaScript runtime (QuickJS-based): exposes Alloy and Forge through `flux:*` modules, embeddable and standalone
- `lattice` - the runtime itself, one crate with two binaries:
  - `solidrt` - what an application ships with: drives the window and the frame loop, loads and sandboxes the packed app, and adds the `srt:*` modules
  - `solidrt-go` - the development client: the same runtime, connected to a dev server, with the launcher and the debug tooling on top. Also built as an Android app, with iOS to follow

**npm packages**

- `@solidrt/core` - links SolidJS with Lattice; the main package for application developers
- `@solidrt/cli` - developer tooling
- `@solidrt/flux-types` - TypeScript type definitions for the Flux runtime
- `create-solidrt` - project scaffolding (`bun create solidrt@latest`)

**npm extension packages**

- `@solidrt/components` - higher-level components built on the core primitives
- `@solidrt/2d` - instanced sprite layer: one atlas, thousands of sprites in a single draw call
- `@solidrt/3d` - retained 3D scene graph: meshes, materials and a camera as Solid components

**Platform packages**

- `@solidrt/linux-x64-gnu` - Linux x64 (glibc)
- `@solidrt/linux-arm64-gnu` - Linux arm64 (glibc)
- `@solidrt/darwin-arm64` - macOS arm64
- `@solidrt/win32-x64-msvc` - Windows x64 (MSVC)
- `@solidrt/android-arm64-v8a` - Android arm64
- `@solidrt/android-armeabi-v7a` - Android arm 32-bit

## Development

To work on SolidRT itself, see [DEVELOPMENT.md](DEVELOPMENT.md) for prerequisites, setup, and build instructions, and [CONTRIBUTING.md](CONTRIBUTING.md) for contribution guidelines.

## Sponsoring

SolidRT is free and open source. If it is useful to you or your company, consider [sponsoring the project on GitHub](https://github.com/sponsors/wellawaretech) - it helps keep development going.

Sponsorship funds platform support, developer tooling, rendering architecture, performance work, documentation, and the unglamorous maintenance that keeps a project alive between features. SolidRT is developed entirely in the open: everything sponsorship funds ships publicly, for everyone. There is nothing exclusive behind a tier.

## Acknowledgements

SolidRT stands on a lot of excellent work by other people.

The runtime is written in [Rust](https://www.rust-lang.org), on top of [tokio](https://tokio.rs) - every asynchronous capability the JavaScript side reaches for ends up there.

**Core technologies**

- [SolidJS](https://www.solidjs.com) - the reactivity system and the universal renderer SolidRT plugs into. 
- [QuickJS](https://github.com/quickjs-ng/quickjs) - the JavaScript engine every application runs on, embedded through the [rquickjs](https://crates.io/crates/rquickjs) bindings.
- [SDL3](https://libsdl.org) - windowing, input, audio and camera, on every platform SolidRT targets, through the [sdl3](https://crates.io/crates/sdl3) bindings.
- [Impeller](https://docs.flutter.dev/perf/impeller) - the renderer. Every pixel SolidRT puts on screen goes through it, via the [impellers](https://crates.io/crates/impellers) bindings.
- [glow](https://crates.io/crates/glow) - GL bindings, loaded from SDL's GL proc addresses.
- [taffy](https://crates.io/crates/taffy) - the flexbox layout engine behind every layout node.

**Various dependencies**

- [hyper](https://hyper.rs) and [reqwest](https://crates.io/crates/reqwest) - HTTP server and client, with [rustls](https://crates.io/crates/rustls) for TLS
- [fastwebsockets](https://crates.io/crates/fastwebsockets) - WebSocket server and client
- [SQLite](https://sqlite.org) - embedded database, through [rusqlite](https://crates.io/crates/rusqlite)
- [iroh](https://www.iroh.computer) - peer-to-peer connectivity and the dev-server tunnel
- [wasmi](https://crates.io/crates/wasmi) - the WebAssembly interpreter behind `flux:wasm`

The full set is in `Cargo.toml` and `package.json`. Every one of these is worth a look in its own right.

## License

MIT. Copyright (c) 2026 Antoine van Wel.
