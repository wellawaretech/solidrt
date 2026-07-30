# SolidRT

A low-level toolkit for creating cross-platform applications.

_SolidRT is in alpha: useful today, but APIs are still stabilizing._

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

## Usage

Scaffold a project with `bun create solidrt@latest`, or see [@solidrt/core](packages/core/README.md) for getting started from scratch.

## Documentation

See [docs](docs/index.md) for the full documentation.

## Development

### Prerequisites

- [Bun](https://bun.sh) - for development only
- [Rust](https://rustup.rs) - for building `solidrt-go` and the runtime

### Setup

```sh
bun install
```

### SRT_HOME

Set `SRT_HOME` to the root of this repository so the CLI picks up locally built binaries instead of the published npm packages:

```sh
export SRT_HOME=/path/to/solidrt
```

With `SRT_HOME` set, `srt run` and similar commands will resolve binaries from `dist/<platform>/` - the output of the build steps below.

### Building

Run from the repo root:

| Command                         | Description                          |
| ------------------------------- | ------------------------------------ |
| `make client`                   | Build the `solidrt-go` client binary |
| `make client PROFILE=debug`     | Build with debug symbols             |
| `make runtime`                  | Build the production runtime         |

Binaries are staged into `dist/<platform>/` after a successful build.

See [CONTRIBUTING.md](CONTRIBUTING.md) for contribution guidelines.

## License

MIT. Copyright (c) 2026 Antoine van Wel.

