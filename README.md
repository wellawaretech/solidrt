# SolidRT

The Solid Runtime - a modern toolkit for building cross-platform applications with SolidJS.

Write your application once, preview it live on several devices at the same time from your development machine. No simulators, no emulators, no platform-specific project setup.

SolidRT is AI-native. The runtime is deeply integrated with coding agents over MCP, and every project is wired up for it from the start: an agent can observe and drive the running application the same way you do, so it can work on its own.

_SolidRT is in alpha: APIs are still stabilizing._

## Quick start

SolidRT development runs on [Bun](https://bun.sh), so install that first. Nothing else is needed: the runtime binary for your platform comes with the CLI.

Bun is a development-time tool only. Applications run on the SolidRT runtime itself, which embeds its own JavaScript engine - a shipped application carries no Bun.

```sh
bun create solidrt@latest my-app
cd my-app
bun run dev
```

`bun dev` runs `srt run src/index.tsx`, which starts the dev server and a local client in one go. The scaffold brings its own `@solidrt/cli`, so no global install is needed.

To preview the same app on another machine or device, install the tooling there with `bun add -g @solidrt/cli@latest` - that pulls the runtime binary for the platform and gives you the `srt` command - then connect it to your dev server:

```sh
srt client --server <host>
```

For a connected Android device, `srt client --android` installs and launches the client for you. Run `srt` without arguments for the full command list.

## Documentation

See [docs](docs/) for the full documentation: it is the website content, one
markdown file per page.

## Repository structure

**Rust crates**

- `alloy` - rendering layer: SDL, Impeller, and glow (GL), plus the render tree and layout engine (taffy)
- `forge` - engine-independent capability cores (HTTP, sqlite, p2p, fs, events, ...) that Flux builds on
- `flux` - JavaScript runtime (QuickJS) with `flux:*` capability modules; embeddable and standalone
- `lattice` - ties Alloy and Flux together; exposes Alloy's render tree to JavaScript via a command API

**npm packages**

- `@solidrt/core` - links SolidJS with Lattice; the main package for application developers
- `@solidrt/components` - higher-level components built on core
- `@solidrt/cli` - developer tooling
- `@solidrt/flux-types` - TypeScript type definitions for the Flux runtime
- `create-solidrt` - project scaffolding (`bun create solidrt@latest`)

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

## Acknowledgements

SolidRT stands on a lot of excellent work by other people.

The runtime is written in [Rust](https://www.rust-lang.org), on top of [tokio](https://tokio.rs) - every asynchronous capability the JavaScript side reaches for ends up there.

**Core technologies**

- [SolidJS](https://www.solidjs.com) - the reactivity system and the universal renderer SolidRT plugs into. SolidRT is named after it, and would not exist without it.
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
