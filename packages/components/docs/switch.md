# Switch

An on/off toggle: the track fills with `primary` when on and `surfaceAlt` when off; the thumb slides across. Controlled via `value`/`onChange`, or uncontrolled via `defaultValue`. Built on `Pressable`, so `disabled` takes no pointer events. `style` overrides the track colors and radius.

```jsx
import { Switch } from "@solidrt/components"
import { createSignal } from "@solidjs/signals"

function NotifyToggle() {
  let [on, setOn] = createSignal(true)
  return <Switch value={on()} onChange={setOn} />
}
```
