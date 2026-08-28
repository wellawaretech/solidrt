# Debugging a running app

Read this before investigating a bug in a running app, or before driving the
app to verify a change: over MCP, or through the same control API from a
shell or a CI step ("The control API without MCP" below).

## Driving the app over MCP

The project ships an MCP server (`srt mcp`) that talks to the dev server
`bunx srt run` starts. If your client lists no `solidrt` tools, it has not
been pointed at the server yet - see "Wiring up an agent client" below. Each
tool documents itself in full - read the tool description rather than guessing
at its arguments. What the individual descriptions cannot tell you:

- If `list_clients` is empty, no app is running: ask the user to start
  `bunx srt run` rather than starting a second one yourself.
  The bridge needs no port: it resolves the server currently serving this
  project, whatever `--port` it was started with, and re-resolves when
  that server goes away or a different project's server takes its port.
  Passing the flag to `srt mcp` pins the bridge to that one server instead.
- Several clients may be attached at once (desktop window, phone, tablet)
  with different sizes, display scales and safe areas. `reload` pushes to all
  of them, but `call_debug` / `send_input` / `get_snapshot` are per client
  (`get_logs` takes a `client` filter), and interactive state does NOT sync
  - a flow driven on one client leaves the others sitting on the initial
  screen, which reads as a crash to a human holding that device. So: when
  driving state via `call_debug`, send the same call to every client (or say
  which client you are using); and before calling a visual change done,
  snapshot each distinct form factor at least once - a layout that fits one
  window can clip or overflow another.
- Measuring or testing? `mute_user_input` first: it mutes the user's own
  input on every client (a stray click or keypress mid-measurement corrupts
  the result); `send_input` still works. `unmute_user_input` the moment you
  are done, or whenever you need the human to press something: they see an
  unresponsive client meanwhile. The bridge unmutes when it exits, the
  server when it stops, but neither is a reason to leave a mute on.
- Editing? `pause_watch` first: the server reloads on save, and a
  half-finished save would land on the user's screens as a build error or
  a broken app. Edit, `reload`, then `resume_watch`. The two holds are
  separate on purpose: the mute keeps the human out while you measure, the
  pause keeps your own saves out while you edit, and a human editing next
  to you keeps auto-reload unless you hold it. The bridge resumes when it
  exits, but do not rely on that.
- A `shader` on `<window>` runs on the finished frame past the point every
  capture reads: `get_snapshot` returns the UNSHADED content (window node
  included), `get_texture` has no id for the shaded layer, and
  `get_gpu_resources` reports only that the pass exists. `bunx srt render` is
  the only programmatic view of what a window shader produces.
- Permission prompts: agents typically ask approval per MCP tool. All of
  these tools only talk to the local dev server the user started with
  `bunx srt run` - nothing leaves the machine - so approving the server as a
  whole is a reasonable default. If repeated prompts get in the way, do not
  work around them; tell the user they can pre-approve the server in their
  agent's settings (most agents have a per-server trust or allowlist setting
  - in Claude Code, add "mcp__solidrt" to `permissions.allow` in
  ~/.claude/settings.json to cover every solidrt project). This is the user's
  call to make, once, in their own tooling.

## Wiring up an agent client

`.mcp.json` in the project root is Claude Code's convention, and a scaffolded
app ships one. MCP standardizes the protocol, not how a client discovers
servers, so every other client reads its own file. The entry is the same
everywhere, in the client's syntax:

```json
{ "command": "bun", "args": ["node_modules/@solidrt/cli/bin/srt", "mcp"] }
```

Where it goes (these locations move between client releases; check the
client's own docs if it does not match):

- Claude Code: `.mcp.json` in the project root, under `mcpServers`. Claude
  Code asks once, per project, before it will launch a server from this
  file; until that is approved the `solidrt` tools are absent.
- Cursor: `.cursor/mcp.json`, under `mcpServers`
- VS Code / Copilot: `.vscode/mcp.json`, under `servers`
- Gemini CLI: `.gemini/settings.json`, under `mcpServers`
- Codex CLI: `~/.codex/config.toml`, under `[mcp_servers.solidrt]`

Several of these also ship a command that writes the entry for you
(`claude mcp add`, `codex mcp add`); prefer it over hand-editing.

The one constraint: the bridge must run with the project as its working
directory. It finds the dev server through the nearest package.json above
its cwd, and the `args` path is relative to the project root. A per-project
file gives that for free. A user-level entry works too, but only while the
client itself was launched from the project root (or the client supports a
`cwd` field and it is set); launched from elsewhere, every tool call answers
"No dev server for ..." while one is running.

## The dev server

What the dev server (`srt run` / `srt server`) is and what it serves.

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
  A record whose process died is pruned when the next server starts, and
  resolution confirms a record against the server before using it.
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
4xx/5xx status, and every response carries `x-solidrt-project` (the key
served) and `x-solidrt-generation` (the server run) headers, so a caller can
confirm it reached the server it meant and notice a restart. Endpoints that
talk to a client take `?client=<id>` (from `/clients`); it may be omitted
when exactly one client is connected.

- `/clients` - `{ generation, key, mode, entry, projectDir, userInputMuted,
  watchPaused, clients: [{ id, ... }] }`. Check `key` (the project root or
  single file served) is the app you mean.
- `/logs?since=<seq>&level=<lvl>&contains=<text>&wait=<ms>&client=<id>` -
  `{ entries: [{ seq, at, client, level, text, repeats? }], latest,
  generation }`. Pass the previous `latest` as `since` to read only new
  output; `client` keeps one client's entries (all clients without); a
  changed `generation` means the server restarted and cursors and client
  ids are stale.
- `/tree?query=<text>&root=<id>&depth=<n>&props=true` - `{ limit, matches:
  [{ id, kind, path, x, y, width, height }] }` for a query, the nested tree
  otherwise. Node ids are per client and change on reload; re-query after
  `/reload`.
- `/snapshot?node=<id>` - `{ width, height, pngBase64 }`, display-scaled;
  add `&format=raw` for `rgbaBase64` (RGBA8 bytes, no decoder needed for
  pixel assertions), `&x=&y=&width=&height=` (all four) to crop,
  `&scale=<1-8>`. Snapshot the smallest node that shows the change, not the
  window root.
- `/texture?id=<textureId>` - same shape and options as `/snapshot`, at the
  texture's native size (a scene or shader target behind a `<texture>` leaf).
- `/gpu?label=<text>` - the GPU resource inventory; `label` keeps only the
  resources created with exactly that label (ids change on reload, labels
  do not).
- `/buffer?id=<bufferId>&offset=<n>&length=<n>&as=<f32|u16|u8>` - vertex
  buffer contents (default f32; reads cap at 64 KiB).
- `/stats?window=<ms>` - the performance statistics. POST
  `/stats?active=true|false` switches the on-screen stats overlay instead
  (the `set_stats_overlay` tool): one client with `&client=<id>`, every
  client (and the setting new clients join with) without; `/clients`
  reports each client's `stats`. `frames` counts the frames that changed
  the picture: tree rebuilds, plus GPU content changes presented without
  one (a layer write, a shader param, an upload - a sprite or shader app
  rebuilds nothing, every frame of it is one of these).
  `fps` is the refresh rate presented at: when motion looks wrong and `fps`
  looks fine, `frames` is the number to read - a picture that only changes
  26 times a second shows 26 there, and the stutter is the app's update
  cadence, not the engine's. `frames: 0` means the picture did not change
  at all in the window.
- `/debug` - the app's registered debug commands; POST
  `/debug?name=<cmd>` with a JSON body as its args to call one.
- POST `/input` with `{ "events": [...] }` - synthetic input through the
  real pipeline, same event shape as the `send_input` tool (tap real
  coordinates read from `/tree`).
- POST `/clock?scale=<x>` (0 pauses) / `?step=<n>` frames while paused;
  `{ scale, pendingSteps }` back. `/clients` reports each client's `timeScale`,
  reset to 1 by every push.
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

## Lessons that cost real time

- console.log + get_logs is your primary probe into runtime state. For state
  you will want repeatedly (a pose, a mode, a counter), bind a debug key that
  logs it and read it back via get_logs.
- Better than debug keys when driving the app over MCP: register debug
  COMMANDS - `registerDebug(name, fn)` from `srt:dev`, invoked via the
  list_debug/call_debug tools. Use them to SET UP state (jump to a level,
  force a mode, seed a scenario); then the runtime-level tools take over -
  set_time_scale 0 freezes the result for as many snapshots as you need,
  and step_frames walks it forward deterministically. Set state, pause,
  snapshot. Registrations reset on hot reload, so register at module init;
  sync return values only - and note a signal you just wrote flushes on a
  microtask, so returning a signal read straight after setting it returns
  the OLD value.
- call_debug sets state directly, skipping focus, key routing, and
  TextInput - fine for SETUP, but "the interaction works" is only shown by
  the real pipeline: verify clicks, typing, and drags with send_input,
  which enters events where SDL input does.
- Key events start at the focused node and bubble to the window root; with
  nothing focused they go to the window root alone. So a debug key bound via
  `<window onKeyDown>` always fires (unless a focused component consumes the
  key with stopPropagation, as TextInput does for editing keys). `key` and
  `code` are W3C KeyboardEvent values, so arrow keys arrive as "ArrowLeft"/
  "ArrowRight"/"ArrowUp"/"ArrowDown" (not "Left"), alongside "Enter",
  "Escape", "a".
- Idle frames skip work: shaders/pipelines only re-render when an input
  changes - their own params/geometry, or a sampled texture (a data upload,
  or a sampled target re-rendering; chains propagate automatically). Measure
  performance while inputs are actually changing. get_snapshot works on an
  idle client (it requests its own frame); a timeout means the JS thread is
  busy or wedged. get_texture on a pipeline's render target reads the
  current output, pending writes included, without needing a new frame.
- When a human reports a visual bug: capture a snapshot and SAY WHAT YOU SEE
  in it before investigating, so you agree on the symptom. If you cannot see
  the problem in the capture, say that instead of guessing.
- Snapshots are downscaled by the time you see them, so a full-window capture
  cannot show you a defect a few pixels across. Whenever you hand-author
  geometry - a `d-path` from raw path math, a `radius` where two shapes meet,
  a stroke join - inspect it MAGNIFIED once, when you write it: get_snapshot
  with a tight crop at scale 4-8 shows the actual rendered pixels enlarged,
  in one call, on the real app. Verifying that a shape is in the right place
  is not the same check as verifying it is drawn right - and get_render_tree
  props answers the third question, whether the value you set is the value
  the renderer holds.
- GPU/geometry bugs: inspect the actual GPU data FIRST - get_gpu_resources
  for draw counts/uniforms/sizes, get_texture for atlas or data-texture
  contents ("is this tile blank?" is a ten-second question), get_buffer for
  vertex data. The pixels only tell you THAT something is wrong; the
  resources tell you WHERE the data stops being right. In a one-big-pipeline
  app the render tree is a single <texture> leaf and tells you nothing -
  these tools are the visibility layer behind it. Only when the GPU data is
  all correct (so the bug is in producing it, or in the shader), reproduce
  the math CPU-side in a scratch bun script against the app's real data and
  print values.
- Validate assets at load time and log anomalies (missing lumps/files,
  fully-transparent composites, zero-sized images). Silent fallbacks hide
  bugs for days; a one-line warning surfaces them the first run.
- After every reload the app restarts from its initial state. If reaching
  the bug site takes navigation, add a dev shortcut (teleport key, noclip,
  initial-state override) before iterating - the round trips add up fast.
- Clamp onFrame time deltas to [0, cap], not just capped: across a hot
  reload the runtime's tick counter resets AFTER the new instance's first
  frame, so the second frame computes a hugely NEGATIVE delta.
  Math.min(dt, cap) lets it through, and one bad frame can corrupt anything
  integrated from dt (positions fly off, accumulators go so negative they
  never recover). Math.max(0, Math.min(dt, cap)) costs nothing.
- A fixed-timestep simulation on that clamped dt still drifts: no panel
  presents at exactly its nominal rate (a "60 Hz" panel measured 60.3) and
  the paced tick tracks the real cadence, so a 16.667 ms step against a
  16.59 ms average dt comes up one step short every few seconds - one frame
  runs no step (freeze), the next runs two (jump), and frame jitter
  scatters which frame it lands on, so it reads as random stutter. The
  runtime hands every callback the refresh rate,
  `onFrame((tick, frame, rate) => ...)` (SDL's nominal Hz): when the step is
  within a few percent of `1000 / rate`, run whole steps per frame
  (`Math.round(dt / STEP_MS)`, clamped to [0, cap]) so the world rides the
  refresh; only accumulate (`acc += dt; while (acc >= STEP_MS) ...`) when
  the display is genuinely off-rate (50, 120, 144 Hz), or interpolate the
  render by `acc / STEP_MS` if the game does not snap to whole pixels. It
  is invisible in a five-second look and survives every renderer
  optimisation; measure it with a steps-per-frame histogram, not by eye.
- A registered onFrame is a standing request, not demand-gated: it re-requests
  the next frame every time it runs, so the runtime keeps calling it - and
  presents - every frame at the refresh rate until you deregister it (fps
  stays at the refresh rate on an idle screen; that is the 60fps burn called
  out in @solidrt/core agents/performance.md, rule 4). The upside is that a
  self-running loop - a game clock, a shader driver, a stepped VM doing silent
  CPU work with no console output - keeps advancing on its own: it does NOT
  stall when the body changes nothing and needs no startup "prime" write.
  Deregister onFrame (return its cleanup, or let onCleanup fire) whenever
  there is nothing left to advance.
- Layout is incremental: a change re-solves only the dirty path, and clean
  subtrees answer from a per-node cache, so long lists no longer cap layout
  (a thousand-node tree relays out in well under a millisecond). If layoutMs
  still grows with tree size, read the get_stats counters - a low
  cacheHits/cacheGets ratio means the layout cache is being defeated, high
  paraShapes means text is actually reshaping. Paint is viewport-culled:
  under an `overflow="hidden"` scroller only the subtrees that can reach the
  visible box are painted, so paintMs tracks what is on screen, not what is
  mounted (nodesPainted in get_stats shows the count). Very long lists still
  pay for the initial mount and for memory, so windowing stays sensible at
  the thousands-of-rows scale.
- Remote images: createImage (and Image) dedupes repeated URLs, caches the
  bytes on disk, and the runtime rate-limits concurrent asset fetches per
  host - do not build your own promise cache around it. Images are fetched
  with no freshness check (an already-cached URL is never re-checked), so
  use versioned URLs for content that changes. Use Image's `fallback` prop
  (an image source) for the broken-image case instead of catching errors
  yourself.
- fetch() never caches by default and ignores server cache headers. Caching
  is explicit and per call: `fetch(url, { cache: "force-cache" })` for
  assets (serve from disk or fetch-and-store, no freshness),
  `{ cache: "reload" }` to refresh an entry. Image/createImage already do
  this for you.
