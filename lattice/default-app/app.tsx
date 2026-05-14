import { render } from "@solidrt/core"
import { createSignal } from "@solidjs/signals"

function App() {
  let [count, setCount] = createSignal(0)
  setInterval(() => setCount((c) => c + 1), 1000)

  return (
    <window>
        <text fontSize={100} color={0x007f7fff}>Hello, World {count()}</text>
    </window>
  )
}

render(() => <App />)
