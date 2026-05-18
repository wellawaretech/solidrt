import { createSignal } from "@solidjs/signals"
import { onResize, render } from "@solidrt/core"

function App() {
  let [bottom, setBottom] = createSignal(0)
  let [right, setRight] = createSignal(0)

  onResize(({ width, height, safeArea }) => {
    console.log("onResize", width, height, safeArea)
    setBottom(10 + (height - safeArea.bottom))
    setRight(10 + (width - safeArea.right))
  })

  return (
    <window display="grid" gridTemplateColumns="1fr 1fr" gridTemplateRows="1fr 1fr">
      <rect width="100%" height="100%" color={0x882222ff} />
      <rect width="100%" height="100%" color={0x222288ff} />
      <rect width="100%" height="100%" color={0x228822ff} />
      <rect width="100%" height="100%" color={0x888822ff} />
      <rect
        r={50}
        position="absolute"
        width="50%"
        height="50%"
        top="25%"
        left="25%"
        color={0xff88007f}
      />
      <view position="absolute" bottom={bottom()} right={right()} justifyContent="center" alignItems="flex-end">
        <text fontSize={16} color={0xffffffff}>
          grid
        </text>
        <text fontSize={32} color={0xffffffff}>
          layout
        </text>
      </view>
    </window>
  )
}

render(() => <App />)
