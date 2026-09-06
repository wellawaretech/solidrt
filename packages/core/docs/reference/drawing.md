# Drawing

The primitives that put pixels on the screen: `rect`, `oval`, `line`, `path`
and `texture`. They share one paint vocabulary and differ only in the geometry
they draw.

In their layout form they take no geometry props at all. A shape derives its
geometry from the layout box it sits in, which is why a `rect` with no props
fills its parent. To place one freely, use the
[detached form](/core/reference/detached/) instead.

## Paint

Fill, stroke and blending, shared by every drawing primitive:

{{ decl packages/core/src/types.d.ts PaintProps }}

`color` takes a CSS color string or a gradient from `createLinearGradient` /
`createRadialGradient`. `drawStyle` picks fill, stroke, or both, and the
stroke props apply to the stroked part.

Where the stroke sits relative to the geometry differs by primitive, and it is
deliberate: a stroked `rect` or `oval` paints *inside* its box like a CSS
border, so nothing bleeds past the box for a clip to cut, while `line` and
`path` strokes stay centered on their geometry, where the geometry is the
stroke rather than a box.

## Dashing

A stroke's dash pattern, on every stroked primitive:

{{ decl packages/core/src/types.d.ts DashProps }}

The pattern is walked along the geometry itself: through a polyline's
vertices, along a path's curves (restarting at each subpath), and around a
rect's or oval's inset outline (inside the box, like the solid stroke).
`dashOffset` slides it - write it every frame for marching ants, or
transition it for a one-shot slide. A dashed stroke keeps its caps on every
dash, and a stroke-and-fill path dashes only the stroke.

`pathLength` declares what the geometry's length counts as, so the pattern
can be written in fractions of it: with `pathLength={1}`, `onLength={0.77}
offLength={1}` draws the first 77%, and transitioning `onLength` from 0 to
1 draws the geometry on (the SVG line-drawing trick, without having to know
the length).

## rect

{{ decl packages/core/src/types.d.ts RectProps }}

A dashed rect (a selection marquee, a drop zone) is the dash props on the
rect itself; see Dashing above. The pattern starts on the top edge after
the top-left corner and runs clockwise.

## oval

{{ decl packages/core/src/types.d.ts OvalProps }}

Dashes like a rect; the pattern starts at 3 o'clock and runs clockwise.

## line

{{ decl packages/core/src/types.d.ts LineProps }}

A laid-out `<line>` without `points` draws its box's top-left-to-bottom-right
diagonal, so in practice it is a rule: give it a thin box. Endpoints are a
detached-only concept, so arbitrary angles and connectors want `d-line`.
`points` makes either form a polyline - a flat `[x0, y0, x1, y1, ...]` array
(or a `Float32Array`), optionally `closed` - the numeric middle ground between
a segment and a path: animate it by writing a new array, nothing is parsed.
Curves want a path.

## path

{{ decl packages/core/src/types.d.ts PathProps }}

`d` is an SVG path string. Reach for `line` instead when the geometry is
numbers that animate (endpoints, or a polyline's `points`): a path animates
by rebuilding its `d` string, where a line moves one number or one array.
A path dashes like a line (see Dashing above), the pattern restarting at
each subpath; the dash props are paint-only writes, the `d` is not re-parsed.

## texture

A GPU texture: a decoded image, a camera frame, a video frame, or a shader
render target. `src` is a texture id, never a URL, which keeps one currency
for every pixel source in the engine.

{{ decl packages/core/src/types.d.ts TextureProps }}

## Logo

The SolidRT brand mark, the same seven-segment puzzle the scaffold's welcome
screen and the player draw, as a component: a square `view` of `size`
pixels with the segments as gradient-filled `d-path`s.

{{ decl packages/core/src/logo.tsx LogoProps }}

The default is static and requests no frames; `"once"` and `"loop"` drive a
staggered per-segment fade through `onFrame`, so the animated forms hold a
frame request only while they run (`"once"` releases it when the last
segment is in).
