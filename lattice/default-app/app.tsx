import { render } from "@solidrt/core"
import { Logo } from "./logo"

function App() {
  return (
    <window title="solidrt-go">
      <d-rect color="#111" />
      <view
        flexGrow={1}
        justifyContent="center"
        alignItems="center"
        flexDirection="column-reverse"
        gap={20}
      >
        <text color="lightgrey">waiting for connection...</text>
        <Logo />
      </view>
    </window>
  )
}

render(() => <App />)
