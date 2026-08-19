# QrCode

Renders a QR code for `data` out of primitives: same-color modules in a row collapse into one box, drawn on a light quiet-zone panel; the grid recomputes only when `data` or `level` changes. It paints black on white by default (not the theme) so it stays scannable through a theme switch; override `color`/`background` only if the contrast still holds. `moduleSize` (default 6) is pixels per module, `margin` (default 16) the quiet zone (keep it non-zero), `level` the error correction (`L`/`M`/`Q`/`H`, default `M`: higher tolerates more damage but caps data length sooner), `radius` the panel's corner radius.

```jsx
import { QrCode } from "@solidrt/components"

<QrCode data="https://solidjs.com" />
<QrCode data={ticket()} moduleSize={8} level="L" />
```
