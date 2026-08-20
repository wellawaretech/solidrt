# Debugging a running app

Read this before investigating a bug in a running app, or before driving the
app over MCP to verify a change.

## Driving the app over MCP

The project ships an MCP server (`.mcp.json`, `srt mcp`) that talks to the dev
server `bunx srt run` starts. Each tool documents itself in full - read the
tool description rather than guessing at its arguments. What the individual
descriptions cannot tell you:

- If `list_clients` is empty, no app is running: ask the user to start
  `bunx srt run src/index.tsx` rather than starting a second one yourself.
  The bridge dials the dev server's default port (34884), so if the user
  started it with `--port N`, `.mcp.json` needs the same flag:
  `"args": [..., "mcp", "--port", "N"]`.
- Several clients may be attached at once (desktop window, phone, tablet)
  with different sizes, display scales and safe areas. `reload` pushes to all
  of them, but `call_debug` / `send_input` / `get_snapshot` / log cursors are
  per client, and interactive state does NOT sync - a flow driven on one
  client leaves the others sitting on the initial screen, which reads as a
  crash to a human holding that device. So: when driving state via
  `call_debug`, send the same call to every client (or say which client you
  are using); and before calling a visual change done, snapshot each distinct
  form factor at least once - a layout that fits one window can clip or
  overflow another.
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
