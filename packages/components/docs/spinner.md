# Spinner

An indeterminate spinner: a 270-degree arc that rotates continuously, driven by core `onFrame`, so it participates in demand-driven rendering and stops when unmounted. `size` (diameter, default 24), `thickness` (default 3), `speed` (revolutions per second, default 1). Color comes from the theme `primary`; override via `style.color`.

```jsx
import { Spinner } from "@solidrt/components"

<Spinner />
<Spinner size={32} thickness={4} speed={1.5} />
```
