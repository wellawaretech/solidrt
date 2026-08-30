# Layout

Layout is flexbox plus a line-based subset of CSS grid, over the whole element
tree. Prop names match CSS, so `flexDirection`, `alignItems`,
`justifyContent`, `gap`, `padding` and `width` mean what you expect.

These props exist on `window`, `view`, and the layout form of every drawing
and text primitive. They do not exist on the
[detached](/core/reference/detached/) forms, which own their geometry instead.

## Units

A bare number is pixels. A percentage is `pct(50)`, a branded value rather
than a parsed string, though the `"50%"` string form is accepted so pasted CSS
keeps working.

```tsx
<view flexDirection="row" gap={8} padding={16}>
  <view width={pct(50)} />
</view>
```

{{ decl packages/core/src/types.d.ts Dimension }}

{{ decl packages/core/src/types.d.ts LengthPercentage }}

## The box

{{ decl packages/core/src/types.d.ts LayoutProps }}

Two divergences from CSS are worth reading twice. `position` has `relative`
and `absolute` only, and an absolute element does not itself become a
containing block: it resolves against the nearest ancestor with
`position="relative"`. And `float` / `clear` are not page layout; they apply
to an element child of a `<text>`, where it becomes an inline atom the lines
wrap around.

## Flexbox

{{ decl packages/core/src/types.d.ts FlexboxProps }}

## Grid

{{ decl packages/core/src/types.d.ts GridProps }}

## Measuring

Positions come from the engine, not from arithmetic mirrored in app code.
`getBoundingBox(node)` returns the box from the most recently computed layout
as `{ x, y, width, height }`, transforms composed, relative to the nearest
`position="relative"` ancestor (falling back to the window), or `null` before
the first layout. `getBoundingBoxViewport(node)` is the same box always
window-relative, the space pointer `clientX`/`clientY` report in. Both are
snapshot reads, not reactive.

The moment to read them is `onLayout(fn)`: it fires after layout is computed
but before paint, so a measurement can still shape what the current frame
draws - the pattern for a connector between two laid-out cards, or an
annotation pinned to a bar the layout placed. Write the result to something
that does not affect layout (detached geometry, a `d` string, a transform)
and call `flush()` so the write lands before the display list is built;
writing a layout prop from `onLayout` costs an extra layout pass.

```tsx
onLayout(() => {
  let a = getBoundingBox(boxA)
  let b = getBoundingBox(boxB)
  if (!a || !b) return
  setD(`M ${a.x + a.width / 2} ${a.y + a.height / 2} L ${b.x} ${b.y}`)
  flush()
})
```
