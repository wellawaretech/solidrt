# ContextMenu

Secondary actions on the wrapped content. The opening gesture follows the physical pointer: right-click for a mouse, long-press (500ms, cancelled by finger travel) for touch. The presentation forks on the interaction policy: `touch` gets a bottom sheet over a scrim, `desktop`/`hybrid` an anchored menu at the pointer that flips up near the bottom edge. Both presentations fade in and out. `items` is a `ContextMenuItem[]` (`{ label, onSelect?, disabled? }`); pressing outside closes without selecting.

```jsx
import { ContextMenu } from "@solidrt/components"

<ContextMenu
  items={[
    { label: "Rename", onSelect: rename },
    { label: "Delete", onSelect: remove },
    { label: "Share", disabled: true },
  ]}
>
  <Card>{file.name}</Card>
</ContextMenu>
```
