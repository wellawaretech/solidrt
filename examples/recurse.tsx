import { onFrame, render, safeArea, windowSize } from "@solidrt/core"
import { createSignal } from "@solidjs/signals"

// Recursion test. 
// If you just want a visual effect like this, then you definitely should 
// not implement it like this!

const SCALE = 0.9
const DEPTH = 25

let rad = (n: number) => (n / 360) * Math.PI * 2
let [rotate, setRotate] = createSignal(0)
let size = () => Math.min(windowSize().width, windowSize().height)

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
      <d-rect color={color()} radius={size() / 4} />
      <Nested depth={props.depth - 1} />
    </view>
  )
}

let bottom = () => Math.max(safeArea().bottom, 10)
let right = () => Math.max(safeArea().right, 10)

function App() {
  let running = true
  let lastTick = 0
  let offset = 0
  let pauseStart = 0

  onFrame((tick: number) => {
    lastTick = tick

    if (!running) return
    setRotate(rad((tick - offset) / 200))
  })

  let pause = () => {
    if (!running) return
    running = false
    pauseStart = lastTick
  }

  let resume = () => {
    if (running) return
    offset += lastTick - pauseStart
    running = true
  }

  let toggle = () => (running ? pause() : resume())

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
        <d-rect color="#eee" radius={10} w={10} h={30} />
        <d-rect color="#eee" radius={10} x={15} w={10} h={30} />
      </view>

      {/* <Promo /> */}
    </window>
  )
}

render(() => <App />)