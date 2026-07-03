import { createSignal, Switch, Match, For } from "@solidrt/core"
import type { LayoutProps } from "@solidrt/core"
import { Pressable, type PressState } from "./pressable"
import { theme } from "./theme"
import { policy } from "./policy"
import { space } from "./spacing"
import { typeStyle } from "./typography"

export interface NavItem {
  value: unknown
  label: string
  // Optional icon content, rendered as-is above (tabs/rail) or beside
  // (sidebar) the label.
  icon?: any
}

export interface NavShellProps {
  items: NavItem[]
  // Controlled selected value. If omitted, the shell is uncontrolled.
  value?: unknown
  defaultValue?: unknown
  onChange?: (value: unknown) => void
  // The page content; keeps its node (and state) when the arrangement changes.
  children?: any
  layout?: LayoutProps
}

const RAIL_WIDTH = 72
const SIDEBAR_WIDTH = 220

/**
 * An app shell that arranges primary navigation around the content per the
 * navigation policy: bottom tabs under it, a narrow rail or a wide sidebar
 * beside it. The content is a single stable node; switching arrangement only
 * flips the shell's flex direction and remounts the (stateless) nav strip, so
 * page state survives a resize across a breakpoint. Safe areas are the
 * caller's concern: wrap the shell (or the window content) in SafeArea.
 */
export function NavShell(props: NavShellProps) {
  let [internal, setInternal] = createSignal(props.defaultValue)
  let value = () => (props.value !== undefined ? props.value : internal())
  let select = (v: unknown) => {
    if (props.value === undefined) setInternal(() => v)
    props.onChange?.(v)
  }

  let labelColor = (item: NavItem) => (item.value === value() ? theme.color.primary : theme.color.textMuted)
  let itemBg = (item: NavItem, s: PressState) =>
    item.value === value()
      ? theme.color.surfaceAlt
      : s.hovered && policy.interaction !== "touch"
        ? theme.color.surfaceHover
        : "transparent"

  // Icon over a small label, centered; shared by the tab bar and the rail.
  let StackedItem = (p: { item: NavItem; padY: number; layout?: LayoutProps }) => (
    <Pressable
      onPress={() => select(p.item.value)}
      layout={{
        flexDirection: "column",
        alignItems: "center",
        justifyContent: "center",
        gap: theme.spacing.sm,
        paddingTop: p.padY,
        paddingBottom: p.padY,
        ...p.layout,
      }}
      style={(s: PressState) => ({ backgroundColor: itemBg(p.item, s), borderRadius: theme.radius.sm })}
    >
      {p.item.icon}
      <text color={labelColor(p.item)} {...typeStyle("caption")}>
        {p.item.label}
      </text>
    </Pressable>
  )

  let Hairline = (p: { vertical?: boolean }) => (
    <view width={p.vertical ? 1 : undefined} height={p.vertical ? undefined : 1}>
      <d-rect color={theme.color.border} />
    </view>
  )

  let Tabs = () => (
    <view flexDirection="column" flexShrink={0}>
      <Hairline />
      <view flexDirection="row">
        <d-rect color={theme.color.surface} />
        <For each={props.items}>
          {(item: NavItem) => <StackedItem item={item} padY={theme.spacing.md} layout={{ flex: 1 }} />}
        </For>
      </view>
    </view>
  )

  let Rail = () => (
    <view flexDirection="row" flexShrink={0}>
      <view flexDirection="column" width={RAIL_WIDTH} gap={theme.spacing.sm} paddingTop={theme.spacing.md}>
        <d-rect color={theme.color.surface} />
        <For each={props.items}>{(item: NavItem) => <StackedItem item={item} padY={theme.spacing.md} />}</For>
      </view>
      <Hairline vertical />
    </view>
  )

  let Sidebar = () => (
    <view flexDirection="row" flexShrink={0}>
      <view flexDirection="column" width={SIDEBAR_WIDTH} gap={theme.spacing.sm} paddingTop={theme.spacing.md}>
        <d-rect color={theme.color.surface} />
        <For each={props.items}>
          {(item: NavItem) => (
            <Pressable
              onPress={() => select(item.value)}
              layout={{
                flexDirection: "row",
                alignItems: "center",
                gap: theme.spacing.md,
                paddingTop: space("sm") + 2,
                paddingBottom: space("sm") + 2,
                paddingLeft: theme.spacing.md,
                paddingRight: theme.spacing.md,
                marginLeft: theme.spacing.sm,
                marginRight: theme.spacing.sm,
              }}
              style={(s: PressState) => ({ backgroundColor: itemBg(item, s), borderRadius: theme.radius.sm })}
            >
              {item.icon}
              <text
                color={item.value === value() ? theme.color.primary : theme.color.text}
                {...typeStyle("body")}
              >
                {item.label}
              </text>
            </Pressable>
          )}
        </For>
      </view>
      <Hairline vertical />
    </view>
  )

  // Children order is (content, nav): "column" puts the nav under the content,
  // "row-reverse" puts it to the left, and the content node never moves.
  return (
    <view flexDirection={policy.navigation === "bottomTabs" ? "column" : "row-reverse"} {...props.layout}>
      <view flex={1} flexDirection="column">
        {props.children}
      </view>
      <Switch>
        <Match when={policy.navigation === "bottomTabs"}>
          <Tabs />
        </Match>
        <Match when={policy.navigation === "rail"}>
          <Rail />
        </Match>
        <Match when={policy.navigation === "sidebar"}>
          <Sidebar />
        </Match>
      </Switch>
    </view>
  )
}
