# Icon

A thin themed wrapper over the core `parseSvg` primitive. `src` is a whole SVG document as a string; the component parses it once (memoized), maps the draws to `<d-path>` in a square `designSize`-fitted box (`size`, default 24) and, for monochrome icons that stroke/fill with `currentColor`, recolors it via `color` (default the theme text color). It carries no icon set and no name registry, so any `currentColor` SVG works (Lucide, Feather, Heroicons, ...) and only the icons you import are bundled. Multi-color documents keep their own fills. For a non-square box, use `parseSvg` directly.

Icons are just SVG strings: import them as assets (`import House from "lucide-static/icons/house.svg"`, resolved to a string), pull them from a string export, or inline a literal.

```jsx
import { Icon } from "@solidrt/components"
import House from "lucide-static/icons/house.svg"

<Icon src={House} />
<Icon src={House} size={32} color={theme.color.primary} />
```
