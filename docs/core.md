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
onResize(fn: ({ width, height, safeArea }) => void): () => void
```

Registers a callback that fires whenever the window is resized. `safeArea` describes OS-reserved insets (e.g. notches, status bars).

Returns a cleanup function. When called inside a reactive scope, cleanup is automatic.

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

Draws a rectangle. Supports paint and pointer event props. `r` sets the corner radius.

```jsx
<rect width={80} height={80} r={8} fill="#0077ff" />
```

### `<oval>`

Draws an oval (ellipse) inscribed in the given bounds. Supports paint and pointer event props.

```jsx
<oval width={80} height={80} fill="#0077ff" />
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