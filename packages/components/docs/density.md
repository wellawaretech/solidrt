# Density

`<Density value="compact">` overrides the density policy for its subtree: every density-scaled metric below - `space()`, control sizes (Checkbox, Switch, Radio, Slider), `Item` and `Button` paddings - resolves this value instead of the global `policy.density`. Regions nest; the nearest wins. Use it to tighten a toolbar, a data table, or a sidebar without per-child props.

```jsx
import { Density, Item } from "@solidrt/components"

<Density value="dense">
  <For each={rows()}>{(r) => <Item label={r.name} />}</For>
</Density>
```

`densityScale()` is the reactive multiplier behind it (1 / 0.85 / 0.7 for comfortable/compact/dense): the nearest `<Density>` above the calling scope, falling back to `policy.density`. Call it during component setup or inside JSX/thunks when building custom density-aware components.
