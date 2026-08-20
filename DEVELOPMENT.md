# Development

This document covers working on SolidRT itself. For building applications with SolidRT, see the [README](README.md).

## Prerequisites

- [Bun](https://bun.sh) - for development only
- [Rust](https://rustup.rs) - for building `solidrt-go` and the runtime

### Windows

The makefiles are POSIX shell scripts, so run `make` from **Git Bash** (shipped
with [Git for Windows](https://git-scm.com)), not from PowerShell or cmd. On top
of Bun and Rust (MSVC toolchain) you need:

- [Visual Studio Build Tools 2022](https://visualstudio.microsoft.com/downloads/)
  with the "Desktop development with C++" workload. This also ships CMake; put
  `C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\Common7\IDE\CommonExtensions\Microsoft\CMake\CMake\bin`
  on your `PATH` (SDL3 is built from source through CMake).
- GNU Make 4.x: `winget install ezwinports.make`. GnuWin32's Make 3.81 does not
  understand `C:/` paths and fails on `platform.mk`.
- LLVM (for `libclang`, used by bindgen to build QuickJS):
  `winget install LLVM.LLVM --location "$env:LOCALAPPDATA\LLVM"` installs
  without admin rights. Then set the user environment variable
  `LIBCLANG_PATH` to `%LOCALAPPDATA%\LLVM\bin`.

Open a fresh terminal after changing `PATH` or `LIBCLANG_PATH`, then run
`make client` from Git Bash at the repo root. The first build also downloads
an Electron zip (~140 MB) into `dist/win32-x64-msvc/` to extract the ANGLE DLLs.

## Setup

```sh
bun install
```

## SRT_HOME

Set `SRT_HOME` to the root of this repository so the CLI picks up locally built binaries instead of the published npm packages:

```sh
export SRT_HOME=/path/to/solidrt
```

With `SRT_HOME` set, `srt run` and similar commands will resolve binaries from `dist/<platform>/` - the output of the build steps below.

## Building

Run from the repo root:

| Command                         | Description                                             |
| ------------------------------- | ------------------------------------------------------- |
| `make help`                     | List all build targets, including Android and dist       |
| `make client`                   | Build the `solidrt-go` client binary                     |
| `make runtime`                  | Build the production runtime binary                      |

Binaries are staged into `dist/<platform>/` after a successful build.

See [CONTRIBUTING.md](CONTRIBUTING.md) for contribution guidelines.
