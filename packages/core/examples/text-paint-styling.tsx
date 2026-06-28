// Painting is uniform across primitives: the `color` prop sets the paint (a CSS
// color string or a gradient). There is no fill / stroke / background prop. To
// outline instead of fill, set drawStyle="stroke" and strokeWidth; "stroke-and-
// fill" does both. The same color prop styles <text>, alongside fontSize /
// fontWeight / fontFamily.
import { render } from "@solidrt/core"

function App() {
  return (
    <window flexDirection="column" gap={16} padding={24} alignItems="center" justifyContent="center">
      <text fontSize={28} fontWeight={700} color="#1a3380">Filled text</text>
      <rect width={160} height={64} radius={12} color="#3366b3" />
      <oval width={120} height={64} drawStyle="stroke" strokeWidth={4} color="#1a3380" />
    </window>
  )
}

render(() => <App />)