# SolidRT app - agent notes

This project uses SolidRT: a custom SolidJS renderer that paints through a Rust
runtime. No DOM, no HTML, no CSS cascade. If you are an AI assistant, read this
whole file before writing or editing code here - it is short on purpose. The
depth lives in the topic files listed under "Read before you", one of which you
should open whenever the work matches its trigger.

## Levels: core, and extensions on top

- @solidrt/core is the low-level foundation: host intrinsics (`<window>`,
  `<view>`, `<text>`, the detached `d-*` drawing primitives) with flat props
  that feed the layout and paint engine directly. An app can be written
  entirely at this level.
- Extensions build on core. The first-party ones are @solidrt/components
  (UI components), @solidrt/2d (2D graphics) and @solidrt/3d (3D graphics).
  None is privileged - an extension is just functions returning core JSX,
  and an app can use a third-party one or grow its own.

Match the level the code you are editing already uses. package.json shows
the choice this app made: only the extensions listed there are installed -
do not add one for a change core covers.

Convention: every @solidrt package describes itself in
node_modules/@solidrt/<name>/AGENTS.md and ships working code in its
examples/. That file is the reference for the package; open it before using
anything from the package, and read the extensions' files only when they
are installed.

## Read before you

The authoritative references ship inside the installed packages. Open the one
that matches the work; do not work from memory of what a web framework does.

- write any reactive code (signals, effects, control flow) ->
  node_modules/solid-js/CHEATSHEET.md - the SolidJS 2.0 model
- touch elements, props, events, gestures or text ->
  node_modules/@solidrt/core/AGENTS.md, and
  node_modules/@solidrt/core/src/types.d.ts + jsx-runtime.d.ts (source of truth)
- style a screen: a background, a gradient, a shadow, an effect, vector art,
  a chart -> node_modules/@solidrt/core/agents/painting.md
- write per-frame code, an animation, or anything writing properties in a
  loop -> node_modules/@solidrt/core/agents/performance.md
- use an installed extension (UI components, 2D, 3D) ->
  node_modules/@solidrt/<name>/AGENTS.md and its examples/
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
@./node_modules/@solidrt/core/AGENTS.md
<!-- components:begin -->
@./node_modules/@solidrt/components/AGENTS.md
<!-- components:end -->
<!-- 2d:begin -->
@./node_modules/@solidrt/2d/AGENTS.md
<!-- 2d:end -->
<!-- 3d:begin -->
@./node_modules/@solidrt/3d/AGENTS.md
<!-- 3d:end -->
@./node_modules/@solidrt/cli/AGENTS.md

## The three traps that cost the most (this is not React/DOM)

Each package's AGENTS.md carries its own trap list; these three are
platform-wide and bite in every app:

1. Reading a signal/prop/store at the top level of a component body (not
   inside JSX, a `createMemo`, or an effect's compute phase) reads it
   untracked - it silently freezes at the initial value. `createEffect` is
   two-argument here: `(compute, apply)`; the Solid 1.x single-arg form does
   not track.
2. Writing a signal or store from inside an owned scope - a component body,
   a `createMemo`, an effect's compute phase - throws
   `REACTIVE_WRITE_IN_OWNED_SCOPE` in dev. Move the write into an event
   handler, an effect's apply phase, `onSettled`, or an `untrack` block; opt
   in with `createSignal(v, { ownedWrite: true })` for internal state.
3. An element-valued prop (children, a content/icon slot) builds a fresh
   native subtree on EVERY read, and an uninserted subtree is never freed -
   a permanent memory leak, not wasted work. Read such props exactly once,
   where they mount; inspect them through the `children()` helper.

## Run / verify

- FIRST check whether a dev server and a client are already running (the MCP
  list_clients tool) and build against those: `reload` pushes your edits to
  the live app, get_logs and get_snapshot verify them. Do not start a second
  `srt run` when one is already up.
- The dev loop is edit -> reload -> get_logs -> get_snapshot. `reload`
  surfaces build errors but not type errors; run `bunx srt check src/index.tsx`
  for those (it builds in memory and never triggers a reload).
- Commands, headless rendering, and the MCP tools are documented in
  node_modules/@solidrt/cli/AGENTS.md and its agents/debugging.md.
