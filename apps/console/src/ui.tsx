// What every pane draws with: the icons (Lucide paths, inlined so the source
// stays ASCII), the focus ring this app's own pressables wear (Button draws
// its own), and the icon button the pane headers use.
import { Icon, Pressable, Text, policy, space, theme, type PressState, type StyleProps } from "@solidrt/components"
import { clientLabel, type Client } from "./servers"

const LUCIDE = (body: string) =>
  `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none"` +
  ` stroke="currentColor" stroke-width="2" stroke-linecap="round"` +
  ` stroke-linejoin="round">${body}</svg>`

export const SERVER_ICON = LUCIDE(
  `<rect x="2" y="3" width="20" height="8" rx="2"/><rect x="2" y="13" width="20" height="8" rx="2"/>` +
    `<path d="M6 7h.01M6 17h.01"/>`,
)
export const BACK_ICON = LUCIDE(`<path d="m12 19-7-7 7-7"/><path d="M19 12h-14"/>`)
// The two outcomes a command can settle into.
export const CHECK_ICON = LUCIDE(`<path d="M20 6 9 17l-5-5"/>`)
export const CROSS_ICON = LUCIDE(`<path d="M18 6 6 18"/><path d="m6 6 12 12"/>`)
export const COLLAPSE_ICON = LUCIDE(
  `<rect x="3" y="3" width="18" height="18" rx="2"/><path d="M9 3v18"/><path d="m16 15-3-3 3-3"/>`,
)
export const EXPAND_ICON = LUCIDE(
  `<rect x="3" y="3" width="18" height="18" rx="2"/><path d="M9 3v18"/><path d="m14 9 3 3-3 3"/>`,
)

// Edge of an icon button's press box: the glyphs are small, so these boxes are
// sized rather than padded. Not density-scaled - a finger is the same size at
// every density.
export const TAP_TARGET = 44

// The focus-navigation ring, spread into a style. Text-colored so it stays
// visible on any fill.
export function focusRing(focused: boolean, radius?: number): StyleProps {
  if (!focused || !policy.focusRing) return {}
  return {
    borderWidth: 2,
    borderColor: theme.color.text,
    borderRadius: radius ?? theme.radius.md,
  }
}

export function IconButton(props: { icon: string; onPress: () => void }) {
  return (
    <Pressable
      focusable
      onPress={props.onPress}
      layout={{
        width: TAP_TARGET,
        height: TAP_TARGET,
        alignItems: "center",
        justifyContent: "center",
      }}
      style={(state: PressState) => ({
        backgroundColor: state.hovered ? theme.color.overlayHover : "transparent",
        borderRadius: theme.radius.md,
        ...focusRing(state.focused),
      })}
    >
      <Icon src={props.icon} size={22} />
    </Pressable>
  )
}

// One client as a pressable row, in the list pane under its server and in a
// Choose client block alike. The highlight is an overlay rather than a
// surface tone, so it shows on either surface the row sits on.
export function ClientChoice(props: { client: Client; active: boolean; onPress: () => void }) {
  return (
    <Pressable
      focusable
      onPress={props.onPress}
      layout={{ padding: space("sm") }}
      style={(state: PressState) => ({
        backgroundColor: props.active
          ? theme.color.overlayPressed
          : state.hovered
            ? theme.color.overlayHover
            : "transparent",
        borderRadius: theme.radius.md,
        ...focusRing(state.focused),
      })}
    >
      <Text>{clientLabel(props.client)}</Text>
    </Pressable>
  )
}
