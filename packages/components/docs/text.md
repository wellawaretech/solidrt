# Text

Themed text in a layout box. `variant` picks a typography role from the theme's type scale (`caption`/`label`/`body`/`title`/`heading`, default `body`); `color` picks a semantic theme color (`text`, `textMuted`, `primary`, `onPrimary`, `danger`, default `text`), with `muted` as sugar for `color="textMuted"`. Font fields go in `layout` (they affect measurement) and individually override the role; `style.color` still wins over `color`.

Font sizes carry `policy.textScale` (the OS text-size preference) and weights carry the low-DPI light-on-dark compensation; use the core `<text>` primitive for text that must not scale.

```jsx
import { Text } from "@solidrt/components"

<Text variant="title">Section</Text>
<Text muted layout={{ maxLines: 2 }}>Supporting copy that may wrap.</Text>
<Text layout={{ fontSize: 18 }} style={{ color: "#fff" }}>Custom</Text>
```

Note that `lineHeight` is a multiplier of `fontSize` (the theme uses 1.3-1.6), not a pixel value.
