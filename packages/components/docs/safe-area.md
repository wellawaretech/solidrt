# SafeArea

Wraps its children in a view padded clear of system UI intrusions (status bars, home indicators, notches). Top and bottom insets are applied by default; pass `false` to opt out of an edge, or a number to apply the inset with that minimum padding.

```jsx
import { SafeArea } from "@solidrt/components"

<SafeArea top bottom>...</SafeArea>       // the default edges
<SafeArea bottom={false}>...</SafeArea>   // top only
<SafeArea top={16} bottom={16}>...</SafeArea>  // insets with a 16px minimum
<SafeArea top bottom left right>...</SafeArea> // all four edges
```
