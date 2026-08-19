# Checkbox

A checkbox: filled with `primary` and a drawn checkmark when checked, an empty bordered box otherwise. Controlled via `checked`/`onChange`, or uncontrolled via `defaultChecked`. The mark is the `theme.icons.check` slot when a theme sets one. `style` overrides the box colors, border, and radius.

```jsx
import { Checkbox } from "@solidrt/components"

<Checkbox checked={agree()} onChange={setAgree} />
```
