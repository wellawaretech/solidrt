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

## rect

{{ decl packages/core/src/types.d.ts RectProps }}

## oval

{{ decl packages/core/src/types.d.ts OvalProps }}

## line

{{ decl packages/core/src/types.d.ts LineProps }}

A laid-out `<line>` without `points` draws its box's top-left-to-bottom-right
diagonal, so in practice it is a rule: give it a thin box. Endpoints are a
detached-only concept, so arbitrary angles and connectors want `d-line`.
`points` makes either form a polyline - a flat `[x0, y0, x1, y1, ...]` array
(or a `Float32Array`), optionally `closed` - the numeric middle ground between
a segment and a path: animate it by writing a new array, nothing is parsed.
Curves want a path. Dashing (`onLength`/`offLength`) runs continuously along
the whole polyline, and `dashOffset` slides the pattern: write it every frame
for marching ants.

## path

{{ decl packages/core/src/types.d.ts PathProps }}

`d` is an SVG path string. Reach for `line` instead when the geometry is
numbers that animate (endpoints, or a polyline's `points`): a path animates
by rebuilding its `d` string, where a line moves one number or one array.

## texture

A GPU texture: a decoded image, a camera frame, a video frame, or a shader
render target. `src` is a texture id, never a URL, which keeps one currency
for every pixel source in the engine.

{{ decl packages/core/src/types.d.ts TextureProps }}

## Logo

The SolidRT brand mark, the same seven-segment puzzle the scaffold's welcome
screen and the launcher draw, as a component: a square `view` of `size`
pixels with the segments as gradient-filled `d-path`s.

{{ decl packages/core/src/logo.tsx LogoProps }}

The default is static and requests no frames; `"once"` and `"loop"` drive a
staggered per-segment fade through `onFrame`, so the animated forms hold a
frame request only while they run (`"once"` releases it when the last
segment is in).
