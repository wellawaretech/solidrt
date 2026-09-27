# lightFrom

The light helpers for an app that moves the theme light. `theme.light.direction` is the way the light travels in screen space (x right, y down, z into the screen); `lightFrom(px, py)` turns a point on the unit disk - where the light comes from, a light pad's coordinates - into that direction, and `lightSource(light)` is its inverse, so a pad shows the current light. `lightAxis(direction)` is the quantized, interned axis every face derives its paint from: equal steps return the same object, which is why a dragged light costs a face a re-derivation only when the axis steps.

```jsx
import { lightFrom, setTheme, theme } from "@solidrt/components"

// A pad: the pointer's position over it, -1..1 per axis, becomes the light.
setTheme({ light: { ...theme.light, direction: lightFrom(px, py) } })
```

The light is data for event-rate changes (a drag, a preset switch, a time of day), not a per-frame animation: every lit face re-derives its paint on each move.
