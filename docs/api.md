# SolidRT API

All functions are imported from `@solidrt/core`:

```ts
import { render, onRender } from "@solidrt/core"
```

## render

```ts
render(code: () => any): void
```

Entry point for a SolidRT application. Accepts a function that returns a `<window>` element. Must be called once at the top level.

## onRender

```ts
onRender(fn: (tick, frame) => void): () => void
```

Registers a callback that fires on every rendered frame. `tick` is the current timestamp in milliseconds. `frame` is the frame count since the application started.

Returns a cleanup function that stops the callback. When called inside a reactive scope (a component or `createEffect`), cleanup is automatic when the scope is destroyed.


## onResize

```ts
onResize(fn: ({ width, height, safeArea }) => void): () => void
```

Registers a callback that fires whenever the window is resized. `displayScale` is the pixel density of the current display. `safeArea` describes OS-reserved insets (e.g. notches, status bars).

Returns a cleanup function that stops the callback. When called inside a reactive scope (a component or `createEffect`), cleanup is automatic when the scope is destroyed.

## Elements

### `<window>`

The root element. Maps to a native OS window. Every application must have exactly one `<window>` as the root, passed to `render()`. Supports layout props.

### `<view>`

The primary container element. Supports layout, transform, and pointer event props. Use it to compose and structure the UI.

### `<text>`

Renders text. Children are the text content.

### `<rect>`

Draws a rectangle. Supports paint and pointer event props. `r` sets the corner radius.

### `<texture>`

Draws a GPU texture. `src` is a texture ID returned by `createTexture`. Supports `x`, `y`, `imageWidth`, `imageHeight`, source crop props (`srcX`, `srcY`, `srcW`, `srcH`), and `params` for shader parameters.

## gpu

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
