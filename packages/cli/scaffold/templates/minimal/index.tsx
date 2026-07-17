// Core-only template: built from @solidrt/core primitives.
import { render, safeArea } from "@solidrt/core"

function App() {
  return (
    <window>
      <view flex={1} paddingTop={safeArea().top} paddingBottom={safeArea().bottom}>
        <text>Hello, World!</text>
      </view>
    </window>
  )
}

render(() => <App />)
