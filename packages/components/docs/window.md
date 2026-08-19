# Window

The root surface of an app: renders a core `<window>`, so `render()` accepts it. Applies `layout` and `style.backgroundColor` only (a window cannot be transformed or bordered), plus `title` and `fullscreen`.

```jsx
import { Window } from "@solidrt/components"

function App() {
  return (
    <Window title="My App" style={{ backgroundColor: "#111" }}>
      {/* ... */}
    </Window>
  )
}
```
