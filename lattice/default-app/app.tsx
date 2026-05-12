import { render, onDevStatus, onRender } from "@solidrt/core"
import { createSignal } from "@solidjs/signals"
import { Logo, type LogoColors } from "./logo"

type Palette = LogoColors & { bg: string }

let palettes: Palette[] = [
  {
    bg: "rgba(217,217,217)",
    dark: "rgba(51,51,51)",
    mid: "rgba(102,102,102)",
    light: "rgba(153,153,153)",
  }, // grey
  {
    bg: "rgba(217,230,250)",
    dark: "rgba(26,51,128)",
    mid: "rgba(51,102,179)",
    light: "rgba(102,153,230)",
  }, // blue
  {
    bg: "rgba(250,224,224)",
    dark: "rgba(128,26,26)",
    mid: "rgba(179,51,51)",
    light: "rgba(230,102,102)",
  }, // red
  {
    bg: "rgba(224,245,230)",
    dark: "rgba(26,102,51)",
    mid: "rgba(51,153,77)",
    light: "rgba(102,204,128)",
  }, // green
  {
    bg: "rgba(250,240,217)",
    dark: "rgba(128,77,26)",
    mid: "rgba(179,128,51)",
    light: "rgba(230,179,77)",
  }, // amber
  {
    bg: "rgba(242,224,250)",
    dark: "rgba(102,26,128)",
    mid: "rgba(153,51,179)",
    light: "rgba(204,102,230)",
  }, // purple
  {
    bg: "rgba(224,245,250)",
    dark: "rgba(26,102,128)",
    mid: "rgba(51,153,179)",
    light: "rgba(102,204,230)",
  }, // teal
  {
    bg: "rgba(250,230,237)",
    dark: "rgba(153,51,102)",
    mid: "rgba(204,77,128)",
    light: "rgba(230,128,179)",
  }, // pink
  {
    bg: "rgba(250,245,224)",
    dark: "rgba(128,102,26)",
    mid: "rgba(179,153,51)",
    light: "rgba(230,204,102)",
  }, // gold
  {
    bg: "rgba(242,230,217)",
    dark: "rgba(38,38,38)",
    mid: "rgba(128,51,26)",
    light: "rgba(230,128,51)",
  }, // ember
]

function App() {
  let [index, setIndex] = createSignal(0)
  let [statusText, setStatusText] = createSignal("")

  setInterval(() => {
    setIndex((i) => (i + 1) % palettes.length)
  }, 1000)

  // onDevStatus((ev) => {
  //   if (ev.status === "scanning") setStatusText("Scanning for dev server...")
  //   else if (ev.status === "connecting") setStatusText(`Connecting to ${ev.address}...`)
  //   else if (ev.status === "connected") setStatusText(`Connected to ${ev.address}`)
  // })

  let palette = () => palettes[index()]!

  return (
    <window title="Solid-RT Demo" width={1600} height={400}>
      <view flexGrow={1} justifyContent="center" alignItems="center" flexDirection="column" gap={20}>
        {/* <d-rect color={palette().bg} /> */}
        <Logo width={500} dark={palette().dark} mid={palette().mid} light={palette().light} />
        <text fontSize={20} color={palette().mid} >
          {statusText()}
        </text>
      </view>
    </window>
  )
}

render(() => <App />)
