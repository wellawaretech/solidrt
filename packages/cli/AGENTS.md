# @solidrt/cli - agent notes

Dense, self-contained facts for running and verifying a SolidRT app. The
prose lives in README.md and src/<command>/docs.md (also the website). For
the authoring model (elements, props, reactivity), see @solidrt/core (its
AGENTS.md).

`srt` is the dev tool. Bun is a dev prerequisite only; SolidRT apps run on the
bundled `flux` runtime, not on Bun. Invoke via `bunx srt <command>`.

The dev loop against a running app is pause_watch -> edit -> reload ->
resume_watch -> get_logs -> get_snapshot, with mute_user_input while you
measure or test and unmute_user_input after; agents/debugging.md has the
why of each hold. Several clients may be attached at once: `reload` reaches
all of them, while call_debug / send_input / get_snapshot are per client
(debugging.md). `reload` surfaces build errors but not type errors:
`bunx srt check` is for those.

agents/ carries the depth this one leaves out; read the one that matches
the work before starting it:
- agents/debugging.md - driving a running app over MCP, pointing an agent
  client at the `srt mcp` server, what the dev server serves and its control
  API without MCP (a shell, a CI step), and the debugging lessons that cost
  real time. Read before investigating a bug, verifying a change against the
  live app, or scripting against a server.
- agents/assets.md - the assets/ folder, inlined imports and the bundle
  output; fonts, the `solidrt` package.json identity key, and distribution
  builds. Read before adding an asset or a font, or building to distribute.

## Commands

- `bunx srt init <dir>` - scaffold a new SolidRT project into a new (empty)
  folder: package.json, tsconfig.json, AGENTS.md, a starter src/index.tsx, an
  empty assets/ (everything in it ships with the app), then installs deps. Greenfield shortcut (no install needed first):
  `bun create solidrt <dir>`.
- `bunx srt run` - dev server + a local client window, from the project root
  (entry `solidrt.entry` in package.json, default src/index.tsx); `bunx srt run
  <file>` serves a single file outside a project. NEEDS A DISPLAY (opens a GUI
  window). Not usable headless. Reloads on save (the bundle's inputs and
  `assets/`); an agent pauses that with `pause_watch` and pushes its edits
  with the MCP `reload` tool.
- `bunx srt tool` - list the build-time tools the installed `@solidrt/*`
  packages ship (`<package>/tools/<name>.ts`, named `<package>/<name>`);
  `bunx srt tool <package>/<name> [arguments]` runs one under bun in the
  project, everything after the name passed through as the tool's own
  arguments (each tool prints its own usage on `--help`). What a tool does
  is the package's business (e.g. `3d/model` bakes a glTF into a model
  file); srt only finds and runs them.
- `bunx srt check [file|dir]` - build in memory and typecheck, no output
  and no reload. With a folder it covers every entry under it (src/index.tsx
  and examples/*), so `bunx srt check .` answers "did I break any example"
  in one call. The dev server runs it once at startup without gating on it.
- `bunx srt bundle` - bundle the project into `dist/bundle/` (or
  `--output <dir>`): `<name>.srt.js` plus the app's isolate modules as
  `isolates/<id>.js`. With `--compile`, bytecode (`.srt.bin`/`.bin`)
  instead. Move the dir, not the bare file - a bundle loaded without its
  isolates/ dir loses them (`--stdout` cannot carry them at all).
  `--minify`, `--dev` also available.
- `bunx srt render [flags]` - render the project OFFSCREEN to PNG frames,
  optionally replaying a `--script` file recorded via `--capture`.
- `bunx srt server [file]` / `bunx srt client` - the two halves of `run`
  separately (server distributes code; clients on other devices connect to it).
- `bunx srt run --capture out.script.json` - records keydown/keyup
  from every connected client into one script file (written on client
  disconnect), for replaying later with `render --script`. The file is JSON
  Lines and hand-authorable: one object per line,
  `{"after": <ms since previous event>, "type": "keydown" | "keyup",
  "key": <W3C KeyboardEvent.key>, "device": <client id>}`. For probing app
  state without a display, `-- <args...>` reaches the app as `flux:process`
  argv, which is often simpler than scripting input.

## Verifying without a display (headless / CI / agent box)

Two reliable checks that need no GUI:

1. `bunx srt bundle` - exit 0 means the app compiles. Fast.
2. `bunx srt render --size 480x640 --duration 1 --fps 2` -
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

## Servers (what is served, and finding it again)

- A dev server serves a project (started in its root: the cwd must hold the
  package.json) or a single file (`srt run <file>` outside a project). Both
  in one place is ambiguous, so `srt run <file>` in a project root needs
  `--project` (the project, with this entry) or `--file` (the file alone).
  Nothing searches upward for a package.json.
- One server per project or file. The server binds the port it had last
  time, else the first free one from 34884 up, and prints it; `--port <N>`
  pins it. Loopback only
  unless `--lan`, which is what phones and other devices (and `srt android`
  on a real device) need; `--tunnel` works without it.
- `-c <N>` / `--client <N>` picks the client data tree (default 0). Storage
  is per app inside a tree, so two projects share client 0; only two clients
  of the same app need distinct slots.
- Dev state lives in `~/.solidrt/`: `servers/<key hash>/` holds each server's
  `live.json` (the registry record, written by the server and removed at
  exit), its remembered `port` and tunnel key; `clients/client<M>/` the
  client trees (srt passes `--data-root ~/.solidrt/clients` to every locally
  spawned client). A record left behind by a crash is pruned when the next
  server starts, and `srt client`, `srt mcp` and `srt android` confirm a
  record against the server (its control API names the key it serves)
  before using it.
- `srt client` and `srt mcp` need no port: run from the project root (or the
  directory of a served file) they resolve the server from the registry;
  `--port` pins one.

## Dev server proxies (when clients on other devices need your machine's data)

- `--proxy-http` - route `fetch` through the dev server; responses cached in
  `.srt-data/http-cache.db` in the project root (delete the file to clear).

