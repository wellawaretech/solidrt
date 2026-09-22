# ContextMenu

Secondary actions on the wrapped content. The opening gesture follows the physical pointer: right-click for a mouse, long-press (500 ms, cancelled by finger travel; core's long-press recognizer) for touch and pen. The long-press wins the finger at its timer: a pressable inside retracts and does not fire on the lift, and a scroll that started first keeps the finger. The presentation forks on the interaction policy: `touch` gets a bottom sheet over a scrim, `desktop`/`hybrid` an anchored menu at the pointer that flips up near the bottom edge. Both presentations fade in and out. `items` is a `ContextMenuItem[]` (`{ label, onSelect?, disabled? }`); pressing outside closes without selecting.

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
