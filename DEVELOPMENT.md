# Development

## Prerequisites

- [Bun](https://bun.sh) - for development only
- [Rust](https://rustup.rs) - for building SolidRT-Go and the runtime

## Setup

```sh
bun install
```

## SRT_HOME

Set `SRT_HOME` to the root of this repository so the CLI picks up locally built
binaries instead of the published npm packages:

```sh
export SRT_HOME=/path/to/solidrt
```

With `SRT_HOME` set, `srt run` and similar commands will resolve binaries from
`dist/<platform>/` - the output of the build steps below.

## Building SolidRT-Go

Run from the repo root:

| Command                         | Description                   |
| ------------------------------- | ----------------------------- |
| `make solidrt-go`               | Build the `solidrt-go` binary |
| `make solidrt-go PROFILE=debug` | Build with debug symbols      |

Binaries are staged into `dist/<platform>/` after a successful build.
