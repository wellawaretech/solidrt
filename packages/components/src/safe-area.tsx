import { safeArea } from "@solidrt/core"

export function SafeArea(props: {
  top?: boolean | number
  bottom?: boolean | number
  left?: boolean | number
  right?: boolean | number
  relative?: boolean
  children?: any
}) {
  let pad = (edge: "top" | "bottom" | "left" | "right") => {
    let defaultOn = edge === "top" || edge === "bottom"
    let p = props[edge] ?? defaultOn
    if (p === false) return 0
    if (p === true) return safeArea()[edge]
    return Math.max(safeArea()[edge], p as number)
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