import { onRender, onResize, render } from "@solidrt/core"
import { createSignal } from "@solidjs/signals"

// Recursion test. 
// If you just want a visual effect like this, then you definitely should 
// not implement it like this!

const SCALE = 0.9
const DEPTH = 25

let rad = (n: number) => (n / 360) * Math.PI * 2
let [rotate, setRotate] = createSignal(0)
let [size, setSize] = createSignal(0)

function Nested(props: { depth: number }) {
  if (props.depth === 0) return

  let d = (1 - (0.95 * props.depth) / DEPTH) * 255
  let defaultColor = `rgb(${d},0,0)`
  let highlightColor = `rgb(${d},0,${d})`
  let [color, setColor] = createSignal(defaultColor)
  return (
    <view
      rotate={rotate()}
      scale={SCALE}
      width={size()}
      height={size()}
      onPointerEnter={() => setColor(highlightColor)}
      onPointerLeave={() => setColor(defaultColor)}
    >
      <d-rect color={color()} r={size() / 4} />
      <Nested depth={props.depth - 1} />
    </view>
  )
}

let [bottom, setBottom] = createSignal(0)
let [right, setRight] = createSignal(0)

onResize(({ width, height, safeArea }) => {
  setSize(Math.min(width, height))
  setBottom(10 + (height - safeArea.bottom))
  setRight(10 + (width - safeArea.right))
})

function App() {
  let running = true
  let lastTick = 0
  let offset = 0

  onRender((tick: number) => {
    lastTick = tick

    if (!running) return
    setRotate(rad((tick - offset) / 200))
  })

  let toggle = () => {
    running = !running

    if (!running) offset = lastTick - offset
    else offset = lastTick - offset
  }


  return (
    <window
      title="Recursion"
      justifyContent="center"
      alignItems="center"
    >
      <Nested depth={DEPTH} />

      <view
        position="absolute"
        bottom={bottom()}
        right={right()}
        width={25}
        height={30}
        onPointerDown={toggle}
      >
        <d-rect color="#eee" r={10} w={10} h={30} />
        <d-rect color="#eee" r={10} x={15} w={10} h={30} />
      </view>

      {/* <Promo /> */}
    </window>
  )
}

render(() => <App />)