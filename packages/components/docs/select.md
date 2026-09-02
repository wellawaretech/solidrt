# Select

A single-choice picker whose presentation forks on the interaction policy: `desktop`/`hybrid` opens an anchored dropdown under the trigger (flipping above when there is no room), `touch` opens a bottom sheet over a scrim. Same contract either way: `options` is an `Option[]` (`{ value, label }`), controlled via `value`/`onChange` or uncontrolled via `defaultValue`; pressing outside closes without a change. `placeholder` shows in the trigger while nothing is selected. Both presentations fade in and out, and the trigger's chevron flips while open. The option list is not scrollable yet, so keep it short. The chevron is the `theme.icons.chevronDown` slot when a theme sets one.

```jsx
import { Select } from "@solidrt/components"

let options = [
  { value: "s", label: "Small" },
  { value: "m", label: "Medium" },
  { value: "l", label: "Large" },
]

<Select options={options} value={size()} onChange={setSize} placeholder="Size" />
```
