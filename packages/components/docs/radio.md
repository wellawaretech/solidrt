# RadioGroup / Radio

A single-selection pair: `RadioGroup` owns the selected value (controlled via `value`/`onChange`, or uncontrolled via `defaultValue`) and shares it with its `Radio` children; each `Radio` is a ring with an inner dot when selected. A string/number child of `Radio` renders as a themed label beside the ring; anything else as-is. `disabled` on the group disables every option, on a `Radio` just that one.

```jsx
import { RadioGroup, Radio } from "@solidrt/components"

<RadioGroup value={plan()} onChange={setPlan}>
  <Radio value="free">Free</Radio>
  <Radio value="pro">Pro</Radio>
  <Radio value="team">Team</Radio>
</RadioGroup>
```
