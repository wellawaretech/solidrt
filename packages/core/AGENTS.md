# @solidrt/core - agent notes

Dense, self-contained facts for writing a SolidRT app. The prose lives in
docs/ (also the website); when this conflicts with it, trust this file and
the types in src/types.d.ts and jsx-runtime.d.ts.

SolidRT is a custom SolidJS renderer: it paints through a Rust runtime, not the
DOM. There is no HTML, no CSS cascade, no `className`.

Two companion files carry the depth this one leaves out; read the one that
matches the work before starting it:
- agents/painting.md - what you paint with, and what replaces each CSS
  reflex. Read before styling a screen: a background, a gradient, a shadow,
  an effect, vector art, a chart.
- agents/performance.md - the performance model, in order of leverage. Read
  before writing any per-frame code, any animation, or anything that writes
  properties in a loop.

## The window is device-sized - design fluid

A SolidRT window is host-sized and resizable, and the SAME app runs on phones,
tablets, and desktops. There is no default "desktop" size to design against.
Design fluid by default: derive sizing and layout from the live window, do not
hardcode desktop pixels.

- Size the layout from `windowSize()` / `capabilities.windowSizeClass`, not from
  fixed pixel widths. Let flex (`flex`, `flexWrap`, `gap`) and percentages carry
  the layout so it reflows on resize instead of clipping or leaving dead space.
- Gate hover-only affordances on `capabilities.hover`. Hover-zoom, hover-reveal,
  and tooltips are dead on a touch device with no pointer that can rest. When
  `capabilities.touch`, provide a tap/press path to the same action.
- `windowSizeClass` uses Material 3 width breakpoints (logical px): `compact`
  (<600), `medium` (600-840), `expanded` (>=840). Drive column counts / layout
  switches off it (see the responsive-grid example).

Exception - fixed-aspect content. For content with fixed internal geometry
(diagrams, slides, dashboards, games, emulators), do not branch on window size
at all: author everything in one design space and let `designSize` fit it.
`<view flex={1} designSize={[1280, 800]}>` uniformly scales and centers the
children (letterboxed), pointer events on them arrive in design coordinates,
and the same code runs unchanged from a desktop window to a phone. Laid-out
children (flex, percentages, text wrap) resolve against the design size too,
so a whole panel scales into a smaller box without reflowing; the view itself
sizes like a replaced element whose intrinsic size is the design size. One
trap from flexbox, not from designSize: in a flex row a width-only design-size view
is stretched to the line's height under the default alignment, so give the
view `alignSelf="flex-start"` (or the row a non-stretch `alignItems`) to get
the design aspect - `aspectRatio` does not override stretch. Reach for
`windowSizeClass` branching only when the layout genuinely reflows across
form factors.

`env` and `capabilities` (both exported from `@solidrt/core`) are the two
objects that expose this. They are plain objects with REACTIVE GETTERS, not
functions - read them as `capabilities.windowSizeClass`, `env.displayScale`
(NOT `capabilities()`); the getter reads reactive state underneath, so a read
inside JSX / a memo / an effect re-runs when it changes. (`windowSize()` and
`safeArea()` from `./window` ARE functions - call those.)

```ts
import { env, capabilities } from "@solidrt/core"

env.windowSize        // { width, height } (same value as windowSize())
env.displayScale      // device pixel ratio (hi-DPI factor)
env.safeArea          // inset distances from each edge
env.systemTheme       // "dark" | "light" | "unknown" (resolves after startup)
env.orientation       // "portrait" | "landscape" | ... | "unknown"

capabilities.windowSizeClass   // "compact" | "medium" | "expanded"
capabilities.hover             // a pointer can rest over content (mouse/trackpad)
capabilities.touch             // direct touch input present
capabilities.precisePointer    // pixel-precise pointing (mouse/trackpad)
capabilities.keyboardNav       // hardware-key navigation available
```

Read behavior decisions through `capabilities`; read `env` directly only when
you need the raw fact (e.g. `env.displayScale` for asset sizing below).

### Vectors vs raster, and hi-DPI

Because the drawn size is fluid and the display DPI varies, asset format is a
real design decision, not an afterthought:

- Prefer VECTORS (`parseSvg` draws mapped to `<d-path>` in a `designSize` view)
  whenever the render size is fluid or DPI varies - they stay crisp at any
  size x `displayScale()`.
- RASTER (`<texture>` / `createImage`) needs source resolution >= displayed size
  x `env.displayScale`, or it softens on hi-DPI. Author raster at 2-3x the
  largest size you will ever draw it. A 256px PNG blown up large on a hi-DPI
  tablet will look soft; the same art as SVG will not.

## Setup

```sh
bun add @solidrt/core        # the renderer
bun add -d @solidrt/cli      # the `srt` tool (see its AGENTS.md)
```

Core primitives alone are enough to build a full app. The optional extensions
(each with its own AGENTS.md) build on it: `@solidrt/components` (themed
widgets), `@solidrt/2d` (2D graphics and games), `@solidrt/3d` (scene graph).

tsconfig.json - the two load-bearing lines are jsx + jsxImportSource:

```json
{ "compilerOptions": {
  "jsx": "preserve",
  "jsxImportSource": "@solidrt/core",
  "moduleResolution": "bundler",
  "strict": true
} }
```

Peer deps @solidjs/signals and @solidjs/universal must match (currently
2.0.0-rc.1); bun resolves them from peerDependencies.

## Element model (the parts that are easy to get wrong)

- `render(() => <App />)`. The returned root MUST be a `<window>` or it throws.
  Call render once, at the top level.

- Errors never halt the app. One thrown while computing an element's props or
  a child expression is contained at that element: it keeps its last good
  value, one `Contained error` log line names the node and the .tsx line,
  and it recovers when the expression computes again. Anything unclaimed
  beyond that (a throwing `createEffect`, an error while mounting) reaches
  render()'s root boundary, which replaces the app's window with an error
  window (message, stack, a Reset button that retries the failed
  computations) and logs `Uncaught error`. `<Errored>` gives a subtree its
  own in-place fallback.

- Two kinds of element:
  - Containers - `<window>`, `<view>`. Do layout + transform + pointer events.
    THEY DO NOT PAINT. A `<view>` has no background/fill prop.
  - Draw primitives - `<rect>`, `<oval>`, `<path>`, `<texture>`, `<text>`.
    These paint. To give a view a background, render a draw primitive (e.g.
    `<d-rect>`) as a child, behind the content.

- Paint color is the `color` prop (a CSS color string). There is NO `fill`,
  `stroke`, or `background` prop (some older doc examples are wrong about this).
  Outlines: `drawStyle="stroke"` (or "stroke-and-fill") plus `strokeWidth`.
  Corner radius on draw primitives: `radius` (number or [tl, tr, br, bl]).

- Registered JSX intrinsics: `window`, `view`, `text`, `span`, `rect`, `oval`,
  `line`, `path`, `texture`, `audio`, plus the `d-` variants `d-view`, `d-rect`,
  `d-oval`, `d-line`, `d-path`, `d-texture`, `d-text`. Line endpoints
  (`x1`/`y1`/`x2`/`y2`) exist only on `d-line`; a laid-out `<line>` has no
  endpoint props and spans its layout box corner to corner. `points` (a flat
  `[x0, y0, x1, y1, ...]` array, plus `closed`) turns either form into a
  polyline and wins over the endpoints while set.

- Plain vs `d-` variant (the `d-` prefix means "detached" - detached from the
  layout engine, Taffy): a plain element (e.g. `rect`) is `RectProps &
  LayoutProps`, so it draws AND is laid out by Taffy. The detached variant
  (`d-rect`) is `RectProps` only - it draws but is NOT in the layout pass; you
  place it yourself with `x`/`y` (omit them and it fills the parent, which is
  how backgrounds work). Reach for `d-` whenever you want explicit coordinate
  positioning instead of layout. It is also a performance lever: for many
  directly-positioned, often-animating elements (e.g. hundreds of balls), `d-`
  skips the per-element layout that plain elements would incur.

- Transform origin on a `d-view`: with `originX`/`originY` unset, scale/rotate
  pivot at the view's local (0,0) - the origin its children's coordinates are
  authored against - not at a box center (a laid-out view pivots at its own
  box center; a d-view has no box). To scale a detached group around its
  content's center, set the origin explicitly in pixels, e.g.
  `originX={100} originY={50}` for content drawn in a 200x100 local space.
  Avoid pct()/keyword origins on a d-view - they resolve against the box
  inherited from the nearest laid-out ancestor.

- Layout-affecting vs not (this matters for per-frame work). Props fall in three
  buckets, split by where they take effect:
  - `LayoutProps` - width/height, min/max sizes, margin, padding, `position` and
    its `top`/`right`/`bottom`/`left` offsets, flex*/gap/display, grid*,
    aspectRatio, overflow. Changing ANY of these triggers a Taffy reflow of the
    node and its subtree.
  - `TransformProps` - x, y, scale/scaleX/scaleY, rotate/rotateX/rotateY,
    perspective, cx/cy, scrollX/scrollY. Applied at paint/composite; NO reflow.
  - `PaintProps` - color, drawStyle, strokeWidth, blendMode, radius. Also no
    reflow.
  So to MOVE or animate an element - dragging, transitions, per-frame motion -
  translate it with the transform `x`/`y` (or scale/rotate), never by animating
  `left`/`top`/`margin`/`width`. Common trap: `left`/`top` read like "position"
  but they are LAYOUT offsets (for `position:absolute`), so driving them every
  frame reflows the tree. Anchor the element once with layout (e.g.
  `position:absolute` at `left:0,top:0`, or just let normal flow place it) and
  then translate it with `x`/`y`.

- Text `lineHeight` is a MULTIPLIER of fontSize (1.3-1.6 is typical), not
  pixels. A CSS-reflex value like 22 makes each line box 22x the font size:
  the text becomes blank space and the parent balloons.

- JSX text children collapse whitespace (ordinary JSX semantics): runs of
  spaces become one, so space-padding a mono label collapses silently. An
  expression container preserves it - `<d-text>{"one    two"}</d-text>` - and
  `\n` inside one produces a hard line break.

- Rich text: `<span>` inside `<text>` restyles a run (`color`, `fontFamily`,
  `fontSize`, `fontWeight`, `fontStyle`, `lineHeight`,
  `textDecoration="underline"`); spans nest and
  inherit inward from the `<text>`. Never lay a paragraph out word by word in
  a wrapping row to mix styles - one `<text>` with spans wraps as a whole.
  A span is content, not a box (no layout or `d-` form, no size, no
  bounding box; it takes its parent's form). A `<span>` takes
  pointer handlers (a link is a span, hit per line it spans), and any other
  element child of `<text>` (`<view>`, `<texture>`, `<path>`, ...) is an
  inline atom flowing with the words as one unbreakable box on the baseline;
  give it margins for spacing, since JSX trims the whitespace around it.

- Events: there is NO `onClick`/`onPress`. A "button" is a `<view>`/`<rect>`
  with `onPointerDown`. Handlers: onPointerDown/Up/Move/Enter/Leave, onWheel,
  onKeyDown/Up, onTextInput, onFocus/onBlur. Text entry: focus a node with an
  `onTextInput` handler. Focus alone never raises the on-screen keyboard: a
  tap on the focused node (or explicit startTextInput()) does, and never
  while a physical keyboard is attached; on keyboard-equipped platforms the
  session starts invisibly at focus so text flows immediately.
  `textInputHints` on the node configures the IME (type/capitalize/
  autocorrect) - identifier fields and terminals want
  `{ capitalize: "none", autocorrect: false }` (OS default auto-capitalizes). Key
  events start at the focused node and bubble leaf->root to the window (with
  nothing focused, the window alone), so `<window onKeyDown>` is the
  app-global shortcut point; `stopPropagation()` ends the walk. `focusable`
  declares focus-navigation candidacy (enumerate via getFocusables()).

- Gesture recognizers, shared by every package: `createPan` (single-pointer
  drag, axis-aware slop, per-event dx/dy) and `createTransform` (merged
  pan + pinch + rotate over the whole pointer set, Flutter-Scale style: streams
  `{ dx, dy, scale, rotation, x, y, pointers }` once per FRAME - positions
  update per event, but the cross-pointer measure waits for the
  `pointerFrame` batch terminator, when every pointer is the same age; one
  finger degrades to a plain pan, and `pointers` is how a consumer gives one-
  and two-finger translation different meanings - dx/dy alone cannot tell
  them apart).
  Spread the returned `.handlers` onto the receiving element. They
  arbitrate through the exported `arena` (ONE per app): a press claims its
  pointer provisionally, movement evidence steals and resolves it, the loser's
  `cancel()` retracts its feedback. Custom recognizers should join the arena
  rather than track pointers ad hoc, or they will double-handle against
  scrollers and pressables.

- Reactivity is SolidJS 2.0 (`@solidjs/signals`), NOT Solid 1.x. `createSignal`
  is as you expect, but `createEffect` takes the 2.0 two-function shape: a
  TRACKED compute that reads signals and returns a value, then an UNTRACKED
  effect that receives it - `createEffect(() => count(), (c) => ...)`. The 1.x
  single-callback form `createEffect(() => { ...count()... })` does NOT track
  here.
  Reading a signal/prop/store at the top level of a component body (not
  inside JSX, a `createMemo`, or an effect's compute phase) reads it
  untracked: it silently freezes at the initial value.
  Writing a signal or store from inside an owned scope (a component body, a
  `createMemo`, an effect's compute phase) throws
  `REACTIVE_WRITE_IN_OWNED_SCOPE` in dev; a loader called in the component
  body that sets state is the classic React / Solid 1.x reflex that hits
  this. Move the write into an event handler, an effect's apply phase,
  `onSettled`, or an `untrack` block; opt in narrowly with
  `createSignal(v, { ownedWrite: true })` for a signal that genuinely is
  internal state.
  Signal writes flush on a microtask: a handler that sets a signal and
  immediately reads it back gets the OLD value. Read it in an effect, or
  call `flush()` (from @solidjs/signals) to force it through.

- An element-valued prop (children, a content/icon slot) compiles to a getter
  that builds a fresh native subtree on EVERY read, and a subtree that is
  never inserted is never freed - native nodes are not garbage collected, so
  what is only wasted work in DOM Solid is a permanent memory leak here. Read
  such props exactly once, at the place they are mounted. To inspect
  children (a typeof probe, counting), resolve them first with the
  `children()` helper (re-exported from @solidrt/core) and probe the resolved
  memo - never `typeof props.children` on the raw prop.

- Animation is target-shaped first: declare `transition` on the element and
  write targets, and the runtime animates natively with no per-frame JS.
  Reach for per-frame work only for genuinely procedural motion:
  `onFrame((tick, frame) => {})` is the native hook (runtime-paced, returns a
  cleanup, auto-cleaned inside a reactive scope); `requestAnimationFrame`
  exists as a web-standard one-shot but is not the preferred driver. A JS
  tween loop or an animation library pushing interpolated values through
  signals is the single most expensive mistake available here - read
  agents/performance.md before writing either.
  Window state: onResize, onLayout, onWindowFocus, onWindowBlur exist, but
  prefer the reactive reads (`env`/`capabilities` above, or the accessors
  `windowSize()`, `safeArea()`, `displayScale()`, `windowFocused()`,
  `keyboardHeight()`, `pointerLocked()`) for reading layout and window
  state. For mouse look, `lockPointer(true)` enters relative mouse mode
  (cursor hidden and confined, positions freeze) and pointer events keep
  reporting motion through `movementX`/`movementY`.

- `createPortal` cannot mount during the app's initial render (it throws
  "no mount target"): gate portal content behind a signal that starts false
  and open it after startup - overlay content is opened, not born open.
  `createScroll` containers need an explicit main-axis size (a height, or
  flex inside a sized parent); with neither they resolve to 0 and the
  content silently vanishes (`maxHeight` alone does not size it). The
  runtime warns when this happens.

- Device/GPU access via subpath imports: @solidrt/core/camera, /microphone,
  /speech, /gpu. Image flow: `decodeImage(bytes)` ->
  `createTexture(data,w,h)` -> `<texture src={id} />`. Pixels are
  premultiplied alpha from decode onward (the GPU contract); `decodeImage(bytes,
  { alpha: "straight" })` keeps the file's color under transparent pixels for
  CPU work, and `encodeImage` converts back to straight for the file.

## Minimal app, core primitives only (verified to render)

```tsx
import { render } from "@solidrt/core"
import { createSignal } from "@solidjs/signals"

function App() {
  let [count, setCount] = createSignal(0)
  return (
    <window flexDirection="column" alignItems="center" justifyContent="center" gap={24}>
      <d-rect color="#0b0f17" />                  {/* window background */}
      <text color="#1f6feb" fontSize={48} fontWeight={800}>{count()}</text>
      <view onPointerDown={() => setCount((c) => c + 1)}
        padding={16} alignItems="center" justifyContent="center">
        <d-rect color="#1f6feb" radius={12} />    {/* button background, underlays the label */}
        <text color="#ffffff" fontSize={20}>increment</text>
      </view>
    </window>
  )
}

render(() => <App />)
```

Note the two `<d-rect>` underlays: a `<view>`/`<window>` does not paint, so a
background is a draw-primitive child placed behind the content.

To run and verify (incl. headless), see @solidrt/cli (its AGENTS.md).
