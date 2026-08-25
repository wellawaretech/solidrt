# Tools

Developer tooling for SolidRT. Today that is `srt`, one command-line tool
covering the whole cycle: scaffold, develop, inspect, ship. A GUI
inspector, itself a SolidRT app, is in progress alongside it.

A scaffolded project wires the common commands into scripts, so day-to-day
work is `bun run dev`, `bun run android` (and `server`, `client`, `pack` for
the rest).

## Develop

```sh
srt run
```

Starts the dev server and a local client window against it, from the
project root (the entry is `solidrt.entry` in package.json, default
`src/index.tsx`); `srt run <file>` serves a single file outside a project.
The server pushes the bundle to every connected client, so one server can
drive a desktop window and a phone at the same time; edits reach them on an
explicit reload (the MCP `reload` tool).

Split them when you need to:

```sh
srt server                        # server only
srt client                        # client only, the project's server (from its root)
srt client --server 192.168.1.5:34884  # client only, pointed at that address
srt client --android              # install and launch on a connected device
```

Useful flags: `--size WxH` for the window, `--stats` for the FPS and memory
overlay, `--tunnel` for the server to accept clients over a peer-to-peer
connection instead of the local network, `--proxy-http` to route the app's
`fetch` calls through the dev server, `--lan` to reach the server from other
devices (loopback only by default), `--port N` to pin the port (otherwise
the server keeps the one it had, else takes the first free one from 34884
up, so projects run side by side without any numbering), and `-- <args>` to hand the app its own arguments
(`flux:process` argv).

## Inspect

```sh
srt mcp
```

Exposes the running app to a coding agent over MCP: logs (source-mapped back
to your TSX), stats, the live render tree, screenshots and texture readback,
GPU resources, input injection, a virtual-time transport (`step_frames`,
`set_time_scale`), reload, and any debug commands the app itself registers.
A scaffolded project ships an `.mcp.json`, so Claude Code attaches to your
running app with no setup. Other agents keep their server list in their own
file; point it at `bun node_modules/@solidrt/cli/bin/srt mcp`, run from the
project root. `agents/debugging.md` in this package lists the file per
client.

That connection is why SolidRT keeps the app inspectable from outside: an
agent working on your app should be able to look at what it is actually
doing, not just at the source.

## Record and replay

```sh
srt run --capture session.json
srt render --script session.json --fps 60 --duration 5
```

`--capture` records key events from connected clients to a script (pointer
input is not captured yet). `srt render` replays it headlessly and writes
frames, which makes bug reports reproducible and turns an interaction into
a video.

## Ship

```sh
srt check src/index.tsx    # build and typecheck, no build output
srt check .                # every entry under a folder (src/index.tsx, examples/*)
srt bundle                 # transpile to JS or bytecode
srt pack                   # standalone executable (experimental)
```

`srt bundle --flux` and `srt pack --flux` target the bare
[Flux runtime](/runtime/) instead of a SolidRT app, for scripts and servers
with no UI. `bundle` takes `--dev`, `--minify`, `--compile` (bytecode) and
`-o`; `pack --folder` writes a flat runner + manifest + bundle + assets
folder instead of one executable.

## Reference

Every command and flag, pulled from the CLI's own usage text:
[Reference](/tools/reference/). `srt` with no arguments prints the same.
