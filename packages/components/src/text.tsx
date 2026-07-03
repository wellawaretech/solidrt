import { createMemo } from "@solidjs/signals"
import type { PointerProps } from "@solidrt/core"
import type { StyleProps, TextLayoutProps } from "./types"
import { theme, type TextVariant } from "./theme"
import { policy } from "./policy"
import { typeWeight } from "./typography"

// Semantic text colors, resolved through the theme. Curated: only tokens that
// make sense as a text fill; style.color takes raw values for anything else.
export type TextColor = "text" | "textMuted" | "primary" | "onPrimary" | "danger"

export interface TextProps extends PointerProps {
  children?: any
  // Typography role from the theme's type scale; defaults to "body". Explicit
  // layout font props override the role's fields individually. fontSize
  // (role-derived or explicit) is multiplied by policy.textScale and
  // fontWeight carries the typeWeight low-DPI compensation; use the core
  // <text> primitive for text that must not scale.
  variant?: TextVariant
  // Semantic color from the theme; defaults to "text". style.color still wins.
  color?: TextColor
  // Sugar for color="textMuted".
  muted?: boolean
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
  let role = () => theme.text[props.variant ?? "body"]
  let size = () => (props.layout?.fontSize ?? role().size) * policy.textScale
  let color = () =>
    props.style?.color ?? theme.color[props.color ?? (props.muted ? "textMuted" : "text")]

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
        color={color()}
        fontFamily={props.layout?.fontFamily ?? theme.text.fontFamily}
        fontSize={size()}
        lineHeight={props.layout?.lineHeight ?? role().lineHeight}
        fontStyle={props.layout?.fontStyle}
        fontWeight={typeWeight(props.layout?.fontWeight ?? role().weight, size())}
        textAlign={props.layout?.textAlign}
        maxLines={props.layout?.maxLines}
      >
        {props.children}
      </text>
    </view>
  )
}