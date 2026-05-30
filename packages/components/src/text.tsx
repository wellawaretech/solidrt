import { createMemo } from "@solidjs/signals"
import type { PointerProps } from "@solidrt/core"
import type { StyleProps, TextLayoutProps } from "./types"

export interface TextProps extends PointerProps {
  children?: any
  ref?: (node: { id: number }) => void
  layout?: TextLayoutProps
  style?: StyleProps
}

// Font keys live in layout (they affect measurement) but belong on the inner
// <text> node, not the box <view>. Strip them out before spreading layout.
const FONT_KEYS = [
  "fontFamily",
  "fontSize",
  "lineHeight",
  "fontStyle",
  "fontWeight",
  "textAlign",
  "maxLines",
]

export function Text(props: TextProps) {
  let box = createMemo(() => {
    let l = props.layout
    if (!l) return {}
    let out: Record<string, unknown> = {}
    for (let key in l) {
      if (!FONT_KEYS.includes(key)) out[key] = (l as Record<string, unknown>)[key]
    }
    return out
  })

  return (
    <view
      ref={props.ref}
      {...box()}
      x={props.style?.x}
      y={props.style?.y}
      scale={props.style?.scale}
      rotate={props.style?.rotate}
      onPointerEnter={props.onPointerEnter}
      onPointerLeave={props.onPointerLeave}
      onPointerDown={props.onPointerDown}
      onPointerUp={props.onPointerUp}
      onPointerMove={props.onPointerMove}
      onWheel={props.onWheel}
      onFocus={props.onFocus}
      onBlur={props.onBlur}
      onKeyDown={props.onKeyDown}
      onKeyUp={props.onKeyUp}
      onTextInput={props.onTextInput}
      pointerEvents={props.pointerEvents}
    >
      <text
        color={props.style?.color}
        fontFamily={props.layout?.fontFamily}
        fontSize={props.layout?.fontSize}
        lineHeight={props.layout?.lineHeight}
        fontStyle={props.layout?.fontStyle}
        fontWeight={props.layout?.fontWeight}
        textAlign={props.layout?.textAlign}
        maxLines={props.layout?.maxLines}
      >
        {props.children}
      </text>
    </view>
  )
}