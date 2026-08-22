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
