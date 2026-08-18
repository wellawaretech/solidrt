# Transforms

Transform props apply after layout, at composite time. Nothing re-records and
nothing reflows, on a laid-out `view` as much as on a `d-view`, so these are
the props to animate.

They live on `ViewOwnProps`, which means both `view` and `d-view` have them;
see [elements](/core/reference/elements/).

{{ decl packages/core/src/types.d.ts TransformProps }}

## Origin

`originX` and `originY` are the point rotation and scale pivot around, split
per axis to match the engine's `x`/`y` prop convention.

{{ decl packages/core/src/types.d.ts OriginX }}

{{ decl packages/core/src/types.d.ts OriginY }}

A percentage origin tracks the layout size with no reactive wiring of your
own. On a `d-view` there is no box, so the origin defaults to the view's local
`(0,0)` - the origin its children's coordinates are authored against - and
`pct()` or keyword origins resolve against the inherited box, which is rarely
what you want. Pivot a `d-view` around its content by setting the origin in
pixels.

## Opacity

`opacity` is group opacity: the children composite together and then fade as a
whole, like CSS. It costs a compositing layer while below 1, unless the view
is a `repaintBoundary`, where it is hoisted to composite time for free.

To fade a single primitive, put the alpha in its `color` instead. Paint alpha
costs nothing.
