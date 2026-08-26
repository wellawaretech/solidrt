// The SolidRT brand mark: seven puzzle segments authored in a 100x100 space,
// each filled with its own light-to-dark gradient. Static by default; the
// animated forms stagger a fade per segment, either once (fade in and hold)
// or in a loop (in, then out, repeat) like the scaffold's welcome screen.
import { For, Show, createSignal } from "solid-js"
import { createLinearGradient } from "./color"
import { onFrame } from "./window"

export interface LogoProps {
  // Rendered width and height in pixels; the mark is square. Default 100.
  size?: number
  // "none" draws the mark at full opacity and requests no frames (default).
  // "once" fades the segments in one after the other and then holds; "loop"
  // fades them in, then out, and repeats.
  animation?: "none" | "once" | "loop"
}

// Per segment: the delay of its fade in ms (its fade out follows at the same
// offset after the last fade in has completed), its gradient, its path.
const SEGMENTS = [
  { base: 0, light: "#3f5494", dark: "#162b6c", d: "M50.000 50.000 L28.330 50.000 C28.330 48.810 27.695 47.711 26.665 47.116 C25.635 46.521 24.365 46.521 23.335 47.116 C22.305 47.711 21.670 48.810 21.670 50.000 L0.000 50.000 L50.000 0.000 L50.000 9.170 C48.810 9.170 47.711 9.805 47.116 10.835 C46.521 11.865 46.521 13.135 47.116 14.165 C47.711 15.195 48.810 15.830 50.000 15.830 L50.000 25.000 L50.000 34.170 C48.810 34.170 47.711 34.805 47.116 35.835 C46.521 36.865 46.521 38.135 47.116 39.165 C47.711 40.195 48.810 40.830 50.000 40.830 L50.000 50.000 Z" },
  { base: 90, light: "#547ebf", dark: "#2b5696", d: "M50.000 50.000 L50.000 59.170 C48.810 59.170 47.711 59.805 47.116 60.835 C46.521 61.865 46.521 63.135 47.116 64.165 C47.711 65.195 48.810 65.830 50.000 65.830 L50.000 75.000 L50.000 84.170 C48.810 84.170 47.711 84.805 47.116 85.835 C46.521 86.865 46.521 88.135 47.116 89.165 C47.711 90.195 48.810 90.830 50.000 90.830 L50.000 100.000 L0.000 50.000 L21.670 50.000 C21.670 48.810 22.305 47.711 23.335 47.116 C24.365 46.521 25.635 46.521 26.665 47.116 C27.695 47.711 28.330 48.810 28.330 50.000 L50.000 50.000 Z" },
  { base: 180, light: "#7ea9ea", dark: "#5681c1", d: "M50.000 25.000 L50.000 15.830 C48.810 15.830 47.711 15.195 47.116 14.165 C46.521 13.135 46.521 11.865 47.116 10.835 C47.711 9.805 48.810 9.170 50.000 9.170 L50.000 0.000 L75.000 25.000 L65.830 25.000 C65.830 26.190 65.195 27.289 64.165 27.884 C63.135 28.479 61.865 28.479 60.835 27.884 C59.805 27.289 59.170 26.190 59.170 25.000 L50.000 25.000 Z" },
  { base: 270, light: "#547ebf", dark: "#2b5696", d: "M50.000 25.000 L59.170 25.000 C59.170 26.190 59.805 27.289 60.835 27.884 C61.865 28.479 63.135 28.479 64.165 27.884 C65.195 27.289 65.830 26.190 65.830 25.000 L75.000 25.000 L75.000 34.170 C73.810 34.170 72.711 34.805 72.116 35.835 C71.521 36.865 71.521 38.135 72.116 39.165 C72.711 40.195 73.810 40.830 75.000 40.830 L75.000 50.000 L65.830 50.000 C65.830 48.810 65.195 47.711 64.165 47.116 C63.135 46.521 61.865 46.521 60.835 47.116 C59.805 47.711 59.170 48.810 59.170 50.000 L50.000 50.000 L50.000 40.830 C48.810 40.830 47.711 40.195 47.116 39.165 C46.521 38.135 46.521 36.865 47.116 35.835 C47.711 34.805 48.810 34.170 50.000 34.170 L50.000 25.000 Z" },
  { base: 360, light: "#7ea9ea", dark: "#5681c1", d: "M50.000 50.000 L59.170 50.000 C59.170 48.810 59.805 47.711 60.835 47.116 C61.865 46.521 63.135 46.521 64.165 47.116 C65.195 47.711 65.830 48.810 65.830 50.000 L75.000 50.000 L64.855 60.145 C64.013 59.304 62.787 58.976 61.638 59.283 C60.489 59.591 59.591 60.489 59.283 61.638 C58.976 62.787 59.304 64.013 60.145 64.855 L50.000 75.000 L50.000 65.830 C48.810 65.830 47.711 65.195 47.116 64.165 C46.521 63.135 46.521 61.865 47.116 60.835 C47.711 59.805 48.810 59.170 50.000 59.170 L50.000 50.000 Z" },
  { base: 450, light: "#3f5494", dark: "#162b6c", d: "M75.000 50.000 L75.000 59.170 C73.810 59.170 72.711 59.805 72.116 60.835 C71.521 61.865 71.521 63.135 72.116 64.165 C72.711 65.195 73.810 65.830 75.000 65.830 L75.000 75.000 L50.000 100.000 L50.000 90.830 C48.810 90.830 47.711 90.195 47.116 89.165 C46.521 88.135 46.521 86.865 47.116 85.835 C47.711 84.805 48.810 84.170 50.000 84.170 L50.000 75.000 L60.145 64.855 C59.304 64.013 58.976 62.787 59.283 61.638 C59.591 60.489 60.489 59.591 61.638 59.283 C62.787 58.976 64.013 59.304 64.855 60.145 L75.000 50.000 Z" },
  { base: 540, light: "#7ea9ea", dark: "#5681c1", d: "M100.000 50.000 L75.000 75.000 L75.000 65.830 C73.810 65.830 72.711 65.195 72.116 64.165 C71.521 63.135 71.521 61.865 72.116 60.835 C72.711 59.805 73.810 59.170 75.000 59.170 L75.000 50.000 L75.000 40.830 C73.810 40.830 72.711 40.195 72.116 39.165 C71.521 38.135 71.521 36.865 72.116 35.835 C72.711 34.805 73.810 34.170 75.000 34.170 L75.000 25.000 L100.000 50.000 Z" },
]

const FADE = 360
const LAST = SEGMENTS[SEGMENTS.length - 1]!.base
// The whole mark is visible from IN_DONE; a loop cycle fades out after that.
const IN_DONE = LAST + FADE
const CYCLE = IN_DONE + LAST + FADE

let clamp = (x: number) => (x < 0 ? 0 : x > 1 ? 1 : x)
let ease = (t: number) => 1 - (1 - t) * (1 - t)
let byte = (x: number) => Math.round(clamp(x) * 255).toString(16).padStart(2, "0")

export function Logo(props: LogoProps) {
  let size = () => props.size ?? 100
  let mode = () => props.animation ?? "none"

  // Elapsed animation time; only advanced while an animated mode is mounted.
  let [clock, setClock] = createSignal(0)

  // The frame loop is mounted through the <Show> below so a mode change
  // remounts it: onFrame holds a standing frame request while registered.
  // "once" releases its request as soon as the last segment is in.
  let Animate = () => {
    let start = -1
    let stop = onFrame((tick) => {
      if (start < 0) start = tick
      let t = tick - start
      if (mode() === "loop") setClock(t % CYCLE)
      else if (t < IN_DONE) setClock(t)
      else {
        setClock(IN_DONE)
        stop()
      }
    })
    return null
  }

  let alpha = (seg: (typeof SEGMENTS)[number]) => {
    if (mode() === "none") return 1
    let t = clock()
    if (t < seg.base) return 0
    let fadeIn = clamp((t - seg.base) / FADE)
    if (mode() === "once") return ease(fadeIn)
    let end = IN_DONE + seg.base + FADE
    if (t >= end) return 0
    return ease(Math.min(fadeIn, clamp((end - t) / FADE)))
  }

  let fill = (seg: (typeof SEGMENTS)[number]) => {
    let a = byte(alpha(seg))
    return createLinearGradient(0, 0, 1, 1, [
      { offset: 0, color: seg.light + a },
      { offset: 1, color: seg.dark + a },
    ])
  }

  return (
    <view width={size()} height={size()} viewBox={[100, 100]}>
      <Show when={mode() !== "none"}>
        <Animate />
      </Show>
      <For each={SEGMENTS}>{(seg) => <d-path d={seg.d} color={fill(seg)} />}</For>
    </view>
  )
}
