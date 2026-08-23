# Core

`@solidrt/core` is the spine of SolidRT. It links SolidJS reactivity
to the native rendertree: an element vocabulary, layout, input, frames, and
the environment model for adapting to the device you are running on.

If you only learn one layer, learn this one. Extensions and tools are built
on it and are replaceable; Core is the part that changes least (SolidRT is
in alpha, so "least" is not "never").

## Elements

There is no DOM. JSX elements are native rendertree nodes, and the
vocabulary is deliberately small:

| Element | Purpose |
| --- | --- |
| `window` | The app window. One per app, the root of the tree. |
| `view` | Layout and input. Boxes, flex containers, hit targets. |
| `text`, `span` | A shaped paragraph, and a styled run inside it. |
| `rect`, `oval`, `line`, `path` | Painted shapes. |
| `texture` | A GPU texture: a decoded image, a camera frame, a shader target. |

Layout and paint are separate jobs, which is the one place the vocabulary
diverges sharply from HTML. A `view` never paints, so there is no
`backgroundColor`; you put a `rect` behind the content, and by default a
shape fills the layout box it sits in:

```tsx
<view padding={16} alignItems="center">
  <rect color="#1b2440" radius={12} />
  <text color="white">Boxed</text>
</view>
```

Each painting element also has a detached twin: `d-view`, `d-rect`,
`d-path`, `d-text`, and so on. Detached elements are positioned by their
parent's coordinate system rather than by layout, so changing one costs no
reflow. Use them for anything that moves at animation frequency.

## Reactivity

Props are reactive values, not snapshots. A signal read inside JSX
subscribes exactly one native property to exactly one signal, and an update
writes that property directly. Nothing re-renders, and there is no virtual
DOM to diff:

```tsx
let [x, setX] = createSignal(0)

<d-rect x={x()} w={40} h={40} color="tomato" />
```

The reactive and control-flow vocabulary comes from SolidJS 2.0 and is
re-exported from `@solidrt/core`, so an app imports everything from one
place: `createSignal`, `createMemo`, `createEffect`, `createStore`,
`onCleanup`, and the control-flow components `For`, `Show`, `Switch`,
`Match`, `Loading`, `Errored`.

Because props are values rather than accessors, the usual Solid rules apply:
do not destructure props, and read reactive values inside the expression
that uses them.

## Layout

Layout is flexbox, plus a line-based subset of CSS grid, over the whole
element tree. Prop names match CSS: `flexDirection`, `alignItems`,
`justifyContent`, `gap`, `padding`, `width`, `position`, `top`.

Units are simpler than CSS. A bare number is pixels; a percentage is
`pct(50)`, a branded value rather than a parsed string:

```tsx
<view flexDirection="row" gap={8} padding={16}>
  <view width={pct(50)} />
</view>
```

`position` has `relative` and `absolute` only, and an absolute element does
not itself become a containing block: it resolves against the nearest
ancestor with `position="relative"`.

## Input

Pointer, wheel, and key events are props on any element:
`onPointerDown`, `onPointerMove`, `onPointerUp`, `onPointerEnter`,
`onPointerLeave`, `onWheel`, `onKeyDown`, `onKeyUp`. Events travel from the
hit leaf up to the root, and `stopPropagation()` ends the walk. Key events
bubble the same way, starting at the focused node - or at the window root
when nothing is focused, so `onKeyDown` on the window is where app-global
shortcuts live.

Text entry goes to the focused node's `onTextInput`. Focusing a field never
raises the on-screen keyboard by itself - a tap on the field (or an explicit
`startTextInput()`) does, and never while a physical keyboard is attached.

Coordinates are logical points, so a handler reads the same numbers on a
high-density phone screen as on a desktop monitor.

## Frames and animation

`onFrame(callback)` runs before every painted frame with the frame time in
ms, the frame count, and the display refresh rate; it returns a disposer and
cleans itself up with the reactive scope it was called in. Rendering is
demand-driven: the runtime does not spin a render loop when nothing changed,
so an idle app is genuinely idle.

```tsx
let [t, setT] = createSignal(0)
onFrame((tick) => setT(tick))

<d-view rotate={t() / 1000}>...</d-view>
```

## Environment and devices

`env` and `capabilities` describe where the app is running: `env` is what
is observed (system theme, text scale, orientation, visibility, connected
input devices), `capabilities` what follows from it for behavior (hover,
touch, precise pointer, keyboard navigation, window size class). Which
runtime features exist on this build is `Flux.capabilities`, by name, never
by guessing from the OS.

Window-shaped values are reactive too: `windowSize()`, `safeArea()`,
`displayScale()`, `keyboardHeight()`, `windowFocused()`.

Device access follows the same reactive shape, as `create*` primitives
imported from Core subpaths rather than an imperative API:

```tsx
import { createCamera } from "@solidrt/core/camera"

let camera = createCamera()

<texture src={camera.texture()} fit="cover" />
```

The same pattern covers `@solidrt/core/microphone`, `/sound`,
`/speech-recognition`, `/text-input`, `/image`, `/color`, and `/gpu`.

## Reference

The [reference](/core/reference/) covers the API by subject: the element
vocabulary, drawing, text, detached elements, layout, transforms, input,
shaders, the GPU module, and the shared types. It shows the shipped declarations themselves,
so it says exactly what your editor says.
