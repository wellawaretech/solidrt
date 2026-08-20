# SolidRT app - agent notes

This project uses SolidRT: a custom SolidJS renderer that paints through a Rust
runtime. No DOM, no HTML, no CSS cascade. If you are an AI assistant, read this
whole file before writing or editing code here - it is short on purpose. The
depth lives in the topic files listed under "Read before you", one of which you
should open whenever the work matches its trigger.

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

## Read before you

The authoritative references ship inside the installed packages. Open the one
that matches the work; do not work from memory of what a web framework does.

- write any reactive code (signals, effects, control flow) ->
  node_modules/solid-js/CHEATSHEET.md - the SolidJS 2.0 model
- touch elements, props, events, gestures or text ->
  node_modules/@solidrt/core/AGENTS.md, and
  node_modules/@solidrt/core/src/types.d.ts + jsx-runtime.d.ts (source of truth)
<!-- components:begin -->
- build UI from the component vocabulary ->
  node_modules/@solidrt/components/AGENTS.md, with full prop tables in its
  README.md and single-concept usage in its examples/ (see that README index)
<!-- components:end -->
- style a screen: a background, a gradient, a shadow, an effect, vector art,
  a chart -> node_modules/@solidrt/core/agents/painting.md
- write per-frame code, an animation, or anything writing properties in a
  loop -> node_modules/@solidrt/core/agents/performance.md
- debug a running app, or drive it over MCP to verify a change ->
  node_modules/@solidrt/cli/agents/debugging.md
- add an asset or font, set the app's identity, or build for distribution ->
  node_modules/@solidrt/cli/agents/assets.md
- run, bundle, typecheck or render headlessly ->
  node_modules/@solidrt/cli/AGENTS.md
- copy a working pattern -> node_modules/@solidrt/core/examples/ (see its
  README.md index)

<!-- Claude Code auto-imports these; other tools read the paths above. -->
@./node_modules/solid-js/CHEATSHEET.md
<!-- components:begin -->
@./node_modules/@solidrt/components/AGENTS.md
<!-- components:end -->
@./node_modules/@solidrt/core/AGENTS.md
@./node_modules/@solidrt/cli/AGENTS.md

## The things assistants get wrong (this is not React/DOM)

1. In a components-based app, most components split their props into two
   objects: `layout={{...}}` for anything that feeds the layout engine
   (flex/grid, sizing, padding/margin, position; font fields for Text), and
   `style={{...}}` for paint-only properties that never relayout
   (`backgroundColor`, `borderColor`, `borderWidth`, `borderRadius`, `color`,
   and the transform `x`/`y`/`rotate`/`scale`). Event handlers
   (`onPointerDown`, `onKeyDown`, ...) are top-level props, not inside
   `layout`/`style`. Core intrinsics take these props flat instead - there
   are no `layout`/`style` objects at that level.
2. `render(() => <App/>)` once, top level. The root MUST be a `<Window>`
   (from @solidrt/components) or the core `<window>` - it throws otherwise.
3. Components' `Window`/`View` do not paint on their own; they only paint
   when you set `style.backgroundColor`/`borderColor` etc - there is no
   separate background element to place by hand. The core `<view>`/`<window>`
   have no background prop at all: in a core-only app the background is a
   draw-primitive child (`<d-rect color={...} />`) behind the content.
4. There is no onClick/onPress on host elements: `onPointerDown` is how you
   make something tappable. A components-based app gets `onPress` from
   `Pressable`/`Button` on top of it.
5. Reactive window state: prefer the accessors windowSize(), safeArea(),
   displayScale(), windowFocused(), keyboardHeight(), pointerLocked()
   (re-exported from @solidrt/core) over onResize/onLayout callbacks for
   reading layout and window state. SafeArea (the component) is usually the
   simpler fix for avoiding notches and system UI. For mouse look,
   lockPointer(true) enters relative mouse mode (cursor hidden and confined,
   positions freeze) and pointer events keep reporting motion through
   movementX/movementY.
6. Animation is target-shaped first: declare `transition` on the element and
   write targets, and the runtime animates natively with no per-frame JS.
   Reach for per-frame work only for genuinely procedural motion, where
   onFrame((tick, frame) => {}) is the native hook (runtime-paced,
   auto-cleans, re-exported from @solidrt/core); requestAnimationFrame
   exists as a web-standard one-shot but is not the preferred driver. A JS
   tween loop or an animation library pushing interpolated values through
   signals is the single most expensive mistake available here - read
   @solidrt/core/agents/performance.md before writing either.
7. In a components-based app, reach for @solidrt/core directly only for what
   components doesn't wrap: raw host intrinsics and the `d-` (detached,
   non-layout) primitives like `d-rect`/`d-path`/`d-oval` for vector art or
   perf-sensitive positioned drawing, device/GPU subpath imports
   (@solidrt/core/camera, /microphone, /gpu), gradients
   (createLinearGradient/createRadialGradient), and createImage/decodeImage
   for images below the `Image` component's level. Components and core
   primitives compose freely in the same tree - a components-based app can
   drop to a `<d-path>` for one custom shape without giving up `View`/`Text`
   everywhere else.
8. tsconfig needs jsx:"preserve" + jsxImportSource:"@solidrt/core" (still
   true even when you build almost entirely with @solidrt/components -
   components are plain functions returning core JSX). Solid peer deps are
   pinned betas - do not bump them casually.
9. Use ASCII characters whenever possible in code and text - for example, no
   em-dashes (use a hyphen), no smart/curly quotes, no unicode symbols.
10. Prefer let over const. Use const only for real constants - a single fixed
    string or number value - and name those in ALL_CAPS.
11. Reading a signal/prop/store at the top level of a component body (not
    inside JSX, a `createMemo`, or an effect's compute phase) reads it
    untracked - it silently freezes at the initial value instead of updating
    on change. `createEffect` takes two arguments now: `(compute, apply)`.
    `compute` is the tracked read phase; `apply(value, prev)` runs untracked
    and is where side effects/DOM-equivalent writes belong. The old
    single-arg `createEffect(fn)` form is gone - using it is an error.
12. A scroll container (ScrollView, or anything on createScroll) needs an
    explicit main-axis size - a height, or flex inside a sized parent. With
    neither it resolves to 0 and its content silently vanishes; maxHeight
    alone does not size it (the auto size it would clamp is already 0). The
    runtime warns when this happens.
13. Text `lineHeight` is a MULTIPLIER of fontSize (the theme uses 1.3-1.6),
    not pixels. A CSS-reflex value like 22 makes each line box 22x the font
    size: the text becomes blank space and the parent balloons.
14. Signal writes flush on a microtask: a handler that sets a signal and
    immediately reads it back gets the OLD value. Read the new value in an
    effect, or call `flush()` (from @solidjs/signals) to force it through.
15. Portals cannot mount during the app's initial render: a Modal (or any
    createPortal content) that is visible at first mount throws "no mount
    target". Gate it behind a signal that starts false and open it after
    startup - overlay content is opened, not born open.
16. An element-valued prop (children, a content/icon slot) compiles to a
    getter that builds a fresh native subtree on EVERY read, and a subtree
    that is never inserted is never freed - native nodes are not garbage
    collected, so what is only wasted work in DOM Solid is a permanent
    memory leak here. Read such props exactly once, at the place they are
    mounted. To inspect children (a typeof probe, counting), resolve them
    first with the children() helper (re-exported from @solidrt/core) and
    probe the resolved memo - never `typeof props.children` on the raw prop.
17. Writing a signal or store from inside an owned scope - a component body, a
    `createMemo`, an effect's compute phase - throws
    `REACTIVE_WRITE_IN_OWNED_SCOPE` in dev. Calling a loader/init function in
    the component body that sets state is the classic React / Solid 1.x
    reflex and hits this every time. Move the write into an event handler, an
    effect's apply phase, or `onSettled`; opt in narrowly with
    `createSignal(v, { ownedWrite: true })` for a signal that genuinely is
    internal state.
18. Cover/contain images: give `Image` a `fit` prop ("fill" | "cover" |
    "contain" | "none" | "scale-down", CSS object-fit semantics, centered)
    plus a box via `layout` in any form - numbers, pct(), flex. Without
    `fit`, only NUMERIC layout sizes reach the image; `width: pct(100)`
    alone draws at intrinsic size. `fit="cover"` is the answer for the
    ported-web hero-image/thumbnail pattern.

## Run / verify

- FIRST check whether a dev server and a client are already running (the MCP
  list_clients tool) and build against those: `reload` pushes your edits to
  the live app, get_logs and get_snapshot verify them. Do not start a second
  `srt run` when one is already up.
- The dev loop is edit -> reload -> get_logs -> get_snapshot. `reload`
  surfaces build errors but not type errors; run `bunx srt check` for those.
- bunx srt run src/index.tsx     - dev server + window (needs a display)
- bunx srt check src/index.tsx   - exit 0 means it compiles and the app's
  types hold (dependency-internal type errors are hidden). Builds in memory:
  writes nothing and never triggers a dev-server reload, so use this while
  iterating - `srt bundle` writes output files and reloads connected clients
- bunx srt render src/index.tsx --size 480x640 --duration 1 --fps 2 - headless
  render to PNG frames (proves it renders; see the cli AGENTS.md for where the
  frames land). The project's assets/ resolve exactly as under `srt run`, so
  asset-dependent apps render headlessly too. It is also the ONLY way to see
  the output of a window shader, which every MCP capture is blind to.
- The project ships an MCP server (.mcp.json, `srt mcp`) that inspects and
  drives the running app: logs, render tree, snapshots, GPU resources, stats,
  synthetic input, a controllable clock. Each tool documents itself in full -
  prefer them over guessing at runtime state, and read
  node_modules/@solidrt/cli/agents/debugging.md before an investigation.
