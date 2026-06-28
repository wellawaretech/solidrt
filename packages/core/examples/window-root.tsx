// The minimal valid app. render() is called once at the top level and the root
// element MUST be <window> - anything else throws. <window> takes layout props
// (it is a flex container), so children can be centered here directly.
import { render } from "@solidrt/core"

function App() {
  return (
    <window title="Hello" alignItems="center" justifyContent="center">
      <text fontSize={24} color="#222">Hello SolidRT</text>
    </window>
  )
}

render(() => <App />)