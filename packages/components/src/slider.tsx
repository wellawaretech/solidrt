import { createSignal } from "@solidjs/signals"
import { getBoundingBox, setPointerCapture } from "@solidrt/core"
import type { LayoutProps, PointerEvent } from "@solidrt/core"
import { theme } from "./theme"
import type { StyleProps } from "./types"

export interface SliderProps {
  // Controlled value. If omitted, the slider is uncontrolled.
  value?: number
  defaultValue?: number
  min?: number
  max?: number
  // Snap increment. Omit for continuous.
  step?: number
  onChange?: (value: number) => void
  disabled?: boolean
  layout?: LayoutProps
  style?: StyleProps
}

const HEIGHT = 24
const GROOVE = 4
const THUMB = 20

let clamp = (x: number, lo: number, hi: number) => (x < lo ? lo : x > hi ? hi : x)

// A horizontal slider. The groove fills up to the thumb; dragging or pressing
// the track sets the value from the pointer x. No pointer capture, so a drag
// that leaves the track stops updating. Controlled via value/onChange, or
// uncontrolled via defaultValue.
export function Slider(props: SliderProps) {
  let min = () => props.min ?? 0
  let max = () => props.max ?? 100
  let [internal, setInternal] = createSignal(props.defaultValue ?? props.min ?? 0)
  let value = () => (props.value !== undefined ? props.value : internal())

  let track: { id: number } | undefined
  let dragging = false

  let pct = () => clamp(((value() - min()) / (max() - min())) * 100, 0, 100)

  let commit = (v: number) => {
    if (props.value === undefined) setInternal(v)
    props.onChange?.(v)
  }

  let setFromClientX = (clientX: number) => {
    if (!track) return
    let box = getBoundingBox(track)
    if (!box || box.width === 0) return
    let f = clamp((clientX - box.x) / box.width, 0, 1)
    let raw = min() + f * (max() - min())
    if (props.step) raw = Math.round(raw / props.step) * props.step
    commit(clamp(raw, min(), max()))
  }

  let handleDown = (e: PointerEvent) => {
    if (props.disabled) return
    // Claim the drag: stopPropagation keeps an ancestor scroller from starting a
    // scroll, and pointer capture routes moves/up here even when the pointer
    // drifts off the track.
    e.stopPropagation()
    if (track) setPointerCapture(track.id, e.pointerId)
    dragging = true
    setFromClientX(e.clientX)
  }
  let handleMove = (e: PointerEvent) => {
    if (!dragging) return
    setFromClientX(e.clientX)
  }
  let handleUp = () => {
    dragging = false
  }

  return (
    <view
      ref={(n: { id: number }) => (track = n)}
      flexDirection="row"
      alignItems="center"
      height={HEIGHT}
      width={200}
      {...props.layout}
      x={props.style?.x}
      y={props.style?.y}
      scale={props.style?.scale}
      rotate={props.style?.rotate}
      pointerEvents={props.disabled ? "none" : "auto"}
      onPointerDown={handleDown}
      onPointerMove={handleMove}
      onPointerUp={handleUp}
    >
      <view position="relative" flex={1} height={GROOVE}>
        <d-rect color={theme.color.surfaceAlt} radius={GROOVE / 2} />
        <view width={`${pct()}%`} height={GROOVE}>
          <d-rect color={theme.color.primary} radius={GROOVE / 2} />
        </view>
        <view position="absolute" left={`${pct()}%`} top={(GROOVE - THUMB) / 2} x={-THUMB / 2}>
          <d-oval w={THUMB} h={THUMB} color={theme.color.primary} />
        </view>
      </view>
    </view>
  )
}
