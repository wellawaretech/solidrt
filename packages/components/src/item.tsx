import { Show, children } from "@solidrt/core"
import type { LayoutProps } from "@solidrt/core"
import { createPress, type PressState } from "./press"
import { theme } from "./theme"
import { policy } from "./policy"
import { space } from "./spacing"
import { typeStyle } from "./typography"
import type { StyleProps } from "./types"

export interface ItemProps {
  // Leading content: an icon, avatar, checkbox, ...
  startContent?: any
  // Primary text. A string/number renders as themed body text; anything else
  // as-is.
  label: any
  // Secondary line under the label. A string/number renders as themed muted
  // caption text; anything else as-is.
  description?: any
  // Trailing content: a badge, timestamp, chevron, action, ...
  endContent?: any
  // Present = the row is interactive: hover/pressed overlay tints, focusable,
  // Enter/remote activation. A returned promise defers further activations
  // until it settles; non-thenable returns are ignored.
  onPress?: () => unknown
  // Fills the row with surfaceAlt to mark the current selection.
  selected?: boolean
  disabled?: boolean
  // Focus-navigation candidacy; defaults to true for interactive rows.
  focusable?: boolean
  ref?: (node: { id: number }) => void
  layout?: LayoutProps
  style?: StyleProps
}

// A list row: leading content, a label with an optional description under it,
// trailing content pushed to the end. The dense-data workhorse - rows compose
// with <For> inside a plain column (or a ScrollView); this package ships no
// List wrapper because a column view IS the list. Paddings and the gap are
// density-scaled, so a <Density> region compacts rows wholesale. With onPress
// the row presses like a menu entry: overlay tints for hover/pressed (no
// scale - rows sit flush in a list), focus ring under the focusRing policy.
export function Item(props: ItemProps) {
  // Theme-level per-component overrides merged under the instance style.
  let styled = () => ({ ...theme.components.item, ...props.style })
  let interactive = () => props.onPress != null && !props.disabled
  let press = createPress(props)

  let bg = () => styled().backgroundColor ?? (props.selected ? theme.color.surfaceAlt : "transparent")
  let overlay = (s: PressState) =>
    !interactive()
      ? "transparent"
      : s.pressed
        ? theme.color.overlayPressed
        : s.hovered && policy.interaction !== "touch"
          ? theme.color.overlayHover
          : "transparent"
  let radius = () => styled().borderRadius ?? theme.radius.sm

  // Resolved once via children(): the typeof probe and the mount site must
  // share one build (see Button).
  let label = children(() => props.label)
  let labelIsText = () => typeof label() === "string" || typeof label() === "number"
  let description = children(() => props.description)
  let descriptionIsText = () => typeof description() === "string" || typeof description() === "number"

  return (
    <view
      ref={(n: { id: number }) => {
        press.ref(n)
        props.ref?.(n)
      }}
      repaintBoundary
      flexDirection="row"
      alignItems="center"
      gap={space("md")}
      paddingTop={space("md")}
      paddingBottom={space("md")}
      paddingLeft={space("lg")}
      paddingRight={space("lg")}
      {...props.layout}
      x={styled().x}
      y={styled().y}
      scale={styled().scale}
      rotate={styled().rotate}
      opacity={props.disabled ? 0.5 : styled().opacity}
      // A passive row attaches no press recognizer: it must not claim
      // pointers from its children (a Switch in a settings row) or from an
      // enclosing pressable. Interactivity is decided at mount.
      {...(props.onPress != null ? press.handlers : {})}
      focusable={(props.focusable ?? true) && interactive()}
      pointerEvents={props.disabled ? "none" : undefined}
    >
      <d-rect color={bg()} radius={radius()} />
      <d-rect color={overlay(press.state())} radius={radius()} />
      {props.startContent}
      <view flexDirection="column" flexGrow={1} flexShrink={1} gap={2}>
        <Show when={labelIsText()} fallback={label()}>
          <text color={theme.color.text} {...typeStyle("body")} maxLines={1}>
            {label()}
          </text>
        </Show>
        <Show when={props.description != null}>
          <Show when={descriptionIsText()} fallback={description()}>
            <text color={theme.color.textMuted} {...typeStyle("caption")} maxLines={1}>
              {description()}
            </text>
          </Show>
        </Show>
      </view>
      {props.endContent}
      <Show when={press.focused() && policy.focusRing}>
        <d-rect drawStyle="stroke" color={theme.color.text} strokeWidth={2} radius={radius()} />
      </Show>
    </view>
  )
}
