# Select

A single-choice picker whose presentation forks on the interaction policy: `desktop`/`hybrid` opens an anchored dropdown under the trigger (above it when only that side has the room), `touch` opens a bottom sheet over a scrim. Same contract either way: `options` is an `Option[]` (`{ value, label }`), controlled via `value`/`onChange` or uncontrolled via `defaultValue`; pressing outside, Escape and the platform's back close without a change. `placeholder` shows in the trigger while nothing is selected. A list taller than the room it has scrolls, opening with the selected option in view, and while open the options are the focus navigation's scope (as a Modal's content is). Both presentations fade in and out, the dropdown glows with the theme's overlay glow, and the trigger's chevron flips while open. The chevron is the `theme.icons.chevronDown` slot when a theme sets one.

```jsx
import { Select } from "@solidrt/components"

let options = [
  { value: "s", label: "Small" },
  { value: "m", label: "Medium" },
  { value: "l", label: "Large" },
]

<Select options={options} value={size()} onChange={setSize} placeholder="Size" />
```
