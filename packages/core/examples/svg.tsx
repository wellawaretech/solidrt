// <svg> draws a whole SVG *document* passed as a STRING in the `src` prop. This
// is not HTML: there are no per-element <rect>/<circle>/<path> JSX children to
// nest. You hand it the SVG source text and usvg parses it (CSS, transforms,
// defs/use, gradients, clips) into a flat path tree the element renders. So an
// SVG is a value (a string you import, fetch, or inline), not markup you author
// inline with JSX.
//
// width/height set the drawn box (percentages work too); a multi-color document
// keeps each shape's own fill, while a monochrome icon using stroke/fill
// "currentColor" is recolored by the host `color` prop. For per-shape authored
// or animated vector art, compose <d-path> instead of this document layer.
//
// This is how you use an existing icon library (Lucide, Heroicons, Feather,
// Material, etc.): those ship SVG source, so import/inline the icon string and
// hand it to `src`. The `currentColor` convention they follow means the `color`
// prop recolors them, exactly as `currentColor` would in a browser.
import { render } from "@solidrt/core"

// Multi-color document: each shape carries its own fill, no host color needed.
const HOUSE = `
<svg viewBox="0 0 100 100" xmlns="http://www.w3.org/2000/svg">
  <rect x="20" y="45" width="60" height="45" fill="#457b9d"/>
  <path d="M10 50 L50 15 L90 50 Z" fill="#e63946"/>
  <rect x="42" y="62" width="16" height="28" fill="#f1faee"/>
  <circle cx="50" cy="35" r="6" fill="#ffd166"/>
</svg>`

// Monochrome icon (Lucide arrow-right) drawn with currentColor, recolored below.
const ARROW = `
<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"
     stroke-linecap="round" stroke-linejoin="round">
  <path d="M5 12h14"/>
  <path d="M12 5l7 7-7 7"/>
</svg>`

function App() {
  return (
    <window justifyContent="center" alignItems="center" flexDirection="row" gap={32}>
      <svg width={120} height={120} src={HOUSE} />
      <svg width={120} height={120} src={ARROW} color="#4f8cff" />
    </window>
  )
}

render(() => <App />)
