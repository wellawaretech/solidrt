// The settings screen: theme mode and the runtime's build identity. Reached
// from the home header gear; a Back button returns home in both layouts.
import { For } from "solid-js"
import { View, Text, Button, ScrollView, SegmentedControl, theme, space } from "@solidrt/components"
import {
  version as buildVersion,
  profile as buildProfile,
  platform as buildPlatform,
} from "srt:apps"
import { DetailCard, DetailRow } from "./detail-card"
import type { ThemeMode } from "./types"

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

export function SettingsScreen(props: {
  mode: ThemeMode
  onMode: (mode: ThemeMode) => void
  onBack: () => void
}) {
  return (
    <ScrollView layout={{ flexGrow: 1 }}>
      <View layout={{ flexGrow: 1, alignItems: "center" }}>
        <View
          layout={{
            flexDirection: "column",
            gap: space("lg"),
            width: "100%",
            maxWidth: 440,
            padding: space("xl"),
          }}
        >
          <View layout={{ flexDirection: "row" }}>
            <Button variant="ghost" size="sm" onPress={props.onBack}>
              Back
            </Button>
          </View>
          <Text variant="heading">Settings</Text>
          <DetailCard title="Appearance">
            <SegmentedControl
              options={[
                { value: "system", label: "System" },
                { value: "light", label: "Light" },
                { value: "dark", label: "Dark" },
              ]}
              value={props.mode}
              onChange={(v) => props.onMode(v as ThemeMode)}
            />
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
