// An installed app's icon: the manifest-declared SVG source carried by
// srt:apps, parsed with parseSvg and rendered with its own colors (unlike the
// currentColor-recolored Icon component). Apps without one - or with a source
// that fails to parse - fall back to a monogram: a muted rounded square
// showing the display name's first letter, drawn with the core <text>
// primitive so it never outgrows the box under policy.textScale.
import { Show, createMemo, For } from "solid-js"
import { parseSvg, type SvgDocument } from "@solidrt/core"
import { View, theme } from "@solidrt/components"
import type { InstalledApp } from "srt:apps"

export function AppIcon(props: { app: InstalledApp; size: number }) {
  let doc = createMemo<SvgDocument | undefined>(() => {
    let src = props.app.icon
    if (!src) return undefined
    try {
      return parseSvg(src)
    } catch (err) {
      console.warn(`App icon for ${props.app.name} failed to parse: ${err}`)
      return undefined
    }
  })

  return (
    <Show
      when={doc()}
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
      {(d) => (
        <view
          repaintBoundary
          pointerEvents="all"
          width={props.size}
          height={props.size}
          flexShrink={0}
          viewBox={[d().width, d().height]}
        >
          <For each={d().draws}>{(draw) => <d-path {...draw} />}</For>
        </view>
      )}
    </Show>
  )
}
