import type {
  Color,
  Gradient,
  LayoutProps,
  Transition,
  TransitionEndEvent,
  TransitionPropName,
  TransitionProps as CoreTransitionProps,
} from "@solidrt/core"

// Native transitions through a component. A component is a root view plus
// the nodes it draws for `style` (a background d-rect, a stroke d-rect for
// the border), so a declaration names the STYLE properties - backgroundColor,
// borderColor, borderWidth, borderRadius - next to the view-level ones, and
// splitTransition hands each entry to the node that owns it. Core's paint
// names (color, radius, strokeWidth) are deliberately not accepted: on a
// component they would land on the root view, which has no paint, and be
// silently ignored.
//
// Controls whose paint is their own (Switch knob, Slider thumb, Checkbox
// mark, ...) take the view-level entries only for now; see
// okf/backlog/component-transitions-internal-paint.md.
export type TransitionViewProp =
  | "opacity"
  | "x"
  | "y"
  | "scale"
  | "scaleX"
  | "scaleY"
  | "rotate"
  | "rotateX"
  | "rotateY"
  | "originX"
  | "originY"
  | "perspective"
  | "clipRadius"
export type TransitionStyleProp = "backgroundColor" | "borderColor" | "borderWidth" | "borderRadius"
export type TransitionScrollProp = "scrollX" | "scrollY"

type TransitionEntries<P extends string> = {
  /** One spec for every property the component animates (no `from`/`exit` here, as in core). */
  all?: Transition
  /** Group stagger (ms) for descendant enter/exit animations; see core. */
  stagger?: number
} & { [K in P]?: Transition }

/** A component's transition declaration: view-level and style names, `all`, or a shorthand string. */
export type ComponentTransition<P extends string = TransitionViewProp | TransitionStyleProp> =
  | TransitionEntries<P>
  | string
  | null

export interface TransitionProps<P extends string = TransitionViewProp | TransitionStyleProp> {
  /**
   * Animate later writes of the listed properties instead of snapping, like
   * the core `transition` prop but in the component's own vocabulary:
   * `transition={{ opacity: "200ms ease-out", backgroundColor: { duration: 300 } }}`.
   * `all` or a bare string covers every property the component animates.
   */
  transition?: ComponentTransition<P>
  /** A transition settled; `property` is in the component's vocabulary (`backgroundColor`, not `color`). */
  onTransitionEnd?: (event: { property: P }) => void
}

/** The core transition declarations for a component's root view and its background/border rects. */
export type SplitTransition = {
  root: CoreTransitionProps["transition"]
  background: CoreTransitionProps["transition"]
  border: CoreTransitionProps["transition"]
}

const STYLE_TO_BACKGROUND: Record<string, TransitionPropName> = { backgroundColor: "color", borderRadius: "radius" }
const STYLE_TO_BORDER: Record<string, TransitionPropName> = {
  borderColor: "color",
  borderWidth: "strokeWidth",
  borderRadius: "radius",
}

/**
 * Map a component declaration onto the three nodes a styled component
 * draws. `all`, a shorthand string and `stagger` apply to every node; named
 * entries go to the node owning the property. An entry for a node that is
 * not mounted (no background set) simply never animates.
 */
export function splitTransition<P extends string>(t: ComponentTransition<P> | undefined): SplitTransition {
  if (t == null || typeof t === "string") return { root: t, background: t, border: t }
  let root: Record<string, unknown> = {}
  let background: Record<string, unknown> = {}
  let border: Record<string, unknown> = {}
  for (let [key, value] of Object.entries(t)) {
    if (key === "all" || key === "stagger") {
      root[key] = value
      background[key] = value
      border[key] = value
    } else if (key in STYLE_TO_BACKGROUND || key in STYLE_TO_BORDER) {
      if (key in STYLE_TO_BACKGROUND) background[STYLE_TO_BACKGROUND[key]!] = value
      if (key in STYLE_TO_BORDER) border[STYLE_TO_BORDER[key]!] = value
    } else {
      root[key] = value
    }
  }
  let pick = (o: Record<string, unknown>) => (Object.keys(o).length ? (o as CoreTransitionProps["transition"]) : undefined)
  return { root: pick(root), background: pick(background), border: pick(border) }
}

/** Report a settled core property in the component vocabulary for the node it settled on. */
export function transitionEndFor<P extends string>(
  node: "root" | "background" | "border",
  handler: ((event: { property: P }) => void) | undefined,
): ((event: TransitionEndEvent) => void) | undefined {
  if (!handler) return undefined
  return (e) => {
    let name: string = e.property
    if (node === "background") name = e.property === "color" ? "backgroundColor" : "borderRadius"
    if (node === "border")
      name = e.property === "color" ? "borderColor" : e.property === "strokeWidth" ? "borderWidth" : "borderRadius"
    handler({ property: name as P })
  }
}

// The split between layout and style follows one rule: layout properties feed
// into Taffy and changing them triggers a relayout; style properties are
// paint-only and can change without affecting layout.

// Paint-only props. None of these change the box Taffy computes. Borders are
// drawn as a stroke overlay (not part of the box model), and the transform is
// applied at paint time, so both live here.
export interface StyleProps {
  color?: Color | Gradient
  backgroundColor?: Color | Gradient
  borderColor?: Color | Gradient
  borderWidth?: number
  borderRadius?: number | [number, number, number, number]
  x?: number
  y?: number
  rotate?: number
  scale?: number
  // Per-axis scale; overrides `scale` on that axis (e.g. scaleX for a flip).
  scaleX?: number
  scaleY?: number
  // 3D rotation in radians about the horizontal (rotateX) / vertical (rotateY)
  // axis; reads as real depth only with `perspective` set.
  rotateX?: number
  rotateY?: number
  // Perspective viewing distance in pixels, enabling 3D depth for rotateX/Y.
  perspective?: number
  // Transform origin (the pivot for scale/rotate), in pixels from the box's
  // top-left. Defaults to the box center on each axis.
  originX?: number
  originY?: number
  // Corner radii for the clip applied when overflow is non-visible (hidden,
  // clip, scroll); a single number or [tl, tr, br, bl].
  clipRadius?: number | [number, number, number, number]
  opacity?: number
}

// One choice in an options list, shared by the single-choice controls
// (Select, SegmentedControl). Lives here so those components stay independent
// of each other: shared shapes go through this module, never a sibling import.
export interface Option {
  value: unknown
  label: string
}

// Text shaping affects measurement, so font props belong with layout rather
// than style. These end up on the inner <text> node, while the box layout
// fields go on the wrapping <view>.
export interface TextLayoutProps extends LayoutProps {
  fontFamily?: "sans" | "mono" | (string & {})
  fontSize?: number
  lineHeight?: number
  fontStyle?: "normal" | "italic"
  fontWeight?: 100 | 200 | 300 | 400 | 500 | 600 | 700 | 800 | 900
  textAlign?: "left" | "right" | "center" | "justify"
  maxLines?: number
}