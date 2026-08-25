# The dev server for agents

What the dev server (`srt run` / `srt server`) is and how to talk to it
directly. For driving the app over MCP, the wiring of agent clients and the
debugging lessons, see src/mcp/agents.md.

- A dev server serves a project (started in its root: the cwd must hold the
  package.json) or a single file (`srt run <file>` outside a project). Both
  in one place is ambiguous, so `srt run <file>` in a project root needs
  `--project` (the project, with this entry) or `--file` (the file alone).
  Nothing searches upward for a package.json.
- One server per project or file. The server binds the port it had last
  time, else the first free one from 34884 up, and prints it; `--port <N>`
  pins it. Loopback only unless `--lan`.
- Dev state lives in `~/.solidrt/`: `servers/<key hash>/` holds each server's
  `live.json` (the registry record, written by the server and removed at
  exit), its remembered `port` and tunnel key; `clients/client<N>/` the
  client trees. `srt client` and `srt mcp` resolve the server from the
  registry by the project (or served file) at their cwd; `--port` pins one.
- The server is one flux process, complete on its own:
  `flux dist/server.js [file] [--project|--file] [--port N] [--lan]
  [--proxy-http] [--capture f] [--tunnel] [--stats] [--client N ...] [-- args]`
  (src/server/args.ts). It finds the platform binaries, bun and srt through
  `SRT_PLATFORM_DIR`, `SRT_BUN` and `SRT_CLI`, which `srt` sets; started by
  hand in a checkout it needs only `SRT_HOME`.
- Reload-on-save watches the bundle's inputs (every file the running
  bundle was built from, dependencies included) and the `assets/` tree, not
  a directory: a file the app does not import never triggers a rebuild.
  While the last build failed, the source tree is watched as a whole until
  a build succeeds. `POST /__control__/watch?active=false` pauses it (the
  MCP `pause_watch` tool) while an agent edits; `POST /__control__/reload`
  (the `reload` tool) is the agent's push.

## The control API without MCP

Every MCP tool is a thin wrapper over the dev server's HTTP control API, so
a shell script, a CI step, or an agent with no MCP bridge can drive the app
the same way. Base: `http://127.0.0.1:<port>/__control__/`, where `<port>`
is the one `srt run` printed at startup (also in the server's
`~/.solidrt/servers/*/live.json` record). GET unless noted;
every endpoint answers JSON and an error is `{ "error": "..." }` with a
4xx/5xx status. Endpoints that talk to a client take `?client=<id>` (from
`/clients`); it may be omitted when exactly one client is connected.

- `/clients` - `{ generation, key, mode, entry, projectDir, clients: [{ id,
  ... }] }`. Check `key` (the project root or single file served) is the app
  you mean.
- `/logs?since=<seq>&level=<lvl>&contains=<text>&wait=<ms>` - `{ entries:
  [{ seq, at, client, level, text, repeats? }], latest, generation }`. Pass
  the previous `latest` as `since` to read only new output; a changed
  `generation` means the server restarted and cursors and client ids are
  stale.
- `/tree?query=<text>&root=<id>&depth=<n>&props=true` - `{ limit, matches:
  [{ id, kind, path, x, y, width, height }] }` for a query, the nested tree
  otherwise. Node ids are per client and change on reload; re-query after
  `/reload`.
- `/snapshot?node=<id>` - `{ width, height, pngBase64 }`, display-scaled;
  add `&format=raw` for `rgbaBase64` (RGBA8 bytes, no decoder needed for
  pixel assertions), `&rect=x,y,w,h` to crop, `&scale=<x>`. Snapshot the
  smallest node that shows the change, not the window root.
- `/texture?id=<textureId>` - same shape and options as `/snapshot`, at the
  texture's native size (a scene or shader target behind a `<texture>` leaf).
- `/gpu?label=<text>` - the GPU resource inventory; `label` keeps only the
  resources created with exactly that label (ids change on reload, labels
  do not).
- `/buffer?id=<bufferId>&offset=<n>&length=<n>&as=<f32|u8|...>` - vertex
  buffer contents.
- `/stats?window=<ms>` - the performance statistics.
- `/debug` - the app's registered debug commands; POST
  `/debug?name=<cmd>` with a JSON body as its args to call one.
- POST `/input` with `{ "events": [...] }` - synthetic input through the
  real pipeline, same event shape as the `send_input` tool (tap real
  coordinates read from `/tree`).
- POST `/clock?scale=<x>` (0 pauses) / `?step=<n>` frames while paused.
- POST `/reload` - rebuild and push to every client; `{ ok, clients }` or
  the build error.
- POST `/load` with `{ "entry": "<path>" }` - switch the entry and push it;
  `{ ok, entry, clients }` or the build error. Relative paths resolve
  against the project root (file mode: the served file's directory). A
  project server only loads files inside its project.
- POST `/mute?active=true|false` - mute/unmute the user's own input on
  every client, gamepads included (synthetic `/input` still goes through;
  resize and close cannot be muted). `{ ok, active, clients }`. The mute
  lifts when the server stops; unmute yourself when done.
- POST `/watch?active=true|false` - resume/pause reload-on-save (`active`
  is whether it watches). `{ ok, active }`. Paused, saves push nothing
  until `/reload`; changes made meanwhile are not replayed on resume. The
  pause lifts when the server stops; resume yourself when done.

The loop is the same as over MCP: `/reload`, then `/logs?since=`, then
`/tree` for coordinates and `/snapshot` of the smallest relevant node.
