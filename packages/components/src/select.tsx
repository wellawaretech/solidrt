import { createSignal, createPortal, onLayout, getBoundingBox, Show, For, env } from "@solidrt/core"
import type { LayoutProps } from "@solidrt/core"
import { Pressable, type PressState } from "./pressable"
import { theme } from "./theme"
import { policy } from "./policy"
import { space } from "./spacing"
import { typeStyle } from "./typography"
import type { StyleProps } from "./types"

export interface SelectOption {
  value: unknown
  label: string
}

export interface SelectProps {
  options: SelectOption[]
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

const GAP = 4
// Minimum distance kept between the dropdown and the window edges.
const MARGIN = 4

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
  let OptionRow = (p: { option: SelectOption; padY: number }) => (
    <Pressable
      onPress={() => choose(p.option.value)}
      layout={{
        flexDirection: "row",
        alignItems: "center",
        paddingTop: p.padY,
        paddingBottom: p.padY,
        paddingLeft: space("md"),
        paddingRight: space("md"),
      }}
      style={(s: PressState) => ({
        backgroundColor:
          s.pressed || (s.hovered && policy.interaction !== "touch") ? theme.color.surfaceHover : "transparent",
      })}
    >
      <text {...bodyText(p.option.value === value() ? theme.color.primary : theme.color.text)}>{p.option.label}</text>
    </Pressable>
  )

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
      let x = Math.round(Math.min(Math.max(a.x, MARGIN), env.windowSize.width - b.width - MARGIN))
      let below = a.y + a.height + GAP
      let y = Math.round(below + b.height > env.windowSize.height - MARGIN ? a.y - b.height - GAP : below)
      let cur = pos()
      if (!cur || cur.x !== x || cur.y !== y) setPos({ x, y })
    })
    return createPortal(
      <view position="absolute" top={0} left={0} right={0} bottom={0}>
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
          <d-rect color={theme.color.surface} radius={theme.radius.sm} />
          <For each={props.options}>
            {(o: SelectOption) => <OptionRow option={o} padY={space("sm")} />}
          </For>
          <d-rect
            drawStyle="stroke"
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
      <view position="absolute" top={0} left={0} right={0} bottom={0}>
        <view position="absolute" top={0} left={0} right={0} bottom={0} onPointerDown={() => setOpen(false)}>
          <d-rect color={theme.color.scrim} />
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
          <d-rect color={theme.color.surface} radius={theme.radius.sm} />
          <For each={props.options}>
            {(o: SelectOption) => <OptionRow option={o} padY={Math.round(theme.spacing.md * 1.5)} />}
          </For>
        </view>
      </view>,
    )

  return (
    <Pressable
      ref={(n) => (trigger = n)}
      onPress={() => setOpen(!open())}
      disabled={props.disabled}
      layout={{
        flexDirection: "row",
        alignItems: "center",
        justifyContent: "space-between",
        gap: theme.spacing.md,
        paddingTop: space("sm"),
        paddingBottom: space("sm"),
        paddingLeft: space("md"),
        paddingRight: space("md"),
        ...props.layout,
      }}
      style={(s: PressState) => ({
        borderColor: theme.color.border,
        borderWidth: theme.borderWidth.sm,
        borderRadius: theme.radius.sm,
        ...props.style,
        backgroundColor:
          props.style?.backgroundColor ??
          (s.hovered && !props.disabled && policy.interaction !== "touch"
            ? theme.color.surfaceHover
            : theme.color.surface),
      })}
    >
      <Show
        when={selected()}
        fallback={<text {...bodyText(theme.color.textMuted)}>{props.placeholder ?? ""}</text>}
      >
        <text {...bodyText(props.disabled ? theme.color.textMuted : theme.color.text)}>{selected()!.label}</text>
      </Show>
      <view width={12} height={8}>
        <d-path
          d="M 2 2 L 6 6 L 10 2"
          drawStyle="stroke"
          color={theme.color.textMuted}
          strokeWidth={2}
          strokeCap="round"
          strokeJoin="round"
        />
      </view>
      <Show when={open()}>
        <Show when={policy.interaction === "touch"} fallback={<Dropdown />}>
          <Sheet />
        </Show>
      </Show>
    </Pressable>
  )
}
