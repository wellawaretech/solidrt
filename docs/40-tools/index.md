# Tools

Developer tooling for SolidRT. Today that is `srt`, one command-line tool
covering the whole cycle: scaffold, develop, inspect, ship. A GUI is planned
alongside it.

A scaffolded project wires the common commands into scripts, so day-to-day
work is `bun run dev`, `bun run android`.

## Develop

```sh
srt run src/index.tsx
```

Starts the dev server and a local client window against it. The server
watches your sources and assets and pushes changes to every connected
client, so one server can drive a desktop window and a phone at the same
time.

Split them when you need to:

```sh
srt server src/index.tsx        # server only
srt client --server 192.168.1.5 # client only, pointed at that server
srt client --android            # install and launch on a connected device
```

Useful flags: `--size WxH` for the window, `--stats` for the FPS and memory
overlay, `--tunnel` to accept clients over a peer-to-peer connection instead
of the local network, `--proxy-http` to route the app's `fetch` calls
through the dev server, `--port N` to move the server off its default 34884
so a second project can run beside the first.

## Inspect

```sh
srt mcp
```

Exposes the running app to a coding agent over MCP: logs (source-mapped back
to your TSX), stats, the live render tree, screenshots, GPU resources, and
any debug commands the app itself registers. A scaffolded project ships an
`.mcp.json`, so an agent can attach to your running app without setup.

That connection is why SolidRT keeps the app inspectable from outside: an
agent working on your app should be able to look at what it is actually
doing, not just at the source.

## Record and replay

```sh
srt run src/index.tsx --capture session.json
srt render src/index.tsx --script session.json --fps 60 --duration 5
```

`--capture` records input from connected clients to a script. `srt render`
replays it headlessly and writes frames, which makes bug reports
reproducible and turns an interaction into a video.

## Ship

```sh
srt check src/index.tsx    # build and typecheck, write nothing
srt bundle src/index.tsx   # transpile to JS or bytecode
srt pack src/index.tsx     # standalone executable (experimental)
```

`srt bundle --flux` and `srt pack --flux` target the bare
[Flux runtime](/runtime/) instead of a SolidRT app, for scripts and servers
with no UI.

## Reference

Every command and flag, pulled from the CLI's own usage text:
[Reference](/tools/reference/). `srt` with no arguments prints the same.
