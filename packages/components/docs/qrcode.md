# QrCode

Renders a QR code for `data` out of primitives: same-color modules in a row collapse into one box, drawn on a light quiet-zone panel; the grid recomputes only when `data` or `level` changes. It paints black on white by default (not the theme) so it stays scannable through a theme switch; override `color`/`background` only if the contrast still holds. `moduleSize` (default 6) is pixels per module, `margin` (default 16) the quiet zone (keep it non-zero), `level` the error correction (`L`/`M`/`Q`/`H`, default `M`: higher tolerates more damage but caps data length sooner), `radius` the panel's corner radius.

Its own entry, not the package root: the encoder it brings along cannot be shaken out of a bundle (it assigns at its top level), so as part of the index it would ride along in every app that imports anything from the package. Imported from `@solidrt/components/qrcode`, it reaches only the apps that show a QR code.

```jsx
import { QrCode } from "@solidrt/components/qrcode"

<QrCode data="https://solidjs.com" />
<QrCode data={ticket()} moduleSize={8} level="L" />
```
