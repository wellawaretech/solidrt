# SolidRT app - agent notes

This project uses SolidRT: a custom SolidJS renderer that paints through a Rust
runtime. No DOM, no HTML, no CSS cascade. If you are an AI assistant, read this
before writing or editing code here.

## Levels: core, and frameworks on top

- @solidrt/core is the low-level foundation: host intrinsics (`<window>`,
  `<view>`, `<text>`, the detached `d-*` drawing primitives) with flat props
  that feed the layout and paint engine directly. An app can be written
  entirely at this level.
- Higher-level component frameworks build on core. @solidrt/components is
  the first-party one: themed widgets (Window, View, Text, Button,
  ScrollView, SafeArea, ...) with the `layout={{...}}`/`style={{...}}` prop
  split. It is not privileged - a framework is just functions returning core
  JSX, and an app can use a third-party one or grow its own.

Match the level the code you are editing already uses. package.json shows
the choice this app made: if no component framework is among the
dependencies, the app is core-only - do not add one for a change core
covers.

Authoritative references ship inside the installed packages - read them:
- node_modules/solid-js/CHEATSHEET.md          - SolidJS 2.0 reactivity/control-flow model
- node_modules/@solidrt/components/AGENTS.md   - the component vocabulary; build UI from these
- node_modules/@solidrt/components/README.md   - full prop tables for every component
- node_modules/@solidrt/components/examples/   - single-concept usage patterns to copy (see its README.md index)
- node_modules/@solidrt/core/AGENTS.md         - the underlying element/prop/reactivity model
- node_modules/@solidrt/core/examples/         - single-concept usage patterns to copy (see its README.md index)
- node_modules/@solidrt/cli/AGENTS.md          - running, bundling, headless verify
- node_modules/@solidrt/core/src/types.d.ts and jsx-runtime.d.ts - source of truth

<!-- Claude Code auto-imports these; other tools read the paths above. -->
@./node_modules/solid-js/CHEATSHEET.md
@./node_modules/@solidrt/components/AGENTS.md
@./node_modules/@solidrt/core/AGENTS.md
@./node_modules/@solidrt/cli/AGENTS.md

## The things assistants get wrong (this is not React/DOM)

1. This is SolidJS 2.0 (see CHEATSHEET.md for the reactivity/control-flow
   model), rendering through a custom Rust runtime instead of the DOM. Build
   UI at the level the app uses (see "Levels" above): core intrinsics
   directly, or a component framework such as @solidrt/components (Window,
   View, Text, Image, TextInput, ScrollView, Pressable, Button, SafeArea,
   theme/setTheme) - the first-party, batteries-included one.
2. Most components split their props into two objects: `layout={{...}}` for
   anything that feeds the layout engine (flex/grid, sizing, padding/margin,
   position; font fields for Text) and `style={{...}}` for paint-only
   properties that never relayout (`backgroundColor`, `borderColor`,
   `borderWidth`, `borderRadius`, `color`, and the transform `x`/`y`/
   `rotate`/`scale`). Event handlers (`onPointerDown`, `onKeyDown`, ...) are
   top-level props, not inside `layout`/`style`.
3. `render(() => <App/>)` once, top level. The root MUST be a `<Window>`
   (from @solidrt/components) or the core `<window>` - it throws otherwise.
4. `Window`/`View` do not paint on their own; they only paint when you set
   `style.backgroundColor`/`borderColor` etc - there is no separate
   background element to place by hand.
5. There is no onClick/onPress on host elements. Use `Pressable`/`Button`
   from components (`onPress`), or `onPointerDown` on a `View` for anything
   custom.
6. Reactive window state: prefer the accessors windowSize(), safeArea(),
   displayScale(), windowFocused(), keyboardHeight() (re-exported from
   @solidrt/core) over onResize/onLayout callbacks for reading layout and
   window state. SafeArea (the component) is usually the simpler fix for
   avoiding notches/system UI.
7. Per-frame animation: onFrame((tick, frame) => {}) is the native hook
   (runtime-paced, auto-cleans), re-exported from @solidrt/core.
   requestAnimationFrame(t => {}) exists as a web-standard one-shot but is
   not the preferred animation driver.
8. In a components-based app, reach for @solidrt/core directly only for
   what components doesn't wrap:
   raw host intrinsics and the `d-` (detached, non-layout) primitives like
   `d-rect`/`d-path`/`d-oval` for vector art or perf-sensitive positioned
   drawing, device/GPU subpath imports (@solidrt/core/camera, /microphone,
   /gpu), gradients (createLinearGradient/createRadialGradient), and
   createImage/decodeImage for images below the `Image` component's level.
   Components and core primitives compose freely in the same tree - a
   components-based app can drop to a `<d-path>` for one custom shape without
   giving up `View`/`Text` everywhere else.
9. tsconfig needs jsx:"preserve" + jsxImportSource:"@solidrt/core" (still
   true even when you build almost entirely with @solidrt/components -
   components are plain functions returning core JSX). Solid peer deps are
   pinned betas - do not bump them casually.
10. Use ASCII characters whenever possible in code and text - for example, no
    em-dashes (use a hyphen), no smart/curly quotes, no unicode symbols.
11. Prefer let over const. Use const only for real constants - a single fixed
    string or number value - and name those in ALL_CAPS.
12. Reading a signal/prop/store at the top level of a component body (not
    inside JSX, a `createMemo`, or an effect's compute phase) reads it
    untracked - it silently freezes at the initial value instead of updating
    on change. `createEffect` takes two arguments now: `(compute, apply)`.
    `compute` is the tracked read phase; `apply(value, prev)` runs untracked
    and is where side effects/DOM-equivalent writes belong. The old
    single-arg `createEffect(fn)` form is gone - using it is an error.
13. A scroll container (ScrollView, or anything on createScroll) needs an
    explicit main-axis size - a height, or flex inside a sized parent. With
    neither it resolves to 0 and its content silently vanishes; maxHeight
    alone does not size it (the auto size it would clamp is already 0). The
    runtime warns when this happens.
14. Text `lineHeight` is a MULTIPLIER of fontSize (the theme uses 1.3-1.6),
    not pixels. A CSS-reflex value like 22 makes each line box 22x the font
    size: the text becomes blank space and the parent balloons.
15. Signal writes flush on a microtask: a handler that sets a signal and
    immediately reads it back gets the OLD value. Read the new value in an
    effect, or call `flush()` (from @solidjs/signals) to force it through.
16. Portals cannot mount during the app's initial render: a Modal (or any
    createPortal content) that is visible at first mount throws "no mount
    target". Gate it behind a signal that starts false and open it after
    startup - overlay content is opened, not born open.
17. An element-valued prop (children, a content/icon slot) compiles to a
    getter that builds a fresh native subtree on EVERY read, and a subtree
    that is never inserted is never freed - native nodes are not garbage
    collected, so what is only wasted work in DOM Solid is a permanent
    memory leak here. Read such props exactly once, at the place they are
    mounted. To inspect children (a typeof probe, counting), resolve them
    first with the children() helper (re-exported from @solidrt/core) and
    probe the resolved memo - never `typeof props.children` on the raw prop.

## Performance model (JS is the slow lane)

The JS engine is interpreted and every property write crosses an FFI boundary
into the runtime, so per-frame JS work is the expensive path while GPU work is
nearly free. Rules, in order of leverage:

1. Continuous effects (snow, particles, animated backgrounds) belong in a
   fragment shader: createShader (from @solidrt/core/gpu) + `<texture
   params={{ iTime }}>`. The whole effect then costs one setProperty per
   frame - the iTime write - regardless of visual complexity. Shader output
   must be premultiplied alpha (white flakes are `vec4(vec3(a), a)`);
   straight alpha (`vec4(1,1,1,a)`) composites as opaque white.
2. Reduce setProperty calls wherever possible: one path string rebuilt per
   frame beats N elements with N animated positions; a shader beats the path
   string. get_stats' setPropsPerFrame is the counter to watch.
3. Never leave onFrame registered while nothing animates: a pending onFrame
   is a standing frame request, so the runtime renders and presents every
   vsync even when the callback body does nothing - an invisible 60fps GPU
   burn that also drags the OS compositor along with it. For an on-demand
   animation pump (tweens), use a self-rechaining one-shot
   requestAnimationFrame that stops re-requesting when its work list
   empties. (Registering onFrame outside a component body also warns
   NO_OWNER_CLEANUP - it assumes a reactive owner.)
4. repaintBoundary works like Flutter's: transforms and opacity on the
   boundary node itself (or any ancestor) are hoisted out of the cache and
   applied at composite time, so animating x/y/scale/rotate/opacity of a
   boundary does NOT re-raster it (verified by A/B measurement - the damage
   system classifies these as Transform and keeps the node's own cache).
   What DOES invalidate the cache is any paint or content change inside the
   subtree - colors, path data, text, a Show toggling - so drive animation
   with transforms and keep the cached content itself static.
5. "snapshot" boundaries pay first-frame texture allocation + raster:
   creating many at once (dealing a board of 64 sprites) is a visible
   one-frame hiccup - pool or pre-warm if that moment matters.

## Run / verify

- bunx srt run src/index.tsx     - dev server + window (needs a display)
- bunx srt check src/index.tsx   - exit 0 means it compiles and the app's
  types hold (dependency-internal type errors are hidden). Builds in memory:
  writes nothing and never triggers a dev-server reload, so use this while
  iterating - `srt bundle` writes output files and reloads connected clients
- bunx srt render src/index.tsx --size 480x640 --duration 1 --fps 2 - headless
  render to PNG frames (proves it renders; see the cli AGENTS.md for where the
  frames land)

## MCP: inspect the running app

The project ships an MCP server (.mcp.json, `srt mcp`) that talks to the dev
server `bunx srt run` starts. When it is loaded in your environment, prefer
its tools over guessing at runtime state:

- list_clients: connected app clients, their platform and runtime capabilities
- get_logs: console output and runtime errors (seq cursor; `wait_ms` long-poll
  to catch output right after a reload; `level`/`contains` filters; repeated
  lines collapse into one entry with a `repeats` count)
- get_render_tree: what the app actually rendered - node kinds, text, and
  window-relative boxes. Whole trees get large: `query` finds nodes by
  kind/text, then `root` + `depth` inspect just that region
- client ids and log cursors die with the dev server: list_clients and
  get_logs responses carry `generation`, and a changed generation means
  re-fetch ids and restart cursors
- get_stats: fps, CPU/memory, frame phase timings, setProperty rate, plus
  layout-activity counters for the last rebuild (nodes, measureCalls,
  paraShapes, dirtiedNodes, cacheGets/cacheHits) - when layoutMs looks
  wrong, these say whether the cost is text shaping, invalidation breadth,
  or a defeated layout cache (healthy incremental rebuilds show a near-100%
  cacheHits rate)
- get_snapshot: PNG capture of any render-tree node's pixels (get node ids
  from get_render_tree; the window node captures everything). Pass `save_to`
  on get_snapshot or get_texture to also write the PNG to a file - the image
  in the tool result cannot be saved afterwards, so decide before capturing
  (e.g. keep a before/after pair to diff)
- get_gpu_resources: inventory of GPU state - textures (size, render target
  or not), vertex buffers (byteLength), pipelines (draw count, attribute
  layout, bound textures, last-applied uniform values)
- get_texture: any GPU texture read back as a PNG by id - atlases, data
  textures, and shader/pipeline render targets alike (a render target is
  "what this pipeline last drew", no frame or snapshot needed); crop with
  x/y/width/height
- get_buffer: a vertex-buffer range decoded to numbers (f32/u16/u8, 64 KiB
  per call) - verify geometry after a writeBuffer instead of inferring it
  from pixels
- reload: rebuild from source and push to every client - THE dev loop is
  edit -> reload -> get_logs -> get_snapshot. reload surfaces build errors
  but not type errors; run `bunx srt check` for those.
- load: bundle a given source file and push it to every client, replacing
  the running app; later reloads rebuild that entry. Use it when the dev
  server has no app loaded yet, or to switch apps without restarting srt.
- watch: pause (enabled: false) or resume the automatic reload-on-save.
  Pause BEFORE creating or editing source files so half-finished work is
  not pushed to the user's screens mid-burst; a successful reload or load
  resumes it, so pause again before the next burst. Never leave it paused
  when you stop working - the user's own saves rely on it.

The tools need a running app: if list_clients is empty, ask the user to start
`bunx srt run src/index.tsx`.

- Permission prompts: agents typically ask approval per MCP tool. All of
  these tools only talk to the local dev server the user started with
  `bunx srt run` - nothing leaves the machine - so approving the server as
  a whole is a reasonable default. If repeated prompts get in the way, do
  not work around them; tell the user they can pre-approve the server in
  their agent's settings (most agents have a per-server trust or allowlist
  setting - in Claude Code, add "mcp__solidrt" to `permissions.allow` in
  ~/.claude/settings.json to cover every solidrt project). This is the
  user's call to make, once, in their own tooling.
- Multiple clients: several clients may be attached (desktop window,
  phone, tablet) with different sizes, display scales, and safe areas.
  reload pushes to all of them, but call_debug / get_snapshot / log
  cursors are per client, and interactive state does NOT sync - a flow
  driven on one client leaves the others sitting on the initial screen,
  which reads as a crash to a human holding that device. So: when driving
  state via call_debug, send the same call to every client (or say which
  client you are using); and before calling a visual change done,
  snapshot each distinct form factor at least once - a layout that fits
  one window can clip or overflow another.

## Debugging a running app (lessons that cost real time)

- console.log + get_logs is your primary probe into runtime state. For state
  you will want repeatedly (a pose, a mode, a counter), bind a debug key that
  logs it and read it back via get_logs.
- Key events are delivered ONLY to the focused node (no bubbling): call
  setFocus(node.id) from the window's ref or onKeyDown never fires. This
  runtime names arrow keys "Left"/"Right"/"Up"/"Down", not "ArrowLeft".
- Idle frames skip work: shaders/pipelines only re-render when their params
  change, so measure performance while uniforms are actually changing.
  get_snapshot works on an idle client (it requests its own frame); a
  timeout means the JS thread is busy or wedged. get_texture on a pipeline's
  render target reads the last-drawn frame without needing a new one.
- When a human reports a visual bug: capture a snapshot and SAY WHAT YOU SEE
  in it before investigating, so you agree on the symptom. If you cannot see
  the problem in the capture, say that instead of guessing.
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
- Frames are demand-gated: JS frame callbacks only run when the previous
  frame changed something (input, signal write, GPU upload). An app whose
  onFrame returns early without side effects on its first frame never gets
  a second one - self-running animation (game clocks, shader-driven
  effects) must make one state change at startup to prime the loop; after
  that its own writes keep it awake.
- Layout is incremental: a change re-solves only the dirty path, and clean
  subtrees answer from a per-node cache, so long lists no longer cap layout
  (a thousand-node tree relays out in well under a millisecond). If layoutMs
  still grows with tree size, read the get_stats counters - a low
  cacheHits/cacheGets ratio means the layout cache is being defeated, high
  paraShapes means text is actually reshaping. Very long lists still pay
  for the initial mount and for memory, so windowing stays sensible at the
  thousands-of-rows scale.
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
