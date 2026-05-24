import { createSignal } from "@solidjs/signals"
import { onResize } from "@solidrt/core"

export function SafeArea(props: {
  edges?: ("top" | "bottom" | "left" | "right")[]
  minimum?: number
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
  let has = (e: string) => (props.edges ?? ["top", "bottom"]).includes(e as any)
  let pad = (edge: "top" | "bottom" | "left" | "right") =>
    has(edge) ? Math.max(insets()[edge], props.minimum ?? 0) : 0
  return (
    <view
      flex={1}
      flexDirection="column"
      marginTop={pad("top")}
      marginBottom={pad("bottom")}
      marginLeft={pad("left")}
      marginRight={pad("right")}
    >
      {props.children}
    </view>
  )
}