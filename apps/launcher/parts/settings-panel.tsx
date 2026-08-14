// The settings panel: theme mode, fullscreen, and the runtime's build
// identity. Reached
// from the home header gear. Lives in the home SplitView's detail pane: beside
// the list in two-pane, the whole screen in single-pane. The heading row's
// back arrow closes it in both layouts (unlike the app detail, the list offers
// no other affordance to dismiss it). Single-pane centers the column, two-pane
// leaves it against the split's hairline, per the SplitView contract. Its
// column is the detail pane's width, not the list's, so switching between an
// app's details and settings does not resize the pane's content.
import { For } from "solid-js"
import {
  View,
  Text,
  ScrollView,
  SegmentedControl,
  Pressable,
  Icon,
  type PressState,
  theme,
  space,
  policy,
} from "@solidrt/components"
import {
  version as buildVersion,
  profile as buildProfile,
  platform as buildPlatform,
} from "srt:apps"
import { DetailCard, DetailRow } from "./detail-card"
import { BackButton } from "./back-button"
import { DETAIL_MAX_WIDTH, TAP_TARGET, focusRing, type ThemeMode } from "./types"

// Lucide maximize/minimize, stroked with currentColor so Icon recolors them.
const MAXIMIZE_SVG = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M8 3H5a2 2 0 0 0-2 2v3"/><path d="M21 8V5a2 2 0 0 0-2-2h-3"/><path d="M3 16v3a2 2 0 0 0 2 2h3"/><path d="M16 21h3a2 2 0 0 0 2-2v-3"/></svg>`
const MINIMIZE_SVG = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M8 3v3a2 2 0 0 1-2 2H3"/><path d="M21 8h-3a2 2 0 0 1-2-2V3"/><path d="M3 16h3a2 2 0 0 1 2 2v3"/><path d="M16 21v-3a2 2 0 0 1 2-2h3"/></svg>`

// One capability name as a filled chip, for the About block's list.
function CapabilityChip(props: { name: string }) {
  return (
    <View
      layout={{
        paddingLeft: space("md"),
        paddingRight: space("md"),
        paddingTop: space("sm"),
        paddingBottom: space("sm"),
      }}
      style={{ backgroundColor: theme.color.surfaceAlt, borderRadius: theme.radius.sm }}
    >
      <Text variant="body" muted>
        {props.name}
      </Text>
    </View>
  )
}

const THEME_MODES: ThemeMode[] = ["system", "light", "dark"]

export function SettingsPanel(props: {
  mode: ThemeMode
  onMode: (mode: ThemeMode) => void
  fullscreen: boolean
  onFullscreen: (on: boolean) => void
  onBack: () => void
}) {
  // The whole segmented control is one focus target; activating it steps to
  // the next mode (a remote has no way to aim at a single segment). Pointer
  // taps on the segments hit their inner pressables first (innermost wins),
  // so only a press on the row's padding cycles.
  let cycleMode = () =>
    props.onMode(THEME_MODES[(THEME_MODES.indexOf(props.mode) + 1) % THEME_MODES.length]!)
  return (
    <ScrollView layout={{ flexGrow: 1 }}>
      <View
        layout={{ flexGrow: 1, alignItems: policy.layout === "twoPane" ? "flex-start" : "center" }}
      >
        <View
          layout={{
            flexDirection: "column",
            gap: space("lg"),
            width: "100%",
            maxWidth: DETAIL_MAX_WIDTH,
            padding: space("xl"),
          }}
        >
          <View
            layout={{
              flexDirection: "row",
              alignItems: "center",
              justifyContent: "space-between",
            }}
          >
            <View layout={{ flexDirection: "row", alignItems: "center", gap: space("md") }}>
              <BackButton onPress={props.onBack} />
              <Text variant="heading">Settings</Text>
            </View>
            <Pressable
              focusable
              onPress={() => props.onFullscreen(!props.fullscreen)}
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
              <Icon src={props.fullscreen ? MINIMIZE_SVG : MAXIMIZE_SVG} size={22} />
            </Pressable>
          </View>
          <DetailCard title="Appearance">
            <Pressable focusable onPress={cycleMode} style={(s: PressState) => focusRing(s.focused)}>
              <SegmentedControl
                options={[
                  { value: "system", label: "System" },
                  { value: "light", label: "Light" },
                  { value: "dark", label: "Dark" },
                ]}
                value={props.mode}
                onChange={(v) => props.onMode(v as ThemeMode)}
              />
            </Pressable>
          </DetailCard>
          <DetailCard title="About">
            <DetailRow label="Build version" value={buildVersion} />
            <DetailRow label="Profile" value={buildProfile} />
            <DetailRow label="Flux version" value={Flux.version} />
            <DetailRow label="Platform" value={buildPlatform} />
          </DetailCard>
          <DetailCard title="Capabilities">
            <View layout={{ flexDirection: "row", flexWrap: "wrap", gap: space("sm") }}>
              <For each={Flux.capabilities}>{(name) => <CapabilityChip name={name} />}</For>
            </View>
          </DetailCard>
        </View>
      </View>
    </ScrollView>
  )
}
