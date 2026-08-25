# @solidrt/cli

Developer tooling for SolidRT: `srt`, one command-line tool covering the
whole cycle of a `@solidrt/core` application: scaffold, develop, inspect,
ship. A GUI inspector, itself a SolidRT app, is in progress alongside it.

> LLM agents: `AGENTS.md` in this package is a dense, self-contained quickstart.

Bun is a dev prerequisite only; apps run on the bundled `flux` runtime.
Invoke via `bunx srt <command>`. A scaffolded project wires the common
commands into scripts, so day-to-day work is `bun run dev`, `bun run
android` (and `server`, `client`, `pack` for the rest).

## Commands

| Command | |
| --- | --- |
| [`srt init <dir>`](src/init/docs.md) | scaffold a new project into a new (empty) folder |
| [`srt run [file]`](src/server/docs.md) | dev server + local client window |
| [`srt server [file]`](src/server/docs.md) | dev server only |
| [`srt client`](src/client/docs.md) | client only, attached to the project's dev server |
| [`srt android`](src/android/docs.md) | install and launch the client on a connected Android device |
| [`srt check [file]`](src/check/docs.md) | build and typecheck, writing nothing |
| [`srt bundle [file]`](src/bundle/docs.md) | transpile to JS or bytecode (dist/bundle/) |
| [`srt render [file]`](src/render/docs.md) | render frames offscreen, optionally replaying a script |
| [`srt pack [file]`](src/pack/docs.md) | bundle + compile to a standalone executable (experimental) |
| [`srt mcp`](src/mcp/docs.md) | MCP server (stdio) exposing the running dev server to agents |

`srt --help` lists every command and option; `srt --version` prints the
version. Run from the project root to work on the project (its entry is
`solidrt.entry` in package.json, default `src/index.tsx`); pass a file to
work on that file on its own.

## Develop

```sh
srt run
```

Starts the dev server and a local client window against it. The server
pushes the bundle to every connected client, so one server can drive a
desktop window and a phone at the same time; edits reach them on an
explicit reload (the MCP `reload` tool) or on save.

Split them when you need to:

```sh
srt server                        # server only
srt client                        # client only, the project's server (from its root)
srt client --server 192.168.1.5:34884  # client only, pointed at that address
srt android                       # install and launch on a connected device
```

## Inspect

```sh
srt mcp
```

Exposes the running app to a coding agent: logs, stats, the live render
tree, screenshots, GPU resources, input injection, a virtual-time transport,
reload, and the app's own debug commands. A scaffolded project ships an
`.mcp.json`, so Claude Code attaches with no setup.

## Record and replay

```sh
srt run --capture session.json
srt render --script session.json --fps 60 --duration 5
```

`--capture` records key events from connected clients to a script; `render`
replays it headlessly and writes frames, which makes bug reports
reproducible and turns an interaction into a video.

## Ship

```sh
srt check .                # build and typecheck every entry, no build output
srt bundle                 # transpile to JS or bytecode
srt pack                   # standalone executable (experimental)
```

`srt bundle --flux` and `srt pack --flux` target the bare Flux runtime
instead of a SolidRT app, for scripts and servers with no UI.

## Layout

One folder per command under `src/`, named for what it does; the first line
of each says its runtime. `src/server/` is the dev server, a flux script
(`bun`-free at runtime, its own tsconfig); every other command runs on bun.
`src/lib/` is what the bun commands share, `src/types/` the type-only
contracts between the two runtimes. Each command folder carries its
`docs.md` (this site); the depth agents need lives in `agents/` at the
package root, as in the other packages.
