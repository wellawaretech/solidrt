import { render, createLinearGradient, safeArea, Logo } from "@solidrt/core"

function App() {
  let backgroundColor = createLinearGradient(0, 0, 1, 1, [
    { offset: 0, color: "#080b16" },
    { offset: 1, color: "#1d2a52" },
  ])

  return (
    <window title="The Solid Runtime">
      <d-rect color={backgroundColor} />
      <view
        flex={1}
        gap={20}
        alignItems="center"
        justifyContent="center"
        paddingTop={safeArea().top}
        paddingBottom={safeArea().bottom}
      >
        <Logo size={300} animation="loop" />
        <text fontSize={40} color="#ccc">The Solid Runtime</text>
      </view>
    </window>
  )
}

render(() => <App />)
