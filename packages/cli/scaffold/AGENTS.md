# SolidRT app - agent notes

This project uses SolidRT: a custom SolidJS renderer that paints through a Rust
runtime. No DOM, no HTML, no CSS cascade. If you are an AI assistant, read this
before writing or editing code here.

Authoritative references ship inside the installed packages - read them:
- node_modules/@solidrt/core/AGENTS.md   - element/prop/reactivity model
- node_modules/@solidrt/cli/AGENTS.md    - running, bundling, headless verify
- node_modules/@solidrt/core/src/types.d.ts and jsx-runtime.d.ts - source of truth

<!-- Claude Code auto-imports these; other tools read the paths above. -->
@./node_modules/@solidrt/core/AGENTS.md
@./node_modules/@solidrt/cli/AGENTS.md

## The things assistants get wrong (this is not React/DOM)

1. SolidJS 2.0, not React (and not Solid 1.x). Import the whole authoring
   surface from @solidrt/core - it re-exports the substrate so you do not have to
   know which package a symbol lives in: render plus the window/paint/event APIs,
   the reactive primitives (createSignal, createMemo, createEffect, createStore,
   reconcile, mapArray, untrack, onCleanup), and the control-flow components (For,
   Show, Switch/Match, Repeat, Loading, Errored). No hooks, no virtual DOM, the
   component body runs once. Render lists with <For each={...}>{item => ...}</For>,
   never array.map. createEffect has the 2.0 two-function shape - a TRACKED
   compute that reads signals and returns a value, then an UNTRACKED effect that
   receives it:
     createEffect(() => count(), (c) => console.log(c))
   The Solid 1.x single-callback form, createEffect(() => console.log(count())),
   does NOT track in 2.0.
2. Host elements are lowercase intrinsics: window, view, text, rect, oval, path,
   texture, audio (+ d- variants). No div/span/img/button.
3. render(() => <App/>) once, top level. The root MUST be <window> or it throws.
4. Containers (window, view) DO NOT PAINT. Background = a draw primitive child
   placed behind the content, e.g. <d-rect color="..." />.
5. Paint color is the color prop (a CSS color string). No fill/stroke/background
   prop. Outline: drawStyle="stroke" + strokeWidth.
6. No onClick/onPress. A button is a <view>/<rect> with onPointerDown.
7. d- prefix = detached from layout: a plain element is laid out by Taffy, a
   d-element you position with x/y (omit to fill the parent = how backgrounds
   work). A detached node cannot have attached (regular, Taffy-laid-out)
   children - everything under a d-element must itself be detached. Nesting a
   plain <view>/<text> inside a d-element is an error.
8. Per-frame work: onFrame((tick, frame) => {}), or the standard
   requestAnimationFrame(t => {}) for animations. Also onResize, onLayout.
9. Device/GPU via subpath imports: @solidrt/core/camera, /microphone, /gpu.
10. tsconfig needs jsx:"preserve" + jsxImportSource:"@solidrt/core". Solid peer
    deps are pinned betas - do not bump them casually.
11. Use ASCII characters whenever possible in code and text - for example, no
    em-dashes (use a hyphen), no smart/curly quotes, no unicode symbols.
12. Mind the safe area. safeArea() from @solidrt/core is a reactive accessor
    returning { top, left, right, bottom } insets (like CSS
    env(safe-area-inset-*)). It is fine to place content outside the safe-area
    zone (e.g. full-bleed backgrounds), just be aware it may not be visible and
    cannot be interacted with (touch gestures). Keep interactive or essential
    content inside the safe area by padding the edges.
13. Prefer let over const. Use const only for real constants - a single fixed
    string or number value - and name those in ALL_CAPS.
14. SolidJS/SolidRT is a reactive framework: strongly prefer reactive primitives
    (signals, memos, effects, the control-flow components) over imperative code.
    Derive state with createMemo, react with createEffect, render conditionally
    with <Show>/<Switch> and lists with <For> - do not hand-roll imperative
    updates, manual subscriptions, or DOM-style mutation.

## Run / verify

- bunx srt run src/index.tsx     - dev server + window (needs a display)
- bunx srt bundle src/index.tsx  - exit 0 means it compiles
- bunx srt record src/index.tsx --size 480x640 --duration 1 --fps 2 - headless
  render to PNG frames (proves it renders; see the cli AGENTS.md for where the
  frames land)