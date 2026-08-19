# ProgressBar

A horizontal progress bar: determinate when given a `value` in `[0, 1]` (the fill grows from the left), indeterminate when `value` is undefined (a short segment slides back and forth, driven by core `onFrame`). Track is `surfaceAlt`, fill is `primary`; override via `style.backgroundColor` (track) and `style.color` (fill).

```jsx
import { ProgressBar } from "@solidrt/components"

<ProgressBar value={0.4} />   // determinate
<ProgressBar />               // indeterminate
```
