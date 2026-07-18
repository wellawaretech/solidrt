import { createSignal, onFrame, Show } from "@solidrt/core"
import type { LayoutProps } from "@solidrt/core"
import { theme } from "./theme"
import { policy } from "./policy"
import type { StyleProps } from "./types"

export interface SpinnerProps {
  // Overall diameter in pixels.
  size?: number
  // Arc stroke width in pixels.
  thickness?: number
  // Revolutions per second.
  speed?: number
  layout?: LayoutProps
  style?: StyleProps
}

const SIZE = 24
const THICKNESS = 3

// An indeterminate spinner: a 270-degree arc that rotates continuously, driven
// by core onFrame (so it participates in demand-driven rendering and stops when
// unmounted). Color comes from the theme; override via style.color.
export function Spinner(props: SpinnerProps) {
  let size = () => props.size ?? SIZE
  let thickness = () => props.thickness ?? THICKNESS
  let color = () => props.style?.color ?? theme.color.primary
  let speed = () => props.speed ?? 1

  // tick is in milliseconds (like performance.now()). The frame loop is mounted
  // through the <Show> below so the motion policy can unmount it: onFrame holds
  // a standing frame request while registered, so a check inside the callback
  // would still keep the renderer free-running. Under "none" the arc freezes;
  // "reduced" halves the spin.
  let [angle, setAngle] = createSignal(0)
  let Animate = () => {
    onFrame((tick) =>
      setAngle((tick / 1000) * speed() * (policy.motion === "reduced" ? 0.5 : 1) * Math.PI * 2),
    )
    return null
  }

  // A 270-degree arc starting at the top, sweeping clockwise. Coordinates are in
  // the parent box space, so the wrapping view is sized to match.
  let path = () => {
    let s = size()
    let r = (s - thickness()) / 2
    let c = s / 2
    return `M ${c} ${c - r} A ${r} ${r} 0 1 1 ${c - r} ${c}`
  }

  return (
    <view
      width={size()}
      height={size()}
      {...props.layout}
      rotate={angle()}
      x={props.style?.x}
      y={props.style?.y}
      opacity={props.style?.opacity}
    >
      <Show when={policy.motion !== "none"}>
        <Animate />
      </Show>
      <d-path
        d={path()}
        drawStyle="stroke"
        color={color()}
        strokeWidth={thickness()}
        strokeCap="round"
      />
    </view>
  )
}
