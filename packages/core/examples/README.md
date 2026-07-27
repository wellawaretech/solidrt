# @solidrt/core examples

Single-concept SolidRT patterns. Each file is a complete, runnable app (ends in
`render(() => <App />)`) demonstrating exactly one thing - copy one and adapt it.
For the SolidJS 2.0 reactivity/control-flow model see `solid-js/CHEATSHEET.md`;
for the element/prop model see `@solidrt/core/AGENTS.md`.

## Host elements and layout
- `window-root.tsx` - the minimal app; the root must be `<window>`.
- `view-layout.tsx` - `<view>` as a flex container; containers do not paint.
- `background-rect.tsx` - a `d-rect` filling its parent as a background.
- `detached-positioning.tsx` - the `d-` prefix: x/y placement, no reflow, detached-only children.
- `text-paint-styling.tsx` - the uniform `color` prop; `drawStyle="stroke"` vs fill.

## Frame and lifecycle
- `frame-animation.tsx` - `onFrame` driving a transform animation each frame.
- `on-layout-connect.tsx` - `onLayout` + `getBoundingBox` connecting laid-out boxes with a `d-path`.

## Pointer input
- `pointer-local-coords.tsx` - the three pointer coordinate frames (`clientX` window, `localX` the handling node's own frame, `parentX` its path-parent's frame - where the node's x/y live) and the transform-proof drag idiom: grab offset from `localX` at down, place with `parentX - offset` on moves. Exact inside rotated/scaled ancestors and when the pointer leaves the node mid-drag.

## Performance
- `repaint-boundary.tsx` - `repaintBoundary` on a `<view>` to keep static content from rebuilding while a neighbor animates: `{true}` retains the recorded draw list, `"snapshot"` also retains the rasterized pixels as a GPU texture (for raster-expensive, screen-aligned, static subtrees). `"snapshot-no-aa"` rasterizes without anti-aliasing: cheaper, fine for text and axis-aligned rects, hard-edged on vector content.

## Scrolling
- `scroll.tsx` - `createScroll`, the headless scroll primitive: it owns only the clamped offset (re-clamped on layout); you supply the viewport/content nodes via refs, apply the offset to `scrollX`/`scrollY`, and wire input (e.g. `onWheel`) to `scrollBy` yourself.

## Window state
- `window-signals.tsx` - reactive `windowSize()` / `safeArea()` accessors (prefer over `onResize`).
- `responsive-grid.tsx` - one app across phone/tablet/desktop: `capabilities.windowSizeClass` (Material 3 breakpoints, a reactive getter) drives the column count and `windowSize()` sizes each card; reflows on resize.

## Overlays
- `portal.tsx` - `createPortal` relocating content to the window root to escape clipping.

## Images and GPU
- `image.tsx` - `createImage` (async value: fetch + decode + upload) read inside a `<Loading>` boundary and shown with `<texture>`.
- `inline-image.tsx` - bytes already in memory: `decodeImage` + `createTexture` (both synchronous) show an image with no `<Loading>` boundary. The sync counterpart to `image.tsx`.
- `gpu-shader.tsx` - a GLSL fragment shader rendered to a texture, animated by driving its `iTime` uniform declaratively through the `<texture params={{...}}>` prop.

## Sound
- `sound.tsx` - `createSound`: decode a clip once from bytes (here a binary import), replay cheaply; `overlap` stacking vs single-voice, `playing()` signal, release on unmount. Points to `createSoundStream` for long tracks streamed from a path.

## Vector graphics
- `svg.tsx` - `<svg src={...}>` draws a whole SVG *document string* (not HTML/JSX children); multi-color fills vs a `currentColor` icon recolored by the `color` prop. This is how to use existing icon libraries (Lucide, Heroicons, etc.) - hand their SVG source to `src`.

## Bundling assets
- `binary-import.tsx` - `import bytes from "./file" with { type: "binary" }` inlines a file's bytes into the bundle as a `Uint8Array` (the bytes are in memory, so `inline-image.tsx` displays them with the synchronous `decodeImage` + `createTexture` path). `with { type: "text" }` works the same way for a string.