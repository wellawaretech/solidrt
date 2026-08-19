# SegmentedControl

A single-choice row of equal-width segments joined flush: only the control's outermost corners are rounded, hairline dividers separate the segments, and the active segment fills with the theme `primary`. Hovered segments tint with the theme `overlayHover` under non-touch interaction policies. `options` is an `Option[]`; controlled via `value`/`onChange`, or uncontrolled via `defaultValue`. Override the inactive fill via `style.backgroundColor` and the outer radius via `style.borderRadius`.

```jsx
import { SegmentedControl } from "@solidrt/components"

<SegmentedControl
  options={[{ value: "day", label: "Day" }, { value: "week", label: "Week" }]}
  value={range()}
  onChange={setRange}
/>
```
