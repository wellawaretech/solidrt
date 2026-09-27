// The lit presets: the same components under darkTheme, lightTheme,
// litDarkTheme and litLightTheme, side by side with a light pad. Under the
// lit presets every face is a role's material under theme.light - a sheen
// along the light, a bevel on its edges, shadows cast from its elevation,
// a press that sinks - and dragging the pad moves the light for the whole
// screen at once (setTheme({ light })). The flat presets are the same
// screen with every material empty and every height zero.
import { render, createSignal, getLayoutBox } from "@solidrt/core"
import type { PointerEvent } from "@solidrt/core"
import {
  Badge,
  Button,
  Card,
  Checkbox,
  Item,
  ProgressBar,
  Radio,
  RadioGroup,
  SegmentedControl,
  Select,
  Slider,
  Surface,
  Switch,
  Text,
  TextInput,
  View,
  Window,
  darkTheme,
  lightTheme,
  litDarkTheme,
  litLightTheme,
  lightFrom,
  lightSource,
  setTheme,
  theme,
} from "@solidrt/components"

type Preset = "flat" | "lit"
type Scheme = "dark" | "light"

const PRESETS = [
  { value: "flat" as const, label: "Flat" },
  { value: "lit" as const, label: "Lit" },
]
const SCHEMES = [
  { value: "dark" as const, label: "Dark" },
  { value: "light" as const, label: "Light" },
]
const RANGES = [
  { value: "day", label: "Day" },
  { value: "week", label: "Week" },
  { value: "month", label: "Month" },
]
// The light pad's size, and its marker.
const PAD = 120
const MARKER = 14

function themeFor(preset: Preset, scheme: Scheme) {
  if (preset === "lit") return scheme === "dark" ? litDarkTheme : litLightTheme
  return scheme === "dark" ? darkTheme : lightTheme
}

// A pad showing where the light comes from; dragging over it moves the
// light. The direction belongs to the app, so a preset switch keeps it.
function LightPad() {
  let node: { id: number } | undefined
  let [dragging, setDragging] = createSignal(false)
  let move = (e: PointerEvent) => {
    if (!node) return
    let box = getLayoutBox(node)
    if (!box || box.width === 0) return
    let px = (e.localX / box.width) * 2 - 1
    let py = (e.localY / box.height) * 2 - 1
    setTheme({ light: { ...theme.light, direction: lightFrom(px, py) } })
  }
  let marker = () => {
    let [sx, sy] = lightSource(theme.light)
    return { x: ((sx + 1) / 2) * PAD - MARKER / 2, y: ((sy + 1) / 2) * PAD - MARKER / 2 }
  }
  return (
    <view
      ref={(n: { id: number }) => (node = n)}
      width={PAD}
      height={PAD}
      cursor="pointer"
      onPointerDown={(e: PointerEvent) => {
        setDragging(true)
        move(e)
      }}
      onPointerMove={(e: PointerEvent) => dragging() && move(e)}
      onPointerUp={() => setDragging(false)}
    >
      <Surface role="control" sunken radius={theme.radius.lg} />
      <d-rect x={marker().x} y={marker().y} w={MARKER} h={MARKER} radius={MARKER / 2} color={theme.light.color} />
    </view>
  )
}

// The look, at module scope: the initial setTheme runs at load, outside any
// component's owned scope (a theme write belongs to an event or to init,
// never to a component's setup).
let [preset, setPreset] = createSignal<Preset>("lit")
let [scheme, setScheme] = createSignal<Scheme>("dark")
function apply(p: Preset, s: Scheme) {
  // The light's direction is the app's: carry it across the switch.
  let direction = theme.light.direction
  setTheme(themeFor(p, s))
  setTheme({ light: { ...theme.light, direction } })
}
apply(preset(), scheme())

function App() {
  let [notify, setNotify] = createSignal(true)
  let [volume, setVolume] = createSignal(62)
  let [range, setRange] = createSignal("week")
  let [query, setQuery] = createSignal("")

  return (
    <Window title="Lit components" style={{ backgroundColor: theme.color.background }} layout={{ padding: 24, gap: 24, flexDirection: "row", alignItems: "flex-start" }}>
      <View layout={{ flexDirection: "column", gap: 16, width: 200 }}>
        <SegmentedControl
          options={PRESETS}
          value={preset()}
          onChange={(v) => {
            setPreset(v)
            apply(v, scheme())
          }}
        />
        <SegmentedControl
          options={SCHEMES}
          value={scheme()}
          onChange={(v) => {
            setScheme(v)
            apply(preset(), v)
          }}
        />
        <Text variant="caption" style={{ color: theme.color.textMuted }}>
          Drag to move the light
        </Text>
        <LightPad />
      </View>
      <Card title="Controls" lift layout={{ width: 360, gap: 16 }}>
        <View layout={{ flexDirection: "row", gap: 8, flexWrap: "wrap" }}>
          <Button>Run</Button>
          <Button variant="secondary">Save</Button>
          <Button variant="ghost">Cancel</Button>
          <Button variant="danger">Delete</Button>
          <Button disabled>Off</Button>
        </View>
        <View layout={{ flexDirection: "row", alignItems: "center", justifyContent: "space-between" }}>
          <Text>Notifications</Text>
          <Switch value={notify()} onChange={setNotify} />
        </View>
        <View layout={{ flexDirection: "row", alignItems: "center", gap: 12 }}>
          <Checkbox defaultChecked />
          <Checkbox />
          <RadioGroup defaultValue="a" layout={{ flexDirection: "row", gap: 12 }}>
            <Radio value="a">Alpha</Radio>
            <Radio value="b">Beta</Radio>
          </RadioGroup>
        </View>
        <Slider value={volume()} onChange={setVolume} layout={{ width: "100%" }} />
        <ProgressBar value={volume() / 100} />
        <View layout={{ flexDirection: "row", gap: 8 }}>
          <Badge>New</Badge>
          <Badge variant="neutral">12</Badge>
          <Badge variant="danger">Err</Badge>
        </View>
        <SegmentedControl options={RANGES} value={range()} onChange={setRange} />
        <TextInput value={query()} onInput={setQuery} placeholder="Search" />
        <Select options={RANGES} defaultValue="week" />
      </Card>
      <Card title="Inbox" layout={{ width: 320, gap: 0, padding: 8 }}>
        <Item label="Compile" description="12 s" onPress={() => {}} />
        <Item label="Link" description="1 s" onPress={() => {}} selected />
        <Item label="Package" description="4 s" onPress={() => {}} />
      </Card>
    </Window>
  )
}

render(() => <App />)
