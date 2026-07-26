# Core API

All functions are imported from `@solidrt/core`:

```ts
import { render, onFrame, onResize } from "@solidrt/core"
```

The core package provides low-level primitives. For higher-level components built on top of these, see [Components](components.md).

## render

```ts
render(code: () => any): void
```

Entry point for a SolidRT application. Accepts a function that returns a `<window>` element. Must be called once at the top level.

```jsx
render(() => <App />)
```

## onFrame

```ts
onFrame(fn: (tick: number, frame: number) => void): () => void
```

Registers a callback that fires on every rendered frame. `tick` is the current timestamp in milliseconds. `frame` is the frame count since the application started.

Returns a cleanup function that stops the callback. When called inside a reactive scope (a component or `createEffect`), cleanup is automatic when the scope is destroyed.

```jsx
onFrame((tick, frame) => {
  setAngle(tick * 0.001)
})
```

## onResize

```ts
onResize(fn: ({ width, height, safeArea, displayScale }) => void): () => void
```

Registers a callback that fires whenever the window is resized. `safeArea` describes OS-reserved insets (e.g. notches, status bars). `displayScale` is the device pixel ratio.

Returns a cleanup function. When called inside a reactive scope, cleanup is automatic.

## onLayout

```ts
onLayout(fn: () => void): () => void
```

Registers a callback that fires after layout has been computed for the current frame, but before paint. Setting layout-affecting properties from this callback triggers one additional layout pass before painting.

Returns a cleanup function. When called inside a reactive scope, cleanup is automatic.

## onWindowFocus

```ts
onWindowFocus(fn: () => void): () => void
```

Registers a callback that fires when the OS window gains focus.

## onWindowBlur

```ts
onWindowBlur(fn: () => void): () => void
```

Registers a callback that fires when the OS window loses focus.

## onBack

```ts
onBack(fn: (e: { preventDefault: () => void }) => void): () => void
```

Registers a callback for the user's back intent: the Android back button or gesture, or the desktop dev chord (Ctrl/Cmd+Shift+Backspace). Call `e.preventDefault()` when back means in-app navigation right now (close a modal, go to the previous screen). When no handler prevents it, the default action runs: `exit()`.

Apps without a handler exit on back everywhere - the correct zero-effort default.

Handlers form a stack: the most recently registered one runs first, and the first to prevent ends the dispatch - handlers registered before it do not run, and neither does the default. Back is a pop, so the thing most recently put on screen answers for it: a dialog that opens over a screen registers after that screen and takes the next back press, then unregisters when it closes and hands the step back. Registration order tracks mount order, so this reads as innermost-first for handlers registered while mounting.

Each screen or overlay therefore owns exactly one step of the back stack and needs to know nothing about the others. The rule that makes that work: a handler that does not prevent must not act either, because the event is still travelling to whoever will handle it.

Returns a cleanup function. When called inside a reactive scope, cleanup is automatic.

```jsx
onBack((e) => {
  if (modalOpen()) {
    e.preventDefault()
    setModalOpen(false)
  }
})
```

There is no way to trap the user: a hung app is force-exited by the client, and the desktop window close button never enters JS.

## exit

```ts
exit(): void
```

Leaves the current app, unconditionally. What that means is the host's decision: back to the launcher in a dev client, quitting when standalone or at the launcher itself (on Android the client moves to the background instead of dying, matching the platform's back-at-root behavior).

This is the default action of an unprevented `back` event. Call it directly to exit programmatically - for example after intercepting back with `preventDefault()` to show an unsaved-changes dialog, then exiting on "discard". Without it, intercepting back would be a one-way trap.

## env

```ts
env: {
  windowSize, safeArea, displayScale, windowFocused, keyboardHeight,
  inputDevices, systemTheme, textScale, orientation, visibility,
  mouseSeen, touchSeen, keyboardSeen
}
```

Reactive facts about the environment the app is running in. Read properties inside a tracked scope (JSX, a memo, an effect) to re-run when they change; a top-level untracked read freezes at the initial value. Behavior decisions should normally go through `capabilities` and the policy layer - read `env` directly when the raw fact itself is needed.

### env.visibility

```ts
env.visibility: "visible" | "hidden"
```

Whether the app is on screen: `"hidden"` while backgrounded (Android) or minimized (desktop), `"visible"` again on return. The web's `visibilityState` vocabulary without the `document` machinery.

**This is the persistence moment.** There is no close event on any platform - Android gives apps no time when killing them, and the desktop window close never enters JS - so save state when `visibility` goes `"hidden"`:

```jsx
createEffect(
  () => env.visibility,
  (v) => {
    if (v === "hidden") saveState()
  },
)
```

The transition is delivered at background time (not on return), so the handler really runs before the OS may kill the process. While hidden, timers keep running but no frames are produced.

---

## Elements

Elements are the building blocks of a SolidRT UI. They map directly to native rendering commands via Lattice.

### `<window>`

The root element. Maps to a native OS window. Every application must have exactly one `<window>` as the root, passed to `render()`. Supports layout props.

### `<view>`

The primary container element. Supports layout, transform, and pointer event props. Use it to compose and structure the UI.

```jsx
<view width={200} height={100} background="#eee">
  <text>hello</text>
</view>
```

### `<text>`

Renders text. Children are the text content.

```jsx
<text color="#333" fontSize={16}>Hello</text>
```

### `<rect>`

Draws a rectangle. Supports paint and pointer event props. `w` and `h` set the size; `x` and `y` offset the origin. `radius` sets the corner radius - a single number applies to all corners, or pass `[top-left, top-right, bottom-right, bottom-left]`.

```jsx
<rect w={80} h={80} radius={8} fill="#0077ff" />
```

### `<oval>`

Draws an oval (ellipse) inscribed in the given bounds. Supports paint and pointer event props. `w` and `h` set the bounds; `x` and `y` offset the origin.

```jsx
<oval w={80} h={80} fill="#0077ff" />
```

### `<line>`

Draws a straight line between two points. Supports paint and pointer event props. Set `onLength` and `offLength` together to draw a dashed line.

```jsx
<line x1={0} y1={0} x2={100} y2={100} stroke="#0077ff" strokeWidth={2} />
<line x1={0} y1={0} x2={100} y2={0} onLength={8} offLength={4} stroke="#0077ff" strokeWidth={2} />
```

### `<path>`

Draws an SVG path. `d` is the SVG path data string. `x` and `y` offset the entire path. `fillRule` controls how overlapping subpaths are filled (`"nonZero"` by default, or `"evenOdd"`). Supports paint and pointer event props.

```jsx
<path d="M 10 10 L 90 10 L 50 80 Z" fill="#0077ff" />
```

### `<texture>`

Draws a GPU texture. `src` is a texture ID returned by `createTexture`. Supports `x`, `y`, `w`, `h`, source crop props (`srcX`, `srcY`, `srcW`, `srcH`), and `params` for shader parameters.

`fit` maps the pixels into the element box with CSS object-fit semantics: `"fill"` (default) stretches, `"cover"`/`"none"` crop, `"contain"`/`"scale-down"` letterbox, everything centered. Paint-only: the box (and hit testing) is unchanged.

For loading images from URLs or bytes without working directly with textures, use the [`<Image>`](components.md#image) component instead.

---

## GPU

Functions for loading image data onto the GPU.

```ts
import { decodeImage, createTexture } from "@solidrt/core"
```

### decodeImage

```ts
decodeImage(bytes: Uint8Array): DecodedImage
```

Decodes an image file (JPEG, PNG, etc.) from raw bytes. Returns an object with `data` (raw RGBA pixels), `width`, and `height`.

### createTexture

```ts
createTexture(data: Uint8Array, width: number, height: number): number
```

Uploads raw RGBA pixel data to the GPU and returns a texture ID. Pass the returned ID as the `src` prop on a `<texture>` element.

```js
let img = decodeImage(bytes)
let id = createTexture(img.data, img.width, img.height)
// <texture src={id} imageWidth={img.width} imageHeight={img.height} />
```

When called inside a reactive scope the texture is freed automatically once that owner is disposed. When called outside one (for example after an `await`, where the owner is no longer current), nothing is registered and you must free it yourself with `destroyTexture(id)`. The same rule applies to `createMutableTexture` and `createShader`. Every create helper also accepts `{ manual: true }` to skip the auto-free when you manage disposal yourself (for example a resource rebuilt on signal changes inside a long-lived component, where each rebuild would otherwise stack another cleanup on the component owner).

To change a texture's size without invalidating its id (for example a data texture backing a window-sized grid), use `resizeTexture(id, data, width, height)`; shader and pipeline targets resize with `setShaderSize(id, width, height)`. Both keep the id stable, so `<texture src>` references, sampler bindings, and the owner-scoped auto-free registered at creation all keep working - nothing needs re-creating.

`destroyTexture` is frame-safe: the runtime reclaims the id only once the render tree no longer references it. Destroying the old id in the same update that repoints `<texture src>` at its replacement is therefore always safe - whichever order the destroy and the swap land in, no frame paints blank. A destroyed id that stays mounted keeps drawing (and stays allocated) until it is unmounted or repointed.

For a shader whose spec is itself reactive, `createShaderMemo(() => ({ fragmentSrc, width, height, params?, textures? }))` returns an accessor for the current texture id and keeps the GPU resource in step: size changes route to `setShaderSize` and params changes to `setShaderParams` (id stays stable), while a new fragment source or new sampler bindings rebuild at a fresh id, update the accessor, and frame-safely destroy the old one.

A live shader's sampler2D inputs can also be retargeted directly with `setShaderTextures(id, { samplerName: textureId })` - the sampler analog of `setShaderParams`: the shader re-renders with its last-applied params against the new sources, without recompiling. Bindings not named keep their current source.

---

## Utilities

### measureText

```ts
measureText(text: string, options?: MeasureTextOptions): { width: number, height: number }
```

Synchronously measures the rendered size of a text string. Accepts the same font options as `<text>`: `fontFamily`, `fontSize`, `fontStyle`, `fontWeight`, `maxLines`.

### getBoundingBox

```ts
getBoundingBox(node: { id: number }): BoundingBox | null
```

Returns the window-relative bounding box of a node from the most recently computed layout, or `null` if the node has no layout or has not been laid out yet. This is a snapshot - call it from `onLayout` or an event handler to get values for the current frame.

### setFocus

```ts
setFocus(nodeId: number | null): void
```

Programmatically moves focus to the given node, or clears focus when passed `null`. Triggers `onFocus` and `onBlur` handlers and activates the on-screen keyboard if the newly focused node has an `onTextInput` handler.

### getFocusedNodeId

```ts
getFocusedNodeId(): number | null
```

Returns the id of the currently focused node, or `null` if nothing is focused.