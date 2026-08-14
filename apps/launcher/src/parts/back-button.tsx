// The launcher's back affordance: an arrow, no label, meant to sit at the left
// of a screen's heading row rather than on a row of its own (a lone labelled
// button above the title reads as a stray form control, and duplicates what the
// title already says). Every view that fills a pane carries one, in both
// layouts: two-pane can reach the others without it, but a pane with no way
// back out of it reads as stuck, and the arrow always means the same thing
// (leave this view, keep the rest of the screen).
import { Pressable, Icon, theme, type PressState } from "@solidrt/components"
import { TAP_TARGET, focusRing } from "./types"

// Lucide arrow-left, stroked with currentColor so Icon recolors it.
const ARROW_LEFT_SVG = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m12 19-7-7 7-7"/><path d="M19 12h-14"/></svg>`

export function BackButton(props: { onPress: () => void }) {
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
      style={(s: PressState) => ({
        backgroundColor: s.hovered ? theme.color.surfaceHover : "transparent",
        borderRadius: theme.radius.md,
        ...focusRing(s.focused),
      })}
    >
      <Icon src={ARROW_LEFT_SVG} size={22} />
    </Pressable>
  )
}
