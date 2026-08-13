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

## Sessions

Several dev servers can run on one machine at once, each with its own clients.
A session is one number that picks both: `srt run -s1` starts a dev server on
port `34884 + 1` with a client using data tree 1, in whatever project you run
it from. The default session is 0, so single-server use needs no flags.

- `-s, --session <N>` selects the dev server (port `34884 + N`). Valid on
  `run`, `server`, `client` and `mcp`.
- `-c, --client <M>` selects the client data tree, defaulting to the session
  number. Pass it to attach a second client to the same session:
  `srt client -s1 -c2`.
- `--port <P>` still wins where valid; the session then only supplies the
  client slot.

Dev state lives in `~/.solidrt/`: `servers/<port>/` holds each server's identity
(its p2p tunnel key, stable across restarts on the same port) and a record of
the running server, and `clients/client<M>/` holds the client data trees.
Deleting `~/.solidrt` resets all of it.

A server run serves the project it was started in; to work on another
project, start a server there (another session runs both at once). The MCP
bridge (`srt mcp`) needs no port configuration: each tool call finds the
server currently serving the project the bridge runs in, so agent config
never changes per session.

## Proxies

- `--proxy-http` - routes `fetch` calls through the dev server. HTTP responses are cached automatically in an SQLite file at `.srt-data/http-cache.db` in the project root. To clear the cache, delete that file.

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
| `package`  | Package the program, assets, and runtime into a standalone distributable (planned) |
| `render`   | Replay a script (optional) and render frames from a `.tsx` file to produce a video |

### Command `server`

Starts the dev server. Usage:

```sh
bunx srt server [flags] [file]
```

When `file` is provided, it is transpiled and pushed to all connected clients immediately. The file is then watched for changes and automatically re-transpiled and pushed on save.

When running, a REPL is started. See section Dev server REPL.

| Flag             | Description                                              |
| ---------------- | -------------------------------------------------------- |
| `-s, --session <N>` | Session number: port `34884 + N` (default: 0)         |
| `--port <N>`     | Dev server port (default: `34884` + session)             |
| `--proxy-http`   | Route fetch calls through the dev server (cache enabled) |
| `--capture <file>` | Record connected clients' key events to a script file  |
| `-- <args...>`   | App arguments, pushed to every client with the app       |

Everything after a bare `--` is the app's argument vector, exposed as
`flux:process` `argv`. It rides every push of the session, so remote clients
see the same arguments as the local one. Relative paths in the arguments
resolve inside the app's data sandbox (the runtime changes into it before app
code runs), so pass absolute paths when you mean a location on the invoking
machine.

`--capture` records keyboard input (keydown/keyup) from every connected
client into one script file, for replaying later with `render --script`.
Events are streamed to disk as they happen (one JSON object per line, [JSON
Lines](https://jsonlines.org/) format) rather than buffered in memory. Each
line is `{"after": <ms since previous event, integer>, "type": "keydown" |
"keyup", "key": "...", "device": <client id>}`. `key` is a W3C
`KeyboardEvent.key` value ("Enter", "ArrowLeft", "a") — the same string apps
observe in their `onKeyDown` handlers.

### Command `client`

Starts a local client. Usage

```sh
bunx srt client [flags]
```

| Flag     | Description          |
| -------- | -------------------- |
| `-s, --session <N>` | Session of the dev server to connect to (default: 0) |
| `-c, --client <N>` | Client number: its own data tree under the data root (default: the session) |
| `--data-root <dir>` | Client data root (default: `~/.solidrt/clients`) |
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

### Command `render`

Write frames to disk instead of showing on screen, optionally replaying a
script recorded with `--capture` (see the `server`/`run` flags above). Usage:

```sh
bunx srt render <file.tsx> [--script <file>]
```

Files are written as `png` with file names `frame-<index>.png`.
Combine them to form a video, for instance using `ffmpeg`:

```sh
ffmpeg -framerate 60 -i frame-%06d.png -c:v libx264 -crf 18 -pix_fmt yuv420p out.mp4
```

| Flag              | Description                                         |
| ----------------- | ---------------------------------------------------- |
| `--script <file>` | Script file to replay (default: no scripted input)   |
| `--size <WxH>`    | Frame size (default: `1280x720`)                      |
| `--fps <N>`       | Frames per second (default: `60`)                     |
| `--duration <N>`  | Duration in seconds (default: `1`)                    |
| `-- <args...>`    | App arguments (`flux:process` `argv`)                 |

`--duration` is an upper bound: an app that calls `exit()` (from `srt:app`)
ends the render run early, so a tool that captures, writes its output, and
exits does not need to guess a frame count. App arguments follow the same
sandbox rule as under `run`: pass absolute paths.

## Development server REPL

When starting the development server, an interactive REPL opens. The REPL lets you manage connected clients and load files without restarting.

| Command         | Description                                      |
| --------------- | ------------------------------------------------ |
| `load <file>`   | Load and push a `.tsx`, `.srt.js`, or `.srt.bin` (inside the project root) |
| `reload`        | Rebuild and push to all clients                  |
| `stop`          | Stop all clients                                 |
| `list`          | List connected clients with platform and version |
| `help`          | Show available REPL commands                     |
| `!<cmd>`        | Run a shell command                              |
| `quit` / `exit` | Shut down the server and exit                    |
