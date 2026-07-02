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

// A themed wrapper over the core <svg> document primitive: a square box sized to
// `size` and colored from the theme by default. This is the only value it adds
// over `<svg src>` directly, so reach for the primitive when you need a
// non-square box or want no theme coupling.
export function Icon(props: IconProps) {
  let size = () => props.size ?? SIZE

  return (
    <svg
      width={size()}
      height={size()}
      src={props.src}
      color={props.color ?? theme.color.text}
      {...props.layout}
    />
  )
}
