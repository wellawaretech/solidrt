# CLI

Developing SolidRT applications begins here.
This package provides the `srt` command and pulls the relevant platform binaries for compiler and runtime.
As a prerequisite you need to install [Bun](https://bun.sh).
Next, install `@solidrt/cli` either globally or as a development dependency in your project:

```sh
bun add -d @solidrt/cli
```

_Bun is not used for running SolidRT applications; SolidRT comes with its own platform-independent JavaScript runtime called `flux`._

Development of SolidRT applications involves a _development server_ and a _client_ called `solidrt-go`.
The dev server watches your source files, transpiles them and distributes them to connected clients. The clients will then run your code in an environment with extra features for development.

The common workflow is to use the `run` command which will start both the dev server and a local runtime. Next, start clients on other devices and connect them to the dev server. Change your code and see all devices update instantly.

## Proxies

SolidRT provides two independent proxies to route traffic through the dev server:

- `--proxy-http` - routes `fetch` calls through the dev server. HTTP responses are cached automatically in an SQLite file named `.srt-cache.db`. To clear the cache, delete that file.
- `--proxy-files` - routes `Flux.file`, `Flux.dir`, and `Flux.write` calls through the dev server, giving all connected clients access to the files on your development machine. USE WITH CARE!

With multiple devices connected, caching is especially useful: a resource fetched once is served from cache on every subsequent reload across all clients.

## Command-line interface

Start `srt` via `bunx`:

```sh
bunx srt <command> [options]
```

| Command   | Description                                                                        |
| --------- | ---------------------------------------------------------------------------------- |
| `server`  | Start dev server only                                                              |
| `client`  | Start a local client only                                                          |
| `run`     | Start both dev server and a local client, and run the file                         |
| `bundle`  | Transpile a `.tsx` file to JavaScript, or compile JavaScript to bytecode           |
| `package` | Package the program, assets, and runtime into a standalone distributable (planned) |
| `record`  | Capture frames from a `.tsx` file to produce a video                               |

### Command `server`

Starts the dev server. Usage:

```sh
bunx srt server [flags] [file]
```

When `file` is provided, it is transpiled and pushed to all connected clients immediately. The file is then watched for changes and automatically re-transpiled and pushed on save.

When running, a REPL is started. See section Dev server REPL.

| Flag            | Description                                              |
| --------------- | -------------------------------------------------------- |
| `--proxy-files` | Route file/dir access through the dev server             |
| `--proxy-http`  | Route fetch calls through the dev server (cache enabled) |

### Command `client`

Starts a local client. Usage

```sh
bunx srt client [flags]
```

| Flag     | Description          |
| -------- | -------------------- |
| `--size` | Window size as `WxH` |

### Command `run`

Starts both the dev server and a client. It takes the same options as `server` and `client` commands.

```sh
bunx srt run [flags] [file]
```

### Command `bundle`

Transpile a `.tsx` file to JavaScript, or compile JavaScript to bytecode.

```sh
bunx srt bundle [flags] <file>
```

| Flag        | Description                                            |
| ----------- | ------------------------------------------------------ |
| `--dev`     | Use development build of SolidJS (default: production) |
| `--compile` | Compile the output to bytecode                         |
| `--minify`  | Minify the output                                      |
| `--output`  | Output filename without extension                      |
| `--stdout`  | Write output to stdout instead of a file               |

Without `--compile`, the output is a `.srt.js` file. With `--compile`, the output is a `.srt.bin` bytecode file that can be loaded directly by the runtime without further compilation.

By default the production build of SolidJS is used. Pass `--dev` to use the development build, which includes extra runtime invariants and an infinite-loop guard.

### Command `package`

> Not yet available. Planned for a future release.

Package a SolidRT application into a standalone distributable. The package includes the compiled program, all assets, and the SolidRT runtime.

```sh
bunx srt package <file.tsx> [options]
```

### Command `record`

Write frames to disk instead of showing on screen. Usage:

```sh
bunx srt record <file.tsx>
```

Files are written as `png` with file names `frame-<index>.png`.
Combine them to form a video, for instance using `ffmpeg`:

```sh
ffmpeg -framerate 60 -i frame-%06d.png -c:v libx264 -crf 18 -pix_fmt yuv420p out.mp4
```

| Flag             | Description                        |
| ---------------- | ---------------------------------- |
| `--size <WxH>`   | Frame size (default: `1280x720`)   |
| `--fps <N>`      | Frames per second (default: `60`)  |
| `--duration <N>` | Duration in seconds (default: `1`) |

## Development server REPL

When starting the development server, an interactive REPL opens. The REPL lets you manage connected clients and load files without restarting.

| Command         | Description                                      |
| --------------- | ------------------------------------------------ |
| `load <file>`   | Load and push a `.tsx`, `.srt.js`, or `.srt.bin` |
| `reload`        | Rebuild and push to all clients                  |
| `stop`          | Stop all clients                                 |
| `list`          | List connected clients with platform and version |
| `help`          | Show available REPL commands                     |
| `!<cmd>`        | Run a shell command                              |
| `quit` / `exit` | Shut down the server and exit                    |
