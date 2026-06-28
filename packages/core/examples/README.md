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

## Window state
- `window-signals.tsx` - reactive `windowSize()` / `safeArea()` accessors (prefer over `onResize`).

## Overlays
- `portal.tsx` - `createPortal` relocating content to the window root to escape clipping.

## Images and GPU
- `image.tsx` - `createImage` to load (fetch + decode + upload) and display an image with `<texture>`.
- `gpu-shader.tsx` - a GLSL fragment shader rendered to a texture, animated via `setShaderParams`.