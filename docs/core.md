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

Draws a GPU texture. `src` is a texture ID returned by `createTexture`. Supports `x`, `y`, `imageWidth`, `imageHeight`, source crop props (`srcX`, `srcY`, `srcW`, `srcH`), and `params` for shader parameters.

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