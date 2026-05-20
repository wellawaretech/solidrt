import { render } from "@solidrt/core"
import { Logo } from "./logo"

function App() {
  return (
    <window title="Solid-RT Demo">
      <view
        flexGrow={1}
        justifyContent="center"
        alignItems="center"
        flexDirection="column"
        gap={20}
      >
        <d-rect color="#111" />
        <view>
          <d-rect color="#000" />
          <Logo />
        </view>
        <view>
          <text color="lightgrey">waiting for connection...</text>
        </view>
      </view>
    </window>
  )
}

render(() => <App />)
