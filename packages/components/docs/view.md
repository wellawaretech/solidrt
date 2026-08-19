# View

A general-purpose box. Spreads `layout` onto the underlying view, applies the transform from `style`, and draws a background and/or border when those style props are set. Takes all pointer event props.

```jsx
import { View } from "@solidrt/components"

<View
  layout={{ padding: 16, flexDirection: "column", gap: 8 }}
  style={{ backgroundColor: "#222", borderRadius: 8 }}
>
  {/* ... */}
</View>
```
