# Divider

A thin rule in the theme `border` color. It stretches across its container on the cross axis: full width inside a column, full height inside a row (pass `orientation="vertical"`). `thickness` defaults to 1px; add spacing with `layout` margins, and override the color via `style.backgroundColor`.

```jsx
import { Divider } from "@solidrt/components"

<Divider />
<Divider orientation="vertical" />
```
