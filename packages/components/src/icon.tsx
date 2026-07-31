import { createMemo, parseSvg, For } from "@solidrt/core"
import type { LayoutProps } from "@solidrt/core"
import { theme } from "./theme"

export interface IconProps {
  // An SVG document as a string: an imported `.svg` asset, a `lucide-static`
  // string export, or an inline template literal. Monochrome icons that stroke/
  // fill with `currentColor` (Lucide, Feather, Heroicons, ...) get recolored by
  // `color`; a multi-color document keeps its own fills.
  src: string
  // Rendered box in pixels, square. Defaults to 24 (the common icon grid).
  size?: number
  // Drives `currentColor` in the document. Defaults to the theme text color.
  color?: string
  layout?: LayoutProps
}

const SIZE = 24

// A themed wrapper over parseSvg: the document parsed once per src/color (a
// memo), its draws mapped to <d-path> in a square viewBox-fitted box colored
// from the theme by default. That is the only value it adds, so reach for
// parseSvg plus your own <view viewBox> when you need a non-square box or
// want no theme coupling. The plain repaintBoundary keeps the static subtree
// from re-recording alongside animating siblings; pointerEvents="all" makes
// the box one hit unit (and skips per-path outline tests) - an icon's shapes
// are never individual targets.
export function Icon(props: IconProps) {
  let size = () => props.size ?? SIZE
  let doc = createMemo(() => parseSvg(props.src, { color: props.color ?? theme.color.text }))

  return (
    <view
      repaintBoundary
      pointerEvents="all"
      width={size()}
      height={size()}
      viewBox={[doc().width, doc().height]}
      {...props.layout}
    >
      <For each={doc().draws}>{(draw) => <d-path {...draw} />}</For>
    </view>
  )
}
