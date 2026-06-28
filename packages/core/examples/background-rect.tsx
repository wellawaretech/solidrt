// Containers do not paint, so a background is just a draw primitive placed behind
// the content. A d- primitive with no x/y/w/h fills its parent, which is exactly
// what you want for a background. Put it FIRST so siblings paint on top of it.
import { render } from "@solidrt/core"

function App() {
  return (
    <window alignItems="center" justifyContent="center">
      <d-rect color="#1a3380" />
      <text fontSize={28} color="#ffffff">Content on a full-bleed background</text>
    </window>
  )
}

render(() => <App />)