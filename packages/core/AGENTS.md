# @solidrt/core - agent notes

Dense, self-contained facts for writing a SolidRT app.
Full docs live in docs/ (and the website). When this conflicts with prose docs,
trust this file and the types in src/types.d.ts and jsx-runtime.d.ts.

SolidRT is a custom SolidJS renderer: it paints through a Rust runtime, not the
DOM. There is no HTML, no CSS cascade, no `className`.

## Setup

```sh
bun add @solidrt/core        # the renderer
bun add -d @solidrt/cli      # the `srt` tool (see its AGENTS.md)
```

`@solidrt/components` is a separate, optional package of higher-level components
(see its own AGENTS.md); core primitives alone are enough to build a full app.

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
2.0.0-beta.14); bun resolves them from peerDependencies.

## Element model (the parts that are easy to get wrong)

- `render(() => <App />)`. The returned root MUST be a `<window>` or it throws.
  Call render once, at the top level.

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

- Registered JSX intrinsics: `window`, `view`, `text`, `rect`, `oval`, `path`,
  `texture`, `audio`, plus the `d-` variants `d-view`, `d-rect`, `d-oval`,
  `d-path`, `d-texture`, `d-text`. NOTE: `<line>` has a LineProps type but is
  NOT a registered intrinsic - it will not typecheck.

- Plain vs `d-` variant: a plain element (e.g. `rect`) is `RectProps &
  LayoutProps` - it draws AND is a flex layout box. The `d-` variant (`d-rect`)
  is `RectProps` only - it draws but is not a layout box, so the parent
  positions it. Use `d-` for backgrounds/borders/overlays that should underlay
  or overlay siblings without taking a flex slot. The `d-` variants also sit
  OUTSIDE the layout engine (Taffy), so they are cheaper - prefer them for
  draw-only content that does not need to participate in layout.

- Events: there is NO `onClick`/`onPress`. A "button" is a `<view>`/`<rect>`
  with `onPointerDown`. Handlers: onPointerDown/Up/Move/Enter/Leave, onWheel,
  onKeyDown/Up, onTextInput, onFocus/onBlur. Text entry: focus a node with an
  `onTextInput` handler (setFocus activates the on-screen keyboard).

- Reactivity is plain SolidJS (`createSignal`, `createEffect`, ... from
  @solidjs/signals). Per-frame work: `onFrame((tick, frame) => {})` (returns a
  cleanup; auto-cleaned inside a reactive scope). Also onResize, onLayout,
  onWindowFocus, onWindowBlur.

- Device/GPU access via subpath imports: @solidrt/core/camera, /microphone,
  /speech, /gpu. Image flow: `decodeImage(bytes)` -> `createTexture(data,w,h)`
  -> `<texture src={id} />`.

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

To run and verify (incl. headless), see @solidrt/cli (its AGENTS.md). For
higher-level components, see @solidrt/components (its AGENTS.md).
