// An installed app's icon: the manifest-declared SVG source carried by
// srt:apps, rendered with its own colors (unlike the currentColor-recolored
// Icon component). Apps without one fall back to a
// monogram: a muted rounded square showing the display name's first letter,
// drawn with the core <text> primitive so it never outgrows the box under
// policy.textScale.
import { Show } from "solid-js"
import { View, theme } from "@solidrt/components"
import type { InstalledApp } from "srt:apps"

export function AppIcon(props: { app: InstalledApp; size: number }) {
  return (
    <Show
      when={props.app.icon}
      fallback={
        <View
          layout={{
            width: props.size,
            height: props.size,
            alignItems: "center",
            justifyContent: "center",
            flexShrink: 0,
          }}
          style={{
            backgroundColor: theme.color.surfaceAlt,
            borderRadius: props.size / 4,
          }}
        >
          <text
            color={theme.color.textMuted}
            fontFamily={theme.text.fontFamily}
            fontSize={props.size * 0.45}
            fontWeight={500}
          >
            {props.app.name.slice(0, 1).toUpperCase()}
          </text>
        </View>
      }
    >
      {(src) => (
        <view width={props.size} height={props.size} flexShrink={0}>
          <svg width={props.size} height={props.size} src={src()} />
        </view>
      )}
    </Show>
  )
}
