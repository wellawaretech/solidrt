// Hello world, core only. The window ignores system insets (notches, status
// bars, home indicators), so the app pads its root view with the safe area
// on all four sides; safeArea() is reactive and updates on rotation.
import { render, safeArea } from "@solidrt/core"

function App() {
  return (
    <window>
      <view
        flex={1}
        paddingTop={safeArea().top}
        paddingBottom={safeArea().bottom}
        alignItems="center"
        justifyContent="center"
      >
        <text>Hello, World!</text>
      </view>
    </window>
  )
}

render(() => <App />)
