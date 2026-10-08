import { Show, children, untrack, withAlpha } from "@solidrt/core"
import { createPress } from "./press"
import { theme } from "./theme"
import { policy } from "./policy"
import { space } from "./spacing"
import { typeStyle } from "./typography"
import { Spinner } from "./spinner"
import { Surface, surfaceSinks } from "./surface"
import type { LayoutProps } from "@solidrt/core"
import type { ElevationLevel, MaterialRole, StyleProps, TransitionProps } from "./types"
import { splitTransition, transitionEndFor, withTransitionDefaults } from "./types"
import { colorFade, scaleFeedback, pressScale } from "./motion"

export type ButtonVariant = "primary" | "secondary" | "ghost" | "danger"
export type ButtonSize = "sm" | "md" | "lg"

export interface ButtonProps extends TransitionProps {
  // A string/number is rendered as the themed label; anything else is rendered
  // as-is, so a button can hold custom content (an icon, a row, ...).
  children?: any
  // Visual role: primary (accent fill), secondary (darker-blue fill), ghost
  // (no fill until hover), danger (destructive accent fill). None draw a border.
  variant?: ButtonVariant
  // Width preset: pins a minimum width (a longer label still expands past
  // it), so a row of buttons lines up. Omitted, the button sizes to its
  // content; a full-width button is the caller's `layout={{ width: "100%" }}`
  // (or a stretching column parent). Padding is the same at every size.
  size?: ButtonSize
  // A returned promise makes this an async action: the button shows a
  // centered spinner in place of the label (geometry unchanged) and ignores
  // presses until it settles. Non-thenable returns are ignored.
  onPress?: () => unknown
  disabled?: boolean
  // Focus-navigation candidacy (spatial nav, TV remotes); on by default.
  // Disabled buttons are never candidates.
  focusable?: boolean
  ref?: (node: { id: number }) => void
  layout?: LayoutProps
  style?: StyleProps
}

// Minimum width per size preset (logical px). A minimum, not a hard width, so
// labels wider than the preset never clip.
const SIZE_WIDTH: Record<ButtonSize, number> = { sm: 88, md: 120, lg: 160 }

// A themed press target: a padded, centered box with a label, its face drawn
// by Surface. The primary and danger variants are accent faces, secondary
// and ghost control faces; a button stands raised (theme.elevation.raised,
// zero under the stock presets) unless ghost or disabled, or style.elevation
// says otherwise. Press feedback is the overlayPressed tint plus, on a face
// with depth (a sheen, a bevel or a height), the face sinking - its key
// shadow fading, its lit edge moving to the far side - and on a flat face a
// slight sprung scale; hover feedback is the overlayHover tint (non-touch
// interaction policies only), all reactive reads of the press state so no
// nodes are recreated. Override the box via style and the padding/sizing
// via layout; because hover is an overlay, it composes over a caller-set
// backgroundColor too. When disabled, it takes no pointer events at all and
// never glows. Focus (spatial nav) draws a ring under the focusRing policy
// in the theme's ring color; Enter/Space/remote-select activates (handled
// by createPress).
export function Button(props: ButtonProps) {
  // Fill and label color per variant, read reactively from the theme. No
  // variant draws a border.
  let colors = () => {
    let c = theme.color
    switch (props.variant ?? "primary") {
      case "secondary":
        return { fill: c.secondary, label: c.onSecondary }
      case "ghost":
        return { fill: "transparent", label: c.text }
      case "danger":
        return { fill: c.danger, label: c.onPrimary }
      default:
        return { fill: c.primary, label: c.onPrimary }
    }
  }
  let variant = () => props.variant ?? "primary"
  let role = (): MaterialRole => (variant() === "primary" || variant() === "danger" ? "accent" : "control")
  // Theme-level per-component overrides merged under the instance style.
  let styled = (): StyleProps => ({ ...theme.components.button, ...props.style })
  let idleFill = () =>
    props.disabled
      ? variant() === "ghost"
        ? "transparent"
        : theme.color.surface
      : colors().fill
  let bg = () => styled().backgroundColor ?? idleFill()
  let radius = () => styled().borderRadius ?? theme.radius.md
  let elevation = (): ElevationLevel => styled().elevation ?? (props.disabled || variant() === "ghost" ? "flat" : "raised")
  let label = () => (props.disabled ? theme.color.textMuted : colors().label)
  // Resolved once via children(): reading the raw children getter builds a new
  // subtree per read, so the typeof probe and the two mount sites below must
  // share this single memoized build (an unmounted build leaks native nodes).
  let resolved = children(() => props.children)
  let isText = () => typeof resolved() === "string" || typeof resolved() === "number"
  // props (not a literal) so a swapped-in onPress is read at event time.
  let press = createPress(props)
  let pressed = () => !props.disabled && press.pressed()
  let hovered = () => !props.disabled && press.hovered() && policy.interaction !== "touch"
  // A face with depth sinks on press; a flat one shrinks instead.
  let sinks = () => surfaceSinks(role(), elevation(), styled().material)
  let style = () => ({
    ...styled(),
    ...(press.focused() && policy.focusRing ? { borderWidth: theme.borderWidth.focus, borderColor: theme.color.ring } : {}),
    backgroundColor: bg(),
    borderRadius: radius(),
    // Always a number: a scale that flips from a number back to undefined
    // hits the transform decoder, which rejects null. Multiply so a
    // caller-set scale is preserved under the press feedback.
    scale: (styled().scale ?? 1) * (sinks() ? 1 : pressScale(press.pressed())),
  })

  let split = () => splitTransition(props.transition)

  return (
    <view
      transition={withTransitionDefaults(split().root, scaleFeedback())}
      onTransitionEnd={transitionEndFor("root", props.onTransitionEnd)}
      ref={(n: { id: number }) => {
        press.ref(n)
        // A ref callback runs in the element's owned scope, where a prop
        // read warns in dev (STRICT_READ_UNTRACKED); the caller's ref is a
        // one-shot, read untracked.
        untrack(() => props.ref)?.(n)
      }}
      repaintBoundary
      flexDirection="row"
      alignItems="center"
      justifyContent="center"
      position="relative"
      paddingTop={space("md")}
      paddingBottom={space("md")}
      paddingLeft={space("lg")}
      paddingRight={space("lg")}
      minWidth={props.size ? SIZE_WIDTH[props.size] : undefined}
      {...props.layout}
      x={style().x}
      y={style().y}
      scale={style().scale}
      rotate={style().rotate}
      opacity={style().opacity}
      {...press.handlers}
      cursor="pointer"
      focusable={(props.focusable ?? true) && props.disabled !== true}
      pointerEvents={props.disabled ? "none" : undefined}
    >
      <Surface
        role={role()}
        elevation={elevation()}
        fill={bg()}
        radius={radius()}
        material={styled().material}
        glow={props.disabled ? null : styled().glow}
        chrome={variant() !== "ghost" || pressed() || hovered()}
        pressed={pressed()}
        hovered={hovered()}
        outline={(style().borderWidth ?? 0) > 0 ? { color: style().borderColor ?? "transparent", width: style().borderWidth! } : null}
        fillTransition={split().background}
        onFillTransitionEnd={transitionEndFor("background", props.onTransitionEnd)}
        outlineTransition={split().border}
        onOutlineTransitionEnd={transitionEndFor("border", props.onTransitionEnd)}
      />
      <Show when={isText()} fallback={resolved()}>
        <text transition={colorFade()} color={press.pending() ? withAlpha(label(), 0) : label()} {...typeStyle("body")}>
          {resolved()}
        </text>
      </Show>
      <Show when={press.pending()}>
        <view position="absolute" top={0} bottom={0} left={0} right={0} alignItems="center" justifyContent="center">
          <Spinner size={16} thickness={2} style={{ color: label() }} />
        </view>
      </Show>
    </view>
  )
}
