# What you paint with (there is no CSS layer)

Read this before building any screen whose look matters: a background, a
decoration, an effect, a chart, anything you would have reached for CSS for.

Layout and props are half the model. There is no stylesheet: no filters, no
box-shadow, no keyframes, no canvas. The visual range a web app gets from CSS
comes from the tiers below instead, and reaching past tier 1 is ordinary
app-building here, not optimization - a screen built only from view
backgrounds and text is using a fraction of the runtime. Pick the tier the
CONTENT calls for, not the one that looks safest. The tiers compose per
ELEMENT, not per screen: a dashboard's cards, rows and labels stay tier-1
laid-out structure even when every card holds a tier-2 chart or a tier-3
shader. Escalate the element the content calls for, never the whole screen.

1. Laid-out elements - `<view>`/`<text>`, with `<rect>` (or a filling
   `<d-rect>` child) for background, border, radius. The structure of a
   screen, not its finish.
2. Vector art, detached from layout - `d-path`/`d-rect`/`d-oval`/`d-line`,
   whose `color` takes a gradient (createLinearGradient /
   createRadialGradient) and which honour `blendMode`, plus `parseSvg` to
   draw a whole SVG document as one subtree. Free-form shapes, decoration,
   diagrams, charts, anything positioned rather than flowed. Examples:
   parse-svg, detached-positioning, text-paint-styling.
3. GPU textures - `createShaderTexture` puts a fragment shader in a
   `<texture>` (moving gradients, noise, glow, dissolves, a background that
   is alive), `createPipelineTexture` draws geometry you generate yourself
   (particles, point clouds, splats), and the `shader` prop post-processes
   content that already exists: on a `<view>` it grades, warps or dissolves
   that subtree, on `<window>` the whole frame. Stack `<texture>` elements
   with `blendMode` to combine passes. Examples: gpu-shader, gpu-particles,
   gpu-pipeline, gpu-instancing, gpu-texture-blend, view-shader,
   window-shader.
4. 3D scenes - add `@solidrt/3d` (not a scaffold dependency): meshes,
   materials and a camera declared as Solid components, rendered into a
   texture that sits in the UI tree like any other element.

Tier 3 is cheaper than it looks. A shader costs one property write per frame
no matter how complex the effect, which is why the performance model
(agents/performance.md) reaches for it first rather than as a last resort.

## Web reflexes and what replaces them

- `color-mix()`, `oklch()`, `lab()` -> not in the color grammar (hex,
  `rgb()`, `hsl()`, `hwb()`, named colors only; the prop throws `Invalid
  color` otherwise). Mix in JS with `mixColors(a, b, t)` from @solidrt/core
  (oklab, so a ramp between two colors stays perceptually even) and pass
  the result; `withAlpha(color, a)` gives any color at an opacity (do not
  rebuild `rgba()` strings by hand from a hex); `brightness(color)` picks
  readable text over any fill.
- gradient background -> a gradient `color` on a `d-rect` (gradients are
  paint values, usable anywhere a color is)
- `filter: blur/grayscale/hue-rotate`, and any "make this look processed" ->
  a `shader` on the view (requires repaintBoundary="snapshot"), or on
  `<window>` for the whole frame
- `box-shadow` / `text-shadow` / glow -> no shadow prop exists: draw an
  offset `d-*` shape under the content, or a view shader with `outset` (the
  transparent margin an effect bleeds into)
- `backdrop-filter` -> no equivalent. A view shader sees only its own
  subtree's pixels, never what is behind it. Frost the whole frame with a
  window shader, or fake the layer with your own content
- CSS `transition` -> the `transition` prop: declare it on the element and
  keep writing targets; the runtime animates natively (performance rule 1)
- `@keyframes` -> a `transition` prop when the motion is target-shaped;
  `onFrame` writing a signal for genuinely procedural sequences; a `uTime`
  uniform when the animation is continuous and visual
- `<canvas>` 2D -> `d-*` primitives (rebuild one `d-path` string per frame
  rather than animating N elements)
- leader lines, connectors, an annotation or badge hugging an element, any
  drawing that must align with laid-out content -> measure, do not
  calculate: read the boxes with `getBoundingBox` inside `onLayout` and
  drive the `d-*` overlay from the result (core AGENTS.md "Measuring
  layout", examples/on-layout-connect.tsx). Mirroring the layout math in JS
  to predict where an element ends up is always wrong somewhere
- `<canvas>` WebGL, three.js -> `createPipelineTexture`, or `@solidrt/3d`
- video background, animated hero, particle field -> a shader texture; this
  is the case the runtime is built for
