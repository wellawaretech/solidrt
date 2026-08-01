// The settings panel: theme mode and the runtime's build identity. Reached
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
  theme,
  space,
  policy,
} from "@solidrt/components"
import { navTarget, navRing } from "./nav"
import {
  version as buildVersion,
  profile as buildProfile,
  platform as buildPlatform,
} from "srt:apps"
import { DetailCard, DetailRow } from "./detail-card"
import { BackButton } from "./back-button"
import { DETAIL_MAX_WIDTH, type ThemeMode } from "./types"

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
  onBack: () => void
}) {
  // The whole segmented control is one nav target; activating it steps to the
  // next mode (a remote has no way to aim at a single segment).
  let modeNav = navTarget(() =>
    props.onMode(THEME_MODES[(THEME_MODES.indexOf(props.mode) + 1) % THEME_MODES.length]!),
  )
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
          <View layout={{ flexDirection: "row", alignItems: "center", gap: space("md") }}>
            <BackButton onPress={props.onBack} />
            <Text variant="heading">Settings</Text>
          </View>
          <DetailCard title="Appearance">
            <View ref={modeNav.ref} style={navRing(modeNav.focused())}>
              <SegmentedControl
                options={[
                  { value: "system", label: "System" },
                  { value: "light", label: "Light" },
                  { value: "dark", label: "Dark" },
                ]}
                value={props.mode}
                onChange={(v) => props.onMode(v as ThemeMode)}
              />
            </View>
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
