import { render, safeArea } from "@solidrt/core"

let bottom = () => Math.max(safeArea().bottom, 10)
let right = () => Math.max(safeArea().right, 10)

function App() {
  return (
    <window display="grid" gridTemplateColumns="1fr 1fr" gridTemplateRows="1fr 1fr">
      <rect color="#822" />
      <rect color="#228" />
      <rect color="#282" />
      <rect color="#882" />
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
