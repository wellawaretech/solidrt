# Elements

The two structural elements. Neither one paints: `window` is the root, `view`
is a box that lays out, clips, transforms and receives input. Everything
visible comes from the [drawing](/core/reference/drawing/) and
[text](/core/reference/text/) primitives inside them.

## window

One per app, the root of the tree. It composes
[layout](/core/reference/layout/) props, so the window is itself the outermost
flex container, and [pointer](/core/reference/input/) props, which is where
app-global key handling lives: key events always end their walk at the window
root.

{{ decl packages/core/src/types.d.ts WindowProps }}

The `shader` prop runs the finished frame through a GPU program as the last
step before the screen; see [shaders](/core/reference/shaders/).

## view

A `view` never paints. There is no `backgroundColor`: you put a `rect` behind
the content, and a shape with no geometry of its own fills the layout box it
sits in.

```tsx
<view padding={16} alignItems="center">
  <rect color="#1b2440" radius={12} />
  <text color="white">Boxed</text>
</view>
```

Its props split in two. `ViewOwnProps` is everything a view offers besides
layout, and it is what the detached `d-view` composes on its own:

{{ decl packages/core/src/types.d.ts ViewOwnProps }}

The laid-out `view` is that plus the layout props:

{{ decl packages/core/src/types.d.ts ViewProps }}

Three of those props are worth knowing before you need them:

- `designSize` fits a design-space coordinate system into the element's box,
  scaled uniformly and centered. Everything under the view - layout, paint,
  input - happens in design units, and the view itself sizes like a
  replaced element whose intrinsic size is the design size. It is the
  natural wrapper for `parseSvg` output, any `d-*` subtree authored in
  fixed units, or a whole laid-out panel that should scale rather than
  reflow.
- `repaintBoundary` retains the subtree's display list, and in its
  `"snapshot"` forms its rasterized pixels too. It is the lever for putting
  heavy static content next to content that changes every frame.
- `clipRadius` rounds the clip, and only does anything when `overflow` is
  non-visible.
