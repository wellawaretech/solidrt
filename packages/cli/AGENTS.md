# @solidrt/cli - agent notes

Dense, self-contained facts for running and verifying a SolidRT app. The
prose lives in README.md and src/<command>/docs.md (also the website). For
the authoring model (elements, props, reactivity), see @solidrt/core (its
AGENTS.md).

`sol` is the dev tool. Bun is a dev prerequisite only; SolidRT apps run on the
bundled `flux` runtime, not on Bun. Invoke via `bun run sol <command>`.

The dev loop against a running app is pause_watch -> edit -> reload ->
resume_watch -> get_logs -> get_snapshot, with mute_user_input while you
measure or test and unmute_user_input after; agents/debugging.md has the
why of each hold. Several clients may be attached at once: `reload` reaches
all of them, while call_debug / send_input / get_snapshot are per client
(debugging.md). `reload` surfaces build errors but not type errors:
`bun run sol check` is for those.

agents/ carries the depth this one leaves out; read the one that matches
the work before starting it:
- agents/debugging.md - driving a running app over MCP, pointing an agent
  client at the `sol mcp` server, what the dev server serves and its control
  API without MCP (a shell, a CI step), and the debugging lessons that cost
  real time. Read before investigating a bug, verifying a change against the
  live app, or scripting against a server.
- agents/assets.md - the assets/ folder, inlined imports and the bundle
  output; fonts, the `solidrt` package.json identity key, and distribution
  builds. Read before adding an asset or a font, or building to distribute.
- agents/testing.md - writing tests for an app or a package with `sol
  test`: what to test at which layer, naming nodes, the time model, how to
  read a failure. Read before writing or repairing a test.

## Commands

- `bun run sol init <dir>` - scaffold a new SolidRT project into a new (empty)
  folder: package.json, tsconfig.json, AGENTS.md, a starter src/index.tsx, a
  starter tests/app.test.tsx, an empty assets/ (everything in it ships with
  the app), then installs deps. Greenfield shortcut (no install needed first):
  `bun create solidrt <dir>`.
- `bun run sol run` - dev server + a local client window, from the project root
  (entry `solidrt.entry` in package.json, default src/index.tsx); `bun run sol
  run <file>` serves a single file outside a project. NEEDS A DISPLAY (opens a GUI
  window). Not usable headless. Reloads on save (the bundle's inputs and
  `assets/`); an agent pauses that with `pause_watch` and pushes its edits
  with the MCP `reload` tool.
- `bun run sol tool` - list the build-time tools the installed `@solidrt/*`
  packages ship (`<package>/tools/<name>.ts`, named `<package>/<name>`);
  `bun run sol tool <package>/<name> [arguments]` runs one in the project,
  everything after the name passed through as the tool's own arguments
  (each tool prints its own usage on `--help`). A tool runs under bun; one
  whose file is `<name>.flux.ts` runs under the flux runtime instead, for
  what only the runtime has, and is named and called the same way. What a tool does
  is the package's business (e.g. `3d/model` bakes a glTF into a model
  file); sol only finds and runs them.
- `bun run sol check [file|dir]` - build in memory and typecheck, no output
  and no reload. With a folder it covers every entry under it (src/index.tsx
  and examples/*), so `bun run sol check .` answers "did I break any example"
  in one call. The dev server runs it once at startup without gating on it.
- `bun run sol test [file|dir]` - run the tests: every `tests/*.test.ts` under
  the folder (default: the cwd), each file bundled and run in its own `flux`
  process, not under Bun. A test file imports `test` and `expect` from
  `flux:test` (flat tests, no `describe`, no hooks; the matchers are in
  flux-types `modules/test.d.ts`). Every test runs in an engine of its
  own: the file is evaluated again for each test, so module state, timers
  and listeners of one test never reach the next, and an uncaught error
  fails the test it happened in. A test runs on real time (timers,
  `performance.now()`, `Date.now()`); `settle()` from `flux:test` waits for
  work the test set off without holding its promise. `Math.random()` is seeded
  in every test's engine, so random inputs are the same on every run;
  `--seed <n>` picks another sequence. `--filter <text>` runs the tests
  whose name contains the text; `--only flux|app` runs one layer;
  everything after `--` reaches the test files as `flux:process` argv.
  Exits nonzero on any failure; a failure prints expected, received and
  the source line, what was in flight, and for an app test the app time
  and frame, what still demanded frames, the outline and the path of a
  snapshot under `dist/test/` (agents/testing.md, "Reading a failure").
  A file that imports `@solidrt/test` (or any app
  runtime module; always a `.test.tsx`) is an app test and runs on the dev
  client, headless: `test(name, async app => ...)`, time passes only by
  `app.frame(n)` / `app.advance(ms)` / `app.settle()` (until the app is at
  rest), timers fire with the frame their time
  falls in, `performance.now()` is 0 and the date starts at 2000-01-01 UTC
  and moves with the frames; `app.mount(ui)`, `find({ text | label |
  kind })`, `app.tap` / `drag` / `key` / `type` / `input` (the `send_input`
  event shape, through the real pipeline), `app.link`, `app.debug`, and
  the readers (`outline()`, `pixel()`, `app.gpu()`) are described in core's
  AGENTS.md ("Testing an app").
- `bun run sol bundle` - bundle the project into `dist/bundle/` (or
  `--output <dir>`): `<name>.sol.js` plus the app's isolate modules as
  `isolates/<id>.js`. With `--compile`, bytecode (`.fluxbc`)
  instead. Move the dir, not the bare file - a bundle loaded without its
  isolates/ dir loses them (`--stdout` cannot carry them at all).
  `--minify`, `--dev` also available.
- `bun run sol render [flags]` - render the project OFFSCREEN to PNG frames,
  optionally replaying a `--script` file recorded via `--capture`;
  `--settle` runs the app to rest first, `--strict` fails on a logged
  error, `--seed <n>` another Math.random sequence (see below).
- `bun run sol server [file]` / `bun run sol client` - the two halves of `run`
  separately (server distributes code; clients on other devices connect to it).
- `bun run sol run --capture out.script.json` - records keydown/keyup
  from every connected client into one script file (written on client
  disconnect), for replaying later with `render --script`. The file is JSON
  Lines and hand-authorable: one object per line,
  `{"after": <ms since previous event>, "type": "keydown" | "keyup",
  "key": <W3C KeyboardEvent.key>, "device": <client id>}`. For probing app
  state without a display, `-- <args...>` reaches the app as `flux:process`
  argv, which is often simpler than scripting input.

## Verifying without a display (headless / CI / agent box)

Two reliable checks that need no GUI:

1. `bun run sol bundle` - exit 0 means the app compiles. Fast.
2. `bun run sol render --settle --strict --size 480x640 --duration 1 --fps 2`
   - renders offscreen via EGL and writes `frame-NNNNNN.png`. `--strict`
   makes exit 0 mean the frames were written AND the app logged no error
   (a scene whose build throws is contained per the error model and would
   otherwise write an empty frame and exit 0); `--settle` makes frame 0
   the app at rest (loads landed, mount transitions played out) instead of
   its loading state. An app that calls `exit()` ends the run early, with
   0. Combine with `--fps`/`--duration` (defaults 1280x720, 60fps, 1s). No
   display needed: rendering uses SDL's offscreen driver, or alloy's own
   EGL pbuffer where that driver cannot go headless (see the ANGLE gotcha
   below). A test (`bun run sol test`, agents/testing.md) asserts on the tree
   instead of a picture and is the better gate for behavior.

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
- Every frame follows a frame callback: frame k is the app's state after its
  (k+1)th `onFrame` call, at time (k+1)/fps; the mount state (before any
  callback) is drawn but never written. `onFrame`'s `rate` is the capture
  fps. A mount-time `windowSize()` read is 0x0, as on a live client (the
  first resize lands after the module has evaluated); read it reactively.
- `--duration` is APP time, not wall clock: the render steps the frame
  clock and waits for nothing, so 40 frames at `--fps 1` can render in two
  real seconds. An app that fetches, reads files or builds in an isolate is
  LOADING in every frame, however long a duration is asked for, unless
  `--settle` is given: then the app is run to rest first and the frames
  start from there (app time passes while it settles). An app that
  animates forever cannot settle and fails with the flag, named.
- There is no wall clock in a render: `performance.now()` reads 0, the
  date starts at 2000-01-01 UTC and moves with the frames, `Math.random()`
  is seeded (`--seed <n>` for another sequence). Two renders of one app
  write identical frames.
- `--fps` is a positive INTEGER (`--fps 0.5` is rejected); slow a scene down
  with fewer frames over a longer `--duration`, or with the app's own clock.
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
  package.json) or a single file (`sol run <file>` outside a project). Both
  in one place is ambiguous, so `sol run <file>` in a project root needs
  `--project` (the project, with this entry) or `--file` (the file alone).
  Nothing searches upward for a package.json.
- One server per project or file. The server binds the port it had last
  time, else the first free one from 34884 up, and prints it; `--port <N>`
  pins it. Loopback only
  unless `--lan`, which is what phones and other devices (and `sol android`
  on a real device) need; `--tunnel` works without it.
- `-c <N>` / `--client <N>` picks the client data tree (default 0). Storage
  is per app inside a tree, so two projects share client 0; only two clients
  of the same app need distinct slots.
- Dev state lives in `~/.solidrt/`: `servers/<key hash>/` holds each server's
  `live.json` (the registry record, written by the server and removed at
  exit), its remembered `port` and tunnel key; `clients/client<M>/` the
  client trees (sol passes `--data-root ~/.solidrt/clients` to every locally
  spawned client). A record left behind by a crash is pruned when the next
  server starts, and `sol client`, `sol mcp` and `sol android` confirm a
  record against the server (its control API names the key it serves)
  before using it.
- `sol client` and `sol mcp` need no port: run from the project root (or the
  directory of a served file) they resolve the server from the registry;
  `--port` pins one.

## Dev server proxies (when clients on other devices need your machine's data)

- `--proxy-http` - route `fetch` through the dev server; responses cached in
  `.solidrt-data/http-cache.db` in the project root (delete the file to clear).

