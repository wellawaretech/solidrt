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

## What you paint with (there is no CSS layer)

Layout and props are half the model. There is no stylesheet: no filters, no
box-shadow, no keyframes, no canvas. The visual range a web app gets from CSS
comes from the tiers below instead, and reaching past tier 1 is ordinary
app-building here, not optimization - a screen built only from view
backgrounds and text is using a fraction of the runtime. Pick the tier the
CONTENT calls for, not the one that looks safest.

1. Laid-out elements - `<view>`/`<text>`, with `<rect>` (or a filling
   `<d-rect>` child) for background, border, radius. The structure of a
   screen, not its finish.
2. Vector art, detached from layout - `d-path`/`d-rect`/`d-oval`/`d-line`,
   whose `color` takes a gradient (createLinearGradient /
   createRadialGradient) and which honour `blendMode`, plus `parseSvg` to
   draw a whole SVG document as one subtree. Free-form shapes, decoration,
   diagrams, charts, anything positioned rather than flowed. Examples:
   parse-svg, detached-positioning, text-paint-styling.
3. GPU textures - `createShaderTexture` puts a fragment shader in a
   `<texture>` (moving gradients, noise, glow, dissolves, a background that
   is alive), `createPipelineTexture` draws geometry you generate yourself
   (particles, point clouds, splats), and the `shader` prop post-processes
   content that already exists: on a `<view>` it grades, warps or dissolves
   that subtree, on `<window>` the whole frame. Stack `<texture>` elements
   with `blendMode` to combine passes. Examples: gpu-shader, gpu-particles,
   gpu-pipeline, gpu-instancing, gpu-texture-blend, view-shader,
   window-shader.
4. 3D scenes - add `@solidrt/3d` (not a scaffold dependency): meshes,
   materials and a camera declared as Solid components, rendered into a
   texture that sits in the UI tree like any other element.

Tier 3 is cheaper than it looks. A shader costs one property write per frame
no matter how complex the effect, which is why the performance notes below
reach for it first rather than as a last resort.

Web reflexes and what replaces them:
- gradient background -> a gradient `color` on a `d-rect` (gradients are
  paint values, usable anywhere a color is)
- `filter: blur/grayscale/hue-rotate`, and any "make this look processed" ->
  a `shader` on the view (requires repaintBoundary="snapshot"), or on
  `<window>` for the whole frame
- `box-shadow` / `text-shadow` / glow -> no shadow prop exists: draw an
  offset `d-*` shape under the content, or a view shader with `outset` (the
  transparent margin an effect bleeds into)
- `backdrop-filter` -> no equivalent. A view shader sees only its own
  subtree's pixels, never what is behind it. Frost the whole frame with a
  window shader, or fake the layer with your own content
- `@keyframes` / transitions -> `onFrame` writing a signal for discrete
  motion; a `uTime` uniform when the animation is continuous and visual
- `<canvas>` 2D -> `d-*` primitives (rebuild one `d-path` string per frame
  rather than animating N elements)
- `<canvas>` WebGL, three.js -> `createPipelineTexture`, or `@solidrt/3d`
- video background, animated hero, particle field -> a shader texture; this
  is the case the runtime is built for

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
4. Components' `Window`/`View` do not paint on their own; they only paint
   when you set `style.backgroundColor`/`borderColor` etc - there is no
   separate background element to place by hand. That is the components level
   only: the core `<view>`/`<window>` have no background prop at all - in a
   core-only app the background is a draw-primitive child
   (`<d-rect color={...} />`) behind the content.
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
18. Writing a signal or store from inside an owned scope - a component body, a
    `createMemo`, an effect's compute phase - throws
    `REACTIVE_WRITE_IN_OWNED_SCOPE` in dev. Calling a loader/init function in
    the component body that sets state is the classic React / Solid 1.x
    reflex and hits this every time. Move the write into an event handler, an
    effect's apply phase, or `onSettled`; opt in narrowly with
    `createSignal(v, { ownedWrite: true })` for a signal that genuinely is
    internal state.
19. Transform origin on a `d-view`: unset `originX`/`originY` pivots
    scale/rotate at the view's local (0,0), the point its children's
    coordinates are drawn against (a laid-out view pivots at its own box
    center; a d-view has no box). To pivot a detached group around its
    content's center, set the origin explicitly in pixels
    (`originX={100} originY={50}` for content drawn in a 200x100 local
    space). Avoid pct()/keyword origins on a d-view - they resolve against
    the box inherited from the nearest laid-out ancestor.
20. Cover/contain images: give `Image` a `fit` prop ("fill" | "cover" |
    "contain" | "none" | "scale-down", CSS object-fit semantics, centered)
    plus a box via `layout` in any form - numbers, pct(), flex. Without
    `fit`, only NUMERIC layout sizes reach the image; `width: pct(100)`
    alone draws at intrinsic size. `fit="cover"` is the answer for the
    ported-web hero-image/thumbnail pattern.

## Performance model (JS is the slow lane)

The JS engine is interpreted and every property write crosses an FFI boundary
into the runtime, so per-frame JS work is the expensive path while GPU work is
nearly free. That holds on desktop and on current mobile hardware; "Where GPU
work stops being free" below is where it does not. Rules, in order of leverage:

1. Continuous effects (snow, particles, animated backgrounds) belong in a
   fragment shader: createShaderTexture (from @solidrt/core/gpu) + `<texture
   params={{ uTime }}>` (the shader declares `uniform float uTime;` itself -
   the preamble declares only what the runtime fills). The whole effect then
   costs one setProperty per frame - the uTime write - regardless of visual
   complexity. Shader output
   must be premultiplied alpha (white flakes are `vec4(vec3(a), a)`);
   straight alpha (`vec4(1,1,1,a)`) composites as opaque white. A source that
   starts with `#version 300 es` is compiled exactly as written - no preamble
   is injected, though the built-in vertex stage still supplies `vUV` - so a
   shader ported from elsewhere keeps its own uniform names without dropping
   to compileShader/linkProgram. Params drive any uniform type: a number
   fills a `float`/`int` scalar, a flat number array fills `vec2`/`vec3`/
   `vec4` (2/3/4 numbers) or `mat4` (16, column-major), dispatched by the
   shader's own declaration - a ported shader's `vec2 uCenter` or
   `vec3 iResolution` needs no splitting into scalars. To combine several
   GPU passes, stack `<texture>` elements and set `blendMode` (e.g. a base
   pass plus an additive `blendMode="plus"` pass) rather than writing a
   compositing shader. Within one pipeline draw, createPipelineTexture's
   `blend: "add"` accumulates overlapping geometry additively (soft point
   splats, glow) - pair it with `depthWrite: false` when depth-tested;
   neither option implies the other. A pipeline's own vertex stage writes
   into a y-down clip space: `gl_Position` y = -1 is the top row of the
   target and +1 the bottom, so camera-up geometry must negate y (or fold
   the flip into its projection) or it draws upside down. Sampling is a
   create-time option on every texture: `{ filter: "nearest" }` for
   hard-pixel upscaling (render a small target, display it big - the
   retro/pixel-art path) and `{ wrap: "repeat" }` to tile outside 0..1 in
   shaders; the defaults are linear and clamp, and the choice applies both
   on screen and to shaders sampling the texture.
2. Reduce setProperty calls wherever possible: one path string rebuilt per
   frame beats N elements with N animated positions; a shader beats the path
   string. get_stats' setPropsPerFrame is the counter to watch. Compiled JSX
   attribute expressions diff before writing, so a per-frame expression that
   returns an unchanged value costs no property write - setPropsPerFrame
   counts values that actually changed, not expressions re-run.
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
   with transforms and keep the cached content itself static. Off a boundary,
   `opacity` on a view is NOT cheap: it wraps the subtree in a compositing
   layer (save_layer) for as long as it is below 1. To fade a single
   primitive, put the alpha in its `color` (`rgba(...)`) - paint alpha is
   free; reserve view `opacity` for fading a genuine group as a whole.
5. "snapshot" boundaries pay first-frame texture allocation + raster:
   creating many at once (dealing a board of 64 sprites) is a visible
   one-frame hiccup - pool or pre-warm if that moment matters.
6. Shading pixels the app already drew is a different mechanism from rule 1's
   generated textures, and both forms are a `shader` prop taking a linked
   program from compileShader/linkProgram (@solidrt/core/gpu), not a
   createShaderTexture source. On `<window>`, `shader={{ program, params }}`
   runs the finished frame through the program as the last step before it
   reaches the screen: the frame binds as `uniform sampler2D uSource`,
   `iResolution` fills by name, and `previous: true` retains the last frame as
   `uPrevious` for motion echo or frame differencing. On a `<view>` the same
   prop shades that subtree in place and REQUIRES repaintBoundary="snapshot"
   (without it the shader is ignored with a warning); the pass sees only the
   subtree's own pixels - grading, warping or dissolving the panel works,
   anything needing what is behind it does not - and is split from content
   invalidation, so a params-only change re-runs the pass against the cached
   snapshot instead of re-rasterizing. A window shader's output is invisible
   to get_snapshot and every other MCP tool; `srt render` is the only way to
   see it (Run / verify below).
7. `flux:wasm` is not the fast lane. It runs a pure interpreter (wasmi, no
   JIT), so tight typed compute gains a small constant factor over the same
   loop in JavaScript, nowhere near browser wasm speed, and every host call
   costs marshalling. Use it to ship one compiled module across every target
   without native binaries, not to speed up per-frame work; for that, rules
   1-2 (move it to the GPU, cut property writes) are the leverage.

### Isolates: heavy work off the JS thread

A long synchronous computation (a big parse, a simulation step, a blocking
`flux:ffi`/`flux:wasm` call) freezes rendering and input for its duration.
Move it into an isolate module: a file whose first statement is the
`"use isolate"` directive runs in a second runtime on its own thread, and
main calls its exports as async functions.

```ts
// src/worker.ts
"use isolate"
export function crunch(data: Uint8Array): number { /* ... */ }
```

```ts
// src/index.tsx
import { isolate } from "flux:isolate"
import type * as Worker from "./worker"
let worker = isolate<typeof Worker>("worker")   // id = path from src/, no extension
let n = await worker.crunch(bytes)              // main keeps rendering meanwhile
```

The bundler builds each such module as its own bundle and ships it with the
app (dev pushes and `srt pack` alike). Rules: main may only `import type`
from an isolate module (a value import is a build error); arguments and
results are copies (numbers, strings, byte buffers, arrays, plain objects -
no functions, no class instances); the child has the non-gui `flux:*`
modules only, so it never touches the render tree; module state persists
between calls and each `isolate()` call is its own instance. An
`async function*` export is a stream: `for await (let p of worker.progress())`
pulls one item per step (progress, ticks, a subscription), `break` ends it in
the isolate, and streams never block plain calls. Full contract:
node_modules/@solidrt/flux-types/modules/isolate.d.ts.

### Where GPU work stops being free

"GPU work is nearly free" is a property of the hardware, not of the engine, and
the spread is wide enough to design against rather than discover late. The same
app - two point-cloud pipelines, 233,600 vertices, one params write each per
onFrame, i.e. exactly what rule 1 recommends - measured 16.7 ms/frame (60 fps,
vsync-locked) on both desktop and a mid-range 2020 tablet, and 120 ms/frame
(8.3 fps) on a 2017 Android TV. Roughly 8x for identical work, with the tablet
indistinguishable from desktop. Measure on a target device if it matters; do
not infer it from the desktop number.

- **On a tiled GPU the budget is primitive count, not pixels.** Every point or
  triangle costs the tiler regardless of how few pixels it covers. On that TV,
  frame time against total vertices with a trivial vertex shader: 20k -> 80 ms,
  35k -> 100 ms, 100k -> 380 ms. Meanwhile `gl_PointSize = 3.0` - nine times
  the fill - measured within one vsync of 1.0, and rendering into a
  quarter-size target measured identical to full size. So for a heavy pass the
  lever is fewer primitives; shrinking the target or the splat usually is not,
  and coverage is far cheaper bought with point size than with more points.
- **A device's compositor can set the frame budget outright**, in which case
  none of the above moves. That TV never presents faster than every 80 ms -
  four refresh periods blocked inside `eglSwapBuffers` - even for a near-empty
  scene, so its ceiling is ~12 fps whatever you draw. Recognise it by a
  content-independent floor: if a trivial scene and a heavy one present at
  nearly the same rate, you are compositor-bound and tuning the scene is
  wasted effort.
- **Per-frame writes are gated on the raster thread, not on vsync**, so a pass
  that costs more than a refresh period does not silently pile up. If
  `rasterQueue` sits persistently above 0 the raster thread is behind; if
  `fenceTimeouts` climbs, the GPU is over its pacing budget.

Finding your own numbers: `get_stats` gives fps, frameMs, setPropsPerFrame,
rasterQueue and fenceTimeouts. When those disagree with what the screen is
visibly doing, ground truth on Android is
`adb shell dumpsys SurfaceFlinger --latency <layer>` for real present
timestamps - engine-reported phase timings can each be honest and still not add
up to the frame period, because work outside the frame call is not in them.

## Assets and app identity

- Everything under `assets/` ships with the app: the folder is collected
  wholesale into each build's version manifest (no bundler analysis, no
  registration step). Reference assets by path - `file("assets/sounds/x.ogg")`
  from `flux:fs` - and treat them as read-only at runtime; writes belong in
  plain relative paths, which land in the app's private data dir.
- Small text-like assets (SVG documents, shaders) can instead be inlined via
  imports. An import attribute picks the form and works on any extension:
  `import src from "./effect.glsl" with { type: "text" }` yields the file's
  contents as a string, `with { type: "binary" }` yields a Uint8Array. `.svg`
  is text-loaded with no attribute needed. Shader sources (`.glsl`/`.vert`/
  `.frag`) are declared as text modules out of the box, so they typecheck
  without setup. Inlining trades update granularity for zero I/O - keep big or
  streamable files (audio, images) in `assets/`.
- Custom fonts go in `assets/fonts/` and are declared in the `solidrt.fonts`
  map in package.json (alias -> file path; role aliases `sans`/`serif`/`mono`
  replace the built-in defaults, `false` drops one, other keys add fonts
  selectable via fontFamily). A newly added font shows after restarting the
  client.
- The `solidrt` key in package.json is the app's identity: set a stable
  reverse-DNS `appId` before distributing - it keys the app's storage
  folder, defaults from the package name in dev, and `srt pack` warns
  while defaulted. `org` and `displayName` are optional display metadata
  (future launcher/window naming) with no storage meaning.
- `bunx srt pack src/index.tsx` builds a single-file executable;
  `bunx srt pack --folder src/index.tsx` writes the flat app folder
  (runner + manifest.json + bundle + assets/, plus the runner's GL
  libraries on Windows and macOS) to `dist/`.

## Run / verify

- FIRST check whether a dev server and a client are already running (MCP
  list_clients, see below) and build/test against those: `reload` pushes your
  edits to the live app, get_logs and get_snapshot verify them. Do not start a
  second `srt run` when one is already up.
- bunx srt run src/index.tsx     - dev server + window (needs a display)
- bunx srt check src/index.tsx   - exit 0 means it compiles and the app's
  types hold (dependency-internal type errors are hidden). Builds in memory:
  writes nothing and never triggers a dev-server reload, so use this while
  iterating - `srt bundle` writes output files and reloads connected clients
- bunx srt render src/index.tsx --size 480x640 --duration 1 --fps 2 - headless
  render to PNG frames (proves it renders; see the cli AGENTS.md for where the
  frames land). It is also the ONLY way to see the output of a window shader
  (the `shader` prop on `<window>`): that pass runs on the finished frame on
  its way to the screen, past the point every other capture reads, so `render`
  frames are the only programmatic view of what it produces

## MCP: inspect the running app

The project ships an MCP server (.mcp.json, `srt mcp`) that talks to the dev
server `bunx srt run` starts. When it is loaded in your environment, prefer
its tools over guessing at runtime state:

- list_clients: connected app clients, their platform and runtime
  capabilities, plus the server's `entry` (the app source it serves) and
  `projectDir` - check entry matches the app you think you are driving. Each
  tool call finds the dev server currently serving this project (by its
  project root), so a server restarted on another port/session is followed
  automatically; no server serving this project is an error, not a wrong
  server.
  Each client also lists `queries`, the dev-tool query kinds its runtime
  answers - check it before planning verification against a mixed-version
  fleet (no "input" = the client predates send_input)
- get_logs: console output and runtime errors (seq cursor; `wait_ms` long-poll
  to catch output right after a reload; `level`/`contains` filters; repeated
  lines collapse into one entry with a `repeats` count)
- get_render_tree: what the app actually rendered - node kinds, text, and
  window-relative boxes. Pass `props: true` for each node's current
  property values (JSX names, off-default only - "is rotate/color/overflow/d
  applied right now" is one call, not a probe entry) and, on transformed nodes, the
  painted `quad` (four corners after transforms; the box is just its
  axis-aligned bounds). Whole trees get large: `query` finds nodes by
  kind/text, then `root` + `depth` inspect just that region
- client ids and log cursors die with the dev server: list_clients and
  get_logs responses carry `generation`, and a changed generation means
  re-fetch ids and restart cursors
- get_stats: fps, CPU/memory, frame phase timings, setProperty rate, plus
  layout-activity counters for the last rebuild (nodes, measureCalls,
  paraShapes/wordHits, dirtiedNodes, cacheGets/cacheHits, nodesPainted) - when
  layoutMs looks wrong, these say whether the cost is text shaping,
  invalidation breadth, or a defeated layout cache (healthy incremental
  rebuilds show a near-100% cacheHits rate); when paintMs looks wrong,
  nodesPainted against mountedNodes says whether viewport culling is doing
  its job (a long scroller paints a near-constant node count however long
  its content). reusedPerSec/skippedPerSec are the demand gate's visible
  signal: frames presented from the cached display list without a rebuild
  (texture content changed, no property writes - expect reusedPerSec near
  fps on texture-driven apps) and frames skipped entirely (nothing
  requested one)
- get_snapshot: PNG capture of any render-tree node's pixels (get node ids
  from get_render_tree; the window node captures everything). A subtree
  capture renders with NO ancestor paint: pixels the subtree does not draw
  come back transparent, not the background behind the node. Captures
  re-rasterize the tree offscreen, so they are also PRE window shader: an app
  with a `shader` on its `<window>` snapshots as its unshaded content, window
  node included, and no MCP tool reads the shaded result (its layer is
  runtime-owned, so get_texture has no id for it; get_gpu_resources reports
  only that the pass exists). Use `srt render` for that one. Crop with
  x/y/width/height (captured-image pixels) and magnify with `scale` (1-8,
  nearest-neighbour) - a tight crop at 4-8x is how small geometry gets
  verified. Pass `save_to` on get_snapshot or get_texture to also write the
  PNG to a file - the image in the tool result cannot be saved afterwards,
  so decide before capturing (e.g. keep a before/after pair to diff)
- set_time_scale / step_frames: the runtime clock. `set_time_scale 0`
  freezes app time (onFrame, requestAnimationFrame, timers, and
  performance.now all stop; Date.now stays wall time), so a snapshot can
  catch an exact frame of any animation instead of racing it; `step_frames
  n` then advances exactly n frames (one refresh period each). Pause,
  snapshot, step, snapshot again to see precisely what changed. ALWAYS set
  the scale back to 1 when done - a paused client looks wedged to the human
  watching - though reload/load also reset it
- send_input: synthetic pointer/key/wheel/text events through the REAL
  input pipeline (hit testing, focus, bubbling) - the way to verify an
  interaction actually works, where call_debug would bypass it. A click is
  one call ({type: "pointer", action: "tap", x, y} - logical points, the
  same space get_render_tree reports); a key hold is {type: "key", action:
  "tap", key: "w", holdMs: 500}; text needs the field focused first (tap
  it), then {type: "text", text: "go"}; drags are down + moves (delayMs
  ~16 apiece) + up. Sequences run in order with per-event delayMs and the
  call returns after the last event is delivered, so a following snapshot
  sees the result. A synthetic mouse keeps hovering at its last position
  (like a real cursor at rest); use pointerType: "touch" for gestures that
  should end hover-free. Composes with the clock: pause, send_input,
  step_frames, snapshot = a deterministic interaction test
- get_gpu_resources: inventory of GPU state - textures (size, render target
  or not), vertex buffers (byteLength), pipelines (draw count, attribute
  layout, bound textures, current uniform values - the most recent writes,
  which the next frame or readback draws with)
- get_texture: any GPU texture read back as a PNG by id - atlases, data
  textures, and shader/pipeline render targets alike (a render target reads
  as its current output, pending writes included, with no frame or snapshot
  needed); crop with x/y/width/height, magnify with `scale`
- get_buffer: a vertex-buffer range decoded to numbers (f32/u16/u8, 64 KiB
  per call) - verify geometry after a writeBuffer instead of inferring it
  from pixels
- list_debug / call_debug: the app's own debug commands (registered with
  `registerDebug` from `srt:dev`) - list them, then invoke by name with a
  JSON argument. Per client, like get_snapshot
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
`bunx srt run src/index.tsx`. The bridge dials the dev server's default port
(34884), so if the user started it with `--port N`, .mcp.json needs the same
flag: `"args": [..., "mcp", "--port", "N"]`.

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
  reload pushes to all of them, but call_debug / send_input / get_snapshot
  / log cursors are per client, and interactive state does NOT sync - a flow
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
  presents - every frame at the refresh rate until you deregister it (fps stays
  at the refresh rate on an idle screen; that is the 60fps burn called out in
  the performance notes above). The upside is that a self-running loop - a game
  clock, a shader driver, a stepped VM doing silent CPU work with no console
  output - keeps advancing on its own: it does NOT stall when the body changes
  nothing and needs no startup "prime" write. Deregister onFrame (return its
  cleanup, or let onCleanup fire) whenever there is nothing left to advance.
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
