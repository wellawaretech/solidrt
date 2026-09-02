import { createSignal, createPortal, onLayout, getBoundingBox, Show, For, env } from "@solidrt/core"
import type { LayoutProps } from "@solidrt/core"
import { createPress } from "./press"
import { theme } from "./theme"
import { policy } from "./policy"
import { space } from "./spacing"
import { typeStyle } from "./typography"
import type { Option, StyleProps, TransitionProps } from "./types"
import { splitTransition, transitionEndFor, withTransitionDefaults } from "./types"
import { Icon } from "./icon"
import { colorFade, popupFade, popupFadeOut, PressFeedback, travelMotion } from "./motion"

export interface SelectProps extends TransitionProps {
  options: Option[]
  // Controlled selected value. If omitted, the select is uncontrolled.
  value?: unknown
  defaultValue?: unknown
  onChange?: (value: unknown) => void
  // Shown in the trigger while nothing is selected.
  placeholder?: string
  disabled?: boolean
  layout?: LayoutProps
  style?: StyleProps
}

// Distance between the trigger and the dropdown.
let gap = () => theme.spacing.sm
// Minimum distance kept between the dropdown and the window edges.
let margin = () => theme.spacing.sm

/**
 * A single-choice picker whose presentation forks on the interaction policy:
 * desktop/hybrid opens an anchored dropdown under the trigger (flipping above
 * when there is no room), touch opens a bottom sheet over a scrim. Same
 * value/onChange contract either way; pressing outside closes without a change.
 * The option list is not scrollable yet, so keep it short.
 */
export function Select(props: SelectProps) {
  let trigger: { id: number } | undefined
  let [open, setOpen] = createSignal(false)
  let [internal, setInternal] = createSignal(props.defaultValue)
  let value = () => (props.value !== undefined ? props.value : internal())
  let selected = () => props.options.find((o) => o.value === value())

  let choose = (v: unknown) => {
    setOpen(false)
    if (props.value === undefined) setInternal(() => v)
    props.onChange?.(v)
  }

  let bodyText = (color: string) => ({ ...typeStyle("body"), color, maxLines: 1 })

  // One option row, shared by both presentations; only the vertical padding
  // differs (the sheet gets taller touch targets).
  let OptionRow = (p: { option: Option; padY: number }) => {
    let press = createPress({ onPress: () => choose(p.option.value) })
    return (
      <view
        ref={press.ref}
        repaintBoundary
        flexDirection="row"
        alignItems="center"
        paddingTop={p.padY}
        paddingBottom={p.padY}
        paddingLeft={space("md")}
        paddingRight={space("md")}
        {...press.handlers}
        focusable
      >
        <PressFeedback
          pressed={press.pressed()}
          hovered={(press.hovered() && policy.interaction !== "touch") || (press.focused() && policy.focusRing)}
        />
        <text transition={colorFade()} {...bodyText(p.option.value === value() ? theme.color.primary : theme.color.text)}>{p.option.label}</text>
      </view>
    )
  }

  // Anchored under the trigger, sized at least as wide as it. Positioned like
  // Tooltip: portal at the window root, pinned at 0,0 and moved with the x/y
  // paint transforms after measuring, so tracking the anchor never reflows.
  let Dropdown = () => {
    let menu: { id: number } | undefined
    let [pos, setPos] = createSignal<{ x: number; y: number } | null>(null)
    let [minWidth, setMinWidth] = createSignal(0)
    onLayout(() => {
      let a = trigger && getBoundingBox(trigger)
      let b = menu && getBoundingBox(menu)
      if (!a || !b) return
      if (a.width !== minWidth()) setMinWidth(a.width)
      let x = Math.round(Math.min(Math.max(a.x, margin()), env.windowSize.width - b.width - margin()))
      let below = a.y + a.height + gap()
      let y = Math.round(below + b.height > env.windowSize.height - margin() ? a.y - b.height - gap() : below)
      let cur = pos()
      if (!cur || cur.x !== x || cur.y !== y) setPos({ x, y })
    })
    // The fade is driven by the position write, not a mount-time `from`: the
    // menu parks offscreen until measured, so a `from` would play unseen.
    // The exit fade lives on this root (the node the close unmounts).
    return createPortal(
      <view
        position="absolute"
        top={0}
        left={0}
        right={0}
        bottom={0}
        opacity={pos() ? 1 : 0}
        transition={popupFadeOut()}
      >
        <view position="absolute" top={0} left={0} right={0} bottom={0} onPointerDown={() => setOpen(false)} />
        <view
          ref={(n: { id: number }) => (menu = n)}
          position="absolute"
          top={0}
          left={0}
          x={pos()?.x ?? -10000}
          y={pos()?.y ?? 0}
          minWidth={minWidth() || undefined}
          flexDirection="column"
          paddingTop={theme.spacing.sm}
          paddingBottom={theme.spacing.sm}
        >
          <d-rect transition={colorFade()} color={theme.color.surface} radius={theme.radius.sm} />
          <For each={props.options}>
            {(o: Option) => <OptionRow option={o} padY={space("sm")} />}
          </For>
          <d-rect
            drawStyle="stroke"
            transition={colorFade()}
            color={theme.color.border}
            strokeWidth={theme.borderWidth.sm}
            radius={theme.radius.sm}
          />
        </view>
      </view>,
    )
  }

  // Bottom sheet over a scrim; the content is a sibling of the scrim (Modal's
  // trick) so pressing an option never has the scrim on its hit path.
  let Sheet = () =>
    createPortal(
      <view position="absolute" top={0} left={0} right={0} bottom={0} opacity={1} transition={popupFade()}>
        <view position="absolute" top={0} left={0} right={0} bottom={0} onPointerDown={() => setOpen(false)}>
          <d-rect transition={colorFade()} color={theme.color.scrim} />
        </view>
        <view
          position="absolute"
          left={0}
          right={0}
          bottom={0}
          flexDirection="column"
          paddingTop={theme.spacing.md}
          paddingBottom={theme.spacing.md + env.safeArea.bottom}
        >
          <d-rect transition={colorFade()} color={theme.color.surface} radius={theme.radius.sm} />
          <For each={props.options}>
            {(o: Option) => <OptionRow option={o} padY={Math.round(theme.spacing.md * 1.5)} />}
          </For>
        </view>
      </view>,
    )

  let press = createPress({
    onPress: () => {
      setOpen(!open())
    },
  })
  let style = () => ({
    borderColor: theme.color.border,
    borderWidth: theme.borderWidth.sm,
    borderRadius: theme.radius.md,
    backgroundColor: theme.color.surface,
    ...theme.components.select,
    ...props.style,
    ...(press.focused() && policy.focusRing ? { borderWidth: theme.borderWidth.focus, borderColor: theme.color.ring } : {}),
  })
  // The chevron flips while the picker is open; travel motion, so it snaps
  // under reduced motion (the flip itself stays).
  let chevronSpin = () => {
    let t = travelMotion()
    return t && { rotate: t }
  }

  let split = () => splitTransition(props.transition)

  return (
    <view
      transition={split().root}
      onTransitionEnd={transitionEndFor("root", props.onTransitionEnd)}
      ref={(n: { id: number }) => {
        press.ref(n)
        trigger = n
      }}
      repaintBoundary
      flexDirection="row"
      alignItems="center"
      justifyContent="space-between"
      gap={theme.spacing.md}
      paddingTop={space("md")}
      paddingBottom={space("md")}
      paddingLeft={space("md")}
      paddingRight={space("md")}
      {...props.layout}
      x={style().x}
      y={style().y}
      scale={style().scale}
      rotate={style().rotate}
      opacity={style().opacity}
      {...press.handlers}
      focusable={!props.disabled}
      pointerEvents={props.disabled ? "none" : undefined}
    >
      <d-rect transition={withTransitionDefaults(split().background, colorFade())} onTransitionEnd={transitionEndFor("background", props.onTransitionEnd)} color={style().backgroundColor ?? "transparent"} radius={style().borderRadius} />
      <PressFeedback
        pressed={press.pressed() && !props.disabled}
        hovered={press.hovered() && !props.disabled && policy.interaction !== "touch"}
        radius={style().borderRadius}
      />
      <Show
        when={selected()}
        fallback={<text transition={colorFade()} {...bodyText(theme.color.textMuted)}>{props.placeholder ?? ""}</text>}
      >
        <text transition={colorFade()} {...bodyText(props.disabled ? theme.color.textMuted : theme.color.text)}>{selected()!.label}</text>
      </Show>
      <view rotate={open() ? Math.PI : 0} transition={chevronSpin()}>
        <Show
          when={theme.icons.chevronDown}
          fallback={
            <view width={12} height={8}>
              <d-path
                d="M 2 2 L 6 6 L 10 2"
                drawStyle="stroke"
                transition={colorFade()}
                color={theme.color.textMuted}
                strokeWidth={2}
                strokeCap="round"
                strokeJoin="round"
              />
            </view>
          }
        >
          <Icon src={theme.icons.chevronDown!} size={12} color={theme.color.textMuted} />
        </Show>
      </view>
      <Show when={open()}>
        <Show when={policy.interaction === "touch"} fallback={<Dropdown />}>
          <Sheet />
        </Show>
      </Show>
      <Show when={(style().borderWidth ?? 0) > 0}>
        <d-rect
          drawStyle="stroke"
          transition={withTransitionDefaults(split().border, colorFade())}
          onTransitionEnd={transitionEndFor("border", props.onTransitionEnd)}
          color={style().borderColor ?? "transparent"}
          strokeWidth={style().borderWidth}
          radius={style().borderRadius}
        />
      </Show>
    </view>
  )
}
