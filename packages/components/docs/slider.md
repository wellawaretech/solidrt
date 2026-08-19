# Slider

A horizontal slider: the groove fills up to the thumb, and pressing or dragging the track sets the value from the pointer position. Controlled via `value`/`onChange` (fires while dragging), or uncontrolled via `defaultValue` (defaults to `min`). `min`/`max` default to 0/100; `step` snaps to an increment, omitted the value is continuous. The drag keeps tracking when the pointer drifts off the track, and an enclosing ScrollView never takes it over.

```jsx
import { Slider } from "@solidrt/components"

<Slider value={volume()} onChange={setVolume} min={0} max={100} step={1} layout={{ width: 180 }} />
```
