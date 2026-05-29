# CLI

Developing SolidRT applications starts at
`@solidrt/cli`.
This package provides the `srt` command and pulls the relevant platform binary.
As a prerequisite, for development only, you need to install [Bun](https://bun.sh). 

_Bun is not used for running SolidRT applications; SolidRT comes with its own runtime called `flux`._

Development of SolidRT applications involves a _development server_ and a _client_ called `solidrt-go`. 
The dev server watches your source files, bundles them and distributes them to connected clients. The clients will then run your code in an environment with extra features for development.

The common workflow is to use the `run` command which will start both. 

Start other clients on any device and connect them to the dev server. Now you can test your application on all connected devices simultaneously.


## Command-line interface

Start `srt` via `bunx`:

```sh
bunx srt <command> [options]
```

## The `run` command

Start the dev server and spawn a local `solidrt-go` client.

```sh
bunx srt run [file]
```

When `file` is provided, it is compiled and pushed to all connected clients immediately. The file is then watched for changes and automatically rebuilt and pushed on save.

**Flags**

| Flag       | Description                                                   |
| ---------- | ------------------------------------------------------------- |
| `--server` | Start the dev server only, without a local client             |
| `--client` | Start a local client only, connecting to a running server     |
| `--proxy`  | Route all file, directory, and network requests from connected clients through the dev server |
| `--cache`  | Cache proxied HTTP responses on the dev server                |
| `--size`   | Window size as `WxH` (default: `1280x720`)                    |

`--cache` requires `--proxy`.

**Examples**

```sh
bunx srt run src/index.tsx                      # server + local client, load file
bunx srt run --server src/index.tsx             # server only
bunx srt run --server --proxy --cache src/index.tsx  # server with caching proxy
bunx srt run --client                           # local client only
```

---

## srt build

Bundle a `.tsx` file to JavaScript, or compile JavaScript to bytecode.

```sh
bunx srt build <file>
```

Accepted input:

| Input       | Description                                        |
| ----------- | -------------------------------------------------- |
| `.tsx`      | Bundle with Bun and the Solid plugin               |
| `.srt.js`   | Compile to bytecode                                |

**Flags**

| Flag               | Description                                          |
| ------------------ | ---------------------------------------------------- |
| `-c, --compile`    | Compile the output to bytecode (`.srt.bin`)          |
| `-m, --minify`     | Minify the output                                    |
| `-o, --output`     | Output filename without extension                    |
| `--stdout`         | Write output to stdout instead of a file             |

Without `--compile`, the output is a `.srt.js` file. With `--compile`, the output is a `.srt.bin` bytecode file that can be loaded directly by the runtime without further compilation.

**Examples**

```sh
bunx srt build src/index.tsx                  # -> index.srt.js
bunx srt build src/index.tsx -c               # -> index.srt.bin
bunx srt build src/index.tsx -c -m -o dist/app  # -> dist/app.srt.bin, minified
bunx srt build src/index.srt.js --stdout      # bytecode to stdout
```

---

## srt bundle

> Not yet available. Planned for a future release.

Package a SolidRT application into a standalone distributable. The bundle includes the compiled program, all assets, and the SolidRT runtime - everything needed to run the app on a target platform without any external dependencies or a separate runtime install.

```sh
bunx srt bundle <file.tsx> [options]
```

<!-- TODO: document flags and output format once the command is implemented -->

---

## srt record

Bundle a `.tsx` file, run it, and capture frames to produce a video.

```sh
bunx srt record <file.tsx>
```

**Flags**

| Flag           | Description                               |
| -------------- | ----------------------------------------- |
| `--fps <N>`    | Frames per second (default: `60`)         |
| `--duration <N>` | Duration in seconds (default: `1`)      |
| `--size <WxH>` | Frame size (default: `1280x720`)          |

**Example**

```sh
bunx srt record src/animation.tsx --duration 3 --fps 30
```

## Development server REPL

Running `srt run` opens an interactive REPL alongside the server. The REPL lets you manage connected clients and load files without restarting.

| Command         | Description                                         |
| --------------- | --------------------------------------------------- |
| `load <file>`   | Load and push a `.tsx`, `.srt.js`, or `.srt.bin`    |
| `reload [n...]` | Rebuild and push to all clients, or specific clients by index |
| `stop [n...]`   | Stop all clients, or specific clients by index      |
| `list`          | List connected clients with platform and version    |
| `help`          | Show available REPL commands                        |
| `!<cmd>`        | Run a shell command                                 |
| `quit` / `exit` | Shut down the server and exit                       |

Client indices come from `list`. For example, `reload 0 2` rebuilds and pushes to clients 0 and 2 only.


## The client: `solidrt-go`