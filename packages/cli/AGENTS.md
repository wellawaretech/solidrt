# @solidrt/cli - agent notes

Dense, self-contained facts for running and verifying a SolidRT app. Full docs
live in docs/ (and the website). For the authoring model (elements, props,
reactivity), see @solidrt/core (its AGENTS.md).

`srt` is the dev tool. Bun is a dev prerequisite only; SolidRT apps run on the
bundled `flux` runtime, not on Bun. Invoke via `bunx srt <command>`.

## Commands

- `bunx srt init <dir>` - scaffold a new SolidRT project into a new (empty)
  folder: package.json, tsconfig.json, AGENTS.md, a starter src/index.tsx, an
  empty assets/ (everything in it ships with the app), then installs deps. Greenfield shortcut (no install needed first):
  `bun create solidrt <dir>`.
- `bunx srt run src/index.tsx` - dev server + a local client window, watches and
  hot-reloads. NEEDS A DISPLAY (opens a GUI window). Not usable headless.
- `bunx srt bundle src/index.tsx` - transpile to `<file>.srt.js`. With
  `--compile`, emits `.srt.bin` bytecode. `--minify`, `--dev`, `--stdout`,
  `--output` also available.
- `bunx srt render src/index.tsx [flags]` - render OFFSCREEN to PNG frames,
  optionally replaying a `--script` file recorded via `--capture`.
- `bunx srt server [file]` / `bunx srt client` - the two halves of `run`
  separately (server distributes code; clients on other devices connect to it).
- `bunx srt run src/index.tsx --capture out.script.json` - records keydown/keyup
  from every connected client into one script file (written on client
  disconnect), for replaying later with `render --script`. The file is JSON
  Lines and hand-authorable: one object per line,
  `{"after": <ms since previous event>, "type": "keydown" | "keyup",
  "key": <W3C KeyboardEvent.key>, "device": <client id>}`. For probing app
  state without a display, `-- <args...>` reaches the app as `flux:process`
  argv, which is often simpler than scripting input.

## Verifying without a display (headless / CI / agent box)

Two reliable checks that need no GUI:

1. `bunx srt bundle src/index.tsx` - exit 0 means the app compiles. Fast.
2. `bunx srt render src/index.tsx --size 480x640 --duration 1 --fps 2` -
   renders offscreen via EGL and writes `frame-NNNNNN.png`. This actually
   proves the app renders. Combine with `--fps`/`--duration` (defaults
   1280x720, 60fps, 1s). No display needed: rendering uses SDL's offscreen
   driver, or alloy's own EGL pbuffer where that driver cannot go headless
   (see the ANGLE gotcha below).

Also headless: the bundled flux runtime runs a plain `.js` file directly -
`node_modules/@solidrt/<platform>/flux script.js` (e.g.
`@solidrt/linux-x64-gnu`). No display, no dev server, full `flux:*` module
access. The right tool for micro-benchmarks and for probing flux module
behavior in isolation.

`render` gotchas:
- Frames land in the directory you ran the command from; `-o <path>` picks a
  different directory (or a path prefix for the `-NNNNNN.png` names).
- `--size` is physical output pixels: layout runs at exactly that size
  (display scale is pinned to 1), so frames are identical on every machine.
- Run from the project directory. There is no `bunx --cwd` flag.
- On ANGLE stacks (Windows, macOS) SDL's offscreen driver cannot go
  headless (no EGL device enumeration), so `render` there builds its own
  EGL pbuffer context behind SDL's dummy video driver instead; the log says
  "using a headless EGL context". If that also fails it renders into a
  hidden window, which needs an interactive desktop session (fails under a
  service, in Session 0, or over SSH-only). Verified headless on Linux
  (Wayland) and the pbuffer path on Windows from a desktop session; a
  non-interactive Windows session and macOS are unverified.

## Sessions (parallel dev servers on one machine)

- `-s <N>` / `--session <N>` (default 0) picks the dev server: port
  `34884 + N`. Valid on `run`, `server`, `client`, `mcp`. `srt run -s1` is a
  second, fully independent dev setup; `srt client -s1 -c2` attaches another
  client to it.
- `-c <N>` / `--client <N>` picks the client data tree, defaulting to the
  session number. (`--compile` gave its short to `--client`.)
- Dev state lives in `~/.solidrt/`: `servers/<port>/` holds each server's tunnel
  key and `live.json` (the registry record MCP resolution reads, removed at
  exit), `clients/client<M>/` the client trees (srt passes
  `--data-root ~/.solidrt/clients` to every locally spawned client).
- A server run serves the project it was started in; `load` outside the
  project root is refused. Restart the server in another project to switch.
- `srt mcp` needs no port: each tool call resolves the server serving the
  project the bridge runs in (registry match + probe); `-s`/`--port` pin it.

## Dev server proxies (when clients on other devices need your machine's data)

- `--proxy-http` - route `fetch` through the dev server; responses cached in
  `.srt-data/http-cache.db` in the project root (delete the file to clear).

## REPL (opened by `run`/`server`)

`load <file>`, `reload [n]`, `stop [n]`, `list`, `!<cmd>`, `quit`/`exit`.
`load` is bound to the project root the server was started in.
