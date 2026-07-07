# SolidRT app - agent notes

This project uses SolidRT: a custom SolidJS renderer that paints through a Rust
runtime. No DOM, no HTML, no CSS cascade. If you are an AI assistant, read this
before writing or editing code here.

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
   UI from @solidrt/components (Window, View, Text, Image, TextInput,
   ScrollView, Pressable, Button, SafeArea, theme/setTheme) - it is the
   higher-level, batteries-included vocabulary and is where most app code
   should live.
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
8. Reach for @solidrt/core directly only for what components doesn't wrap:
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

## Run / verify

- bunx srt run src/index.tsx     - dev server + window (needs a display)
- bunx srt bundle src/index.tsx  - exit 0 means it compiles
- bunx srt playback src/index.tsx --size 480x640 --duration 1 --fps 2 - headless
  render to PNG frames (proves it renders; see the cli AGENTS.md for where the
  frames land)
