# SolidRT

A low-level toolkit for creating cross-platform applications.

_SolidRT is in pre-alpha stage. Anything can and will be changed._

## Repository structure

**Rust crates**

- `alloy` - rendering layer combining SDL, Impeller, and wgpu
- `flux` - embeds a JavaScript runtime (QuickJS) into a Rust application
- `lattice` - ties Alloy and Flux together; exposes rendering to JavaScript via a command API

**npm packages**

- `@solidrt/core` - links SolidJS with Lattice; the main package for application developers
- `@solidrt/cli` - developer tooling

**Platform packages**

- `@solidrt/linux-x64-gnu` - Linux x64 (glibc)
- `@solidrt/darwin-arm64` - macOS arm64
- `@solidrt/win32-x64-msvc` - Windows x64 (MSVC)

## Usage

Install from npm - see [@solidrt/core](packages/core/README.md) for getting started.

## API

See [docs/api.md](docs/api.md) for the full API reference.

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

### Building `solidrt-go`

Run from the repo root:

| Command                         | Description                   |
| ------------------------------- | ----------------------------- |
| `make solidrt-go`               | Build the `solidrt-go` binary |
| `make solidrt-go PROFILE=debug` | Build with debug symbols      |

Binaries are staged into `dist/<platform>/` after a successful build.

See [CONTRIBUTING.md](CONTRIBUTING.md) for contribution guidelines.

## License

MIT. Copyright (c) 2026 Antoine van Wel.

