import { createSignal } from "@solidjs/signals"
import { onResize } from "@solidrt/core"

export function SafeArea(props: {
  top?: boolean | number
  bottom?: boolean | number
  left?: boolean | number
  right?: boolean | number
  relative?: boolean
  children?: any
}) {
  let [insets, setInsets] = createSignal({ top: 0, bottom: 0, left: 0, right: 0 })
  onResize(({ width, height, safeArea }) => {
    setInsets({
      top: safeArea.top,
      left: safeArea.left,
      bottom: height - safeArea.bottom,
      right: width - safeArea.right,
    })
  })
  let pad = (edge: "top" | "bottom" | "left" | "right") => {
    let defaultOn = edge === "top" || edge === "bottom"
    let p = props[edge] ?? defaultOn
    if (p === false) return 0
    if (p === true) return insets()[edge]
    return Math.max(insets()[edge], p as number)
  }
  return (
    <view
      flex={1}
      flexDirection="column"
      position={props.relative !== false ? "relative" : undefined}
      marginTop={pad("top")}
      marginBottom={pad("bottom")}
      marginLeft={pad("left")}
      marginRight={pad("right")}
    >
      {props.children}
    </view>
  )
}