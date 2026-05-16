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
onRender(fn: (tick: number) => void): () => void
```

Registers a callback that fires on every rendered frame. `tick` is the current timestamp in milliseconds.

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
