# @solidrt/core examples

Single-concept SolidRT patterns. Each file is a complete, runnable app (ends in
`render(() => <App />)`) demonstrating exactly one thing - copy one and adapt it.
For the SolidJS 2.0 reactivity/control-flow model see `solid-js/CHEATSHEET.md`;
for the element/prop model see `@solidrt/core/AGENTS.md`.

## Host elements and layout
- `window-root.tsx` - the minimal app; the root must be `<window>`.
- `view-layout.tsx` - `<view>` as a flex container; containers do not paint.
- `view-viewbox.tsx` - `viewBox` on a `<view>`: author a scene once in fixed design units and let the view uniformly scale-and-center (letterbox) that space into its box. A pure fit transform - it never sizes the element (layout still does); children live in design space (the box they inherit IS the design size, so a bare `d-rect` fills it); pointer `localX`/`localY` arrive in design units. The fixed-aspect alternative to `windowSizeClass` reflow for diagrams, slides, dashboards, game boards.
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
- `responsive-grid.tsx` - one app across phone/tablet/desktop: `capabilities.windowSizeClass` (Material 3 breakpoints, a reactive getter) drives the column count and `windowSize()` sizes each card; reflows on resize. The reflow answer; for fixed-aspect content use `view-viewbox.tsx` instead.

## Overlays
- `portal.tsx` - `createPortal` relocating content to the window root to escape clipping.

## Images and GPU
- `image.tsx` - `createImage` (async value: fetch + decode + upload) read inside a `<Loading>` boundary and shown with `<texture>`.
- `inline-image.tsx` - bytes already in memory: `decodeImage` + `createTexture` (both synchronous) show an image with no `<Loading>` boundary. The sync counterpart to `image.tsx`.
- `gpu-shader.tsx` - a GLSL fragment shader rendered to a texture via `createShaderTexture`, animated by driving its own `uTime` uniform declaratively through the `<texture params={{...}}>` prop. Shows both source dialects side by side: without a `#version` line the runtime injects the `vUV`/`iResolution`/`fragColor` preamble (exactly what the runtime provides - app-driven uniforms are the source's own declarations), while a source starting with `#version 300 es` is compiled exactly as written and names its own uniforms - which is what lets a shader written elsewhere run unchanged.
- `gpu-texture-blend.tsx` - compositing two shader targets as stacked `<texture>` layers with `blendMode` ("plus" additive over a base pass), the alternative to a third shader that samples both. Click toggles against `"source-over"` to show why. The tree-level counterpart to blending within one draw (`gpu-particles.tsx`); without `blend: "add"` a target's own draw runs with GL blending disabled.
- `gpu-raw-program.tsx` - the raw shading layer: compileShader/linkProgram/createRenderPipeline/createShaderTarget, one vertex stage shared by two programs, with and without the standard header.
- `gpu-pipeline.tsx` - `createPipelineTexture`: a custom vertex+fragment pair drawing an interleaved vertex buffer (position and color attributes resolved by name), a depth-tested spinning cube whose `uTime` uniform is driven through `<texture params>` exactly like a fragment shader's.
- `gpu-particles.tsx` - an additive particle field: `topology: "points"` with `blend: "add"`, so overlapping gaussian splats (`gl_PointSize` from the vertex stage, `gl_PointCoord` falloff, premultiplied output) accumulate into a glowing volume instead of overwriting. The tints are typed vec3 uniforms filled from 3-number array params.
- `gpu-instancing.tsx` - instanced drawing: one 3-vertex triangle drawn hundreds of times via `instanceCount`, each instance placed and tinted on a phyllotaxis spiral from `gl_InstanceID` alone. `setDraw(id, { instanceCount })` merges into the draw range per frame (absent keys keep their values, like params), so the population breathes without touching the buffer.
- `gpu-draw-list.tsx` - `createDrawTarget`: one render target holding an ordered, mutable LIST of draws - two orbiting triangles from different programs sharing one depth buffer (the target owns the depth storage, each pipeline the test/write behavior), a third entry added and removed live via its stable `DrawId`, and `setDrawParams` as the per-object channel.
- `gpu-shared-params.tsx` - shared target state on a draw target: `setTargetParams`/`setTargetTextures` write values every entry reads - a ring of quads spins and color-cycles from ONE write per frame, and the shared sampler source swaps for the whole target at once. The two entries that seeded their own uTint/uMap show the precedence rule (an entry's own value beats the shared one), and mixed programs show partial coverage (a program that does not declare a shared name skips it). `createDrawTarget`'s positional `params` and `opts.textures` seed both channels before any entry exists.
- `window-shader.tsx` - the `shader` prop on `<window>`: the finished frame drawn through a raw-linked warp program before present, click to toggle between warp and identity.
- `window-shader-history.tsx` - the window shader's frame history: `previous` binds last frame as uPrevious, drawn as a one-frame motion echo behind an orbiting square; click toggles the echo term.

## Sound
- `sound.tsx` - `createSound`: decode a clip once from bytes (here a binary import), replay cheaply; `overlap` stacking vs single-voice, `playing()` signal, release on unmount. Points to `createSoundStream` for long tracks streamed from a path.

## Vector graphics
- `parse-svg.tsx` - `parseSvg` turns a whole SVG *document string* (not HTML/JSX children) into plain draw data mapped to `<d-path>` inside a `viewBox`-fitted view; per-shape hover highlighting shows the payoff (exact-outline hit testing, recolor without re-parse), plus a `currentColor` icon recolored via the `color` option. This is how to use existing icon libraries (Lucide, Heroicons, etc.) - hand their SVG source to `parseSvg`.

## Bundling assets
- `binary-import.tsx` - `import bytes from "./file" with { type: "binary" }` inlines a file's bytes into the bundle as a `Uint8Array` (the bytes are in memory, so `inline-image.tsx` displays them with the synchronous `decodeImage` + `createTexture` path).
- `text-import.tsx` - `with { type: "text" }`, the string counterpart: inlines a `.glsl` shader source (`wave.glsl`) into the bundle, available synchronously with no runtime read. Works on any extension; `.svg` needs no attribute and `.glsl`/`.vert`/`.frag` are declared as text modules already.