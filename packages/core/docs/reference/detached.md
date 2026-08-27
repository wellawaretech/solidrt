# Detached elements

Every painting element has a detached twin: `d-view`, `d-rect`, `d-oval`,
`d-line`, `d-path`, `d-text`, `d-texture`. A detached element has no layout
box. It is not part of the layout at all, and instead owns its geometry in
paint-space pixels, positioned in the coordinate system of its parent.

That is the whole idea, and the reason to reach for one:

```tsx
let [x, setX] = createSignal(0)
onFrame((now) => setX(Math.sin(now / 500) * 100))

<view width={pct(100)} height={200}>
  <d-rect x={x()} y={20} w={40} h={40} color="tomato" />
</view>
```

Moving that rect writes one number to one native property. Nothing reflows,
because there is nothing in the layout to reflow. The same content as a
laid-out `<rect>` with `left={x()}` would put the layout engine on the path of
every frame.

**Reach for the detached form first for anything that moves at animation
frequency**, and for content authored in fixed design units - a chart, a
diagram, an svg drawing, a particle field. Reach for the layout form when you
want the element to participate in a flex or grid arrangement, which is most
static UI.

## What changes

A detached element composes exactly the same paint, text and pointer props as
its layout twin. One thing is swapped: the [layout](/core/reference/layout/)
props are replaced by geometry props.

| | Layout form | Detached form |
| --- | --- | --- |
| Position | flex or grid placement, `top`/`left`, margins | `x`, `y` in the parent's coordinates |
| Size | `width`/`height` and the box it is given | `w`, `h`, defaulting to the inherited box |
| Costs a reflow | yes | no |

`d-view` is the container case: it composes `ViewOwnProps` directly (see
[elements](/core/reference/elements/)), so it still transforms, clips and
takes input, but a layout prop on it is dropped with a one-time warning
rather than silently ignored.

`<span>` has no detached form, since an inline run never has a box of its own
to detach from.

## Geometry

Detached geometry is paint-space pixels and never affects layout.

{{ decl packages/core/src/types.d.ts PositionProps }}

Most primitives add a size, defaulting to the box they inherit from their
ancestor, so a `d-rect` with only `x`/`y` still has something to draw:

{{ decl packages/core/src/types.d.ts GeometryProps }}

`d-oval` measures its box rather than a radius:

{{ decl packages/core/src/types.d.ts OvalGeometryProps }}

For `d-text`, width is the shaping width and height is reported bounds only,
since a paragraph's height always falls out of the text itself:

{{ decl packages/core/src/types.d.ts TextGeometryProps }}

`d-path` takes only a position, since its size is whatever its `d` string
draws. `d-line` has no width and height either: its geometry is its two
endpoints (or, as a polyline, its `points`), which is what makes it the
primitive to reach for when the geometry moves:

{{ decl packages/core/src/types.d.ts LineGeometryProps }}

## The other x and y

`x` and `y` also appear on [transform](/core/reference/transforms/) props,
where they mean a post-layout subtree translation and exist on laid-out views
too. The two never collide on one element: on a `d-*` primitive `x`/`y` is
where the geometry is drawn, and on a `view` or `d-view` it is a translation
applied to the whole subtree. Both are cheap; neither triggers layout.
