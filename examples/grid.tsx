import { createSignal } from "@solidjs/signals"
import { onResize, render } from "@solidrt/core"

function App() {
  let [bottom, setBottom] = createSignal(0)
  let [right, setRight] = createSignal(0)

  onResize(({ width, height, safeArea }) => {
    setBottom(10 + (height - safeArea.bottom))
    setRight(10 + (width - safeArea.right))
  })

  return (
    <window display="grid" gridTemplateColumns="1fr 1fr" gridTemplateRows="1fr 1fr">
      <rect width="100%" height="100%" color="#822" />
      <rect width="100%" height="100%" color="#228" />
      <rect width="100%" height="100%" color="#282" />
      <rect width="100%" height="100%" color="#882" />
      <rect
        radius={[50, 0, 50, 0]}
        position="absolute"
        width="50%"
        height="50%"
        top="25%"
        left="25%"
        color="#f808"
      />
      <view
        position="absolute"
        bottom={bottom()}
        right={right()}
        justifyContent="center"
        alignItems="flex-end"
      >
        <text fontSize={48} color="#fff">
          grid
        </text>
        <text fontSize={64} color="#fff">
          layout
        </text>
      </view>
    </window>
  )
}

render(() => <App />)
