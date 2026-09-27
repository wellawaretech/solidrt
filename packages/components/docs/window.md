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

The window takes the theme's finish (see Theming) whenever the theme has one; a theme without a finish declares no shader at all. `shader` is the window's own finish, a core window shader declaration forwarded as-is; set, it replaces the theme's, and `shader={null}` opts out of it. A window shader runs past the point snapshots read, so a finish is checked with `srt render`, not `get_snapshot`.
