import { render, createSignal, safeArea, createEffect, untrack, env, capabilities } from "@solidrt/core"
import {
  policy,
  setPolicy,
  type DensityPolicy,
  type MotionPolicy,
  Window,
  View,
  Text,
  Button,
  TextInput,
  Switch,
  Checkbox,
  RadioGroup,
  Radio,
  Slider,
  QrCode,
  Card,
  Divider,
  Badge,
  Spinner,
  ProgressBar,
  Tooltip,
  Select,
  ContextMenu,
  Icon,
  ScrollView,
  theme,
  space,
  setTheme,
  darkTheme,
  lightTheme,
  SafeArea,
} from "@solidrt/components"

// A growing gallery of @solidrt/components. The whole thing recolors live when
// the theme switches: every component reads theme.* reactively, so pressing
// "Toggle theme" reflows the palette with no remount. The "Environment and
// policies" card shows the environment -> capabilities -> policies cascade and
// lets you force density/motion overrides to watch every control adapt.

// Lucide icons are SVG documents (strings). In a real app you import them as
// assets (`import House from "lucide-static/icons/house.svg"`) or from a string
// export; we inline a few here so the example stays self-contained. Each strokes
// with currentColor, so Icon recolors them from the theme.
const LUCIDE = (body: string) =>
  `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none"` +
  ` stroke="currentColor" stroke-width="2" stroke-linecap="round"` +
  ` stroke-linejoin="round">${body}</svg>`
const HOUSE = LUCIDE(
  `<path d="M15 21v-8a1 1 0 0 0-1-1h-4a1 1 0 0 0-1 1v8"/>` +
    `<path d="M3 10a2 2 0 0 1 .709-1.528l7-6a2 2 0 0 1 2.582 0l7 6A2 2 0 0 1 21 10v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/>`,
)
const HEART = LUCIDE(
  `<path d="M2 9.5a5.5 5.5 0 0 1 9.591-3.676.56.56 0 0 0 .818 0A5.49 5.49 0 0 1 22 9.5c0 2.29-1.5 4-3 5.5l-5.492 5.313a2 2 0 0 1-3 .019L5 15c-1.5-1.5-3-3.2-3-5.5"/>`,
)
const STAR = LUCIDE(
  `<path d="M11.525 2.295a.53.53 0 0 1 .95 0l2.31 4.679a2.123 2.123 0 0 0 1.595 1.16l5.166.756a.53.53 0 0 1 .294.904l-3.736 3.638a2.123 2.123 0 0 0-.611 1.878l.882 5.14a.53.53 0 0 1-.771.56l-4.618-2.428a2.122 2.122 0 0 0-1.973 0L6.396 21.01a.53.53 0 0 1-.77-.56l.881-5.139a2.122 2.122 0 0 0-.611-1.879L2.16 9.795a.53.53 0 0 1 .294-.906l5.165-.755a2.122 2.122 0 0 0 1.597-1.16z"/>`,
)
const SETTINGS = LUCIDE(
  `<path d="M9.671 4.136a2.34 2.34 0 0 1 4.659 0 2.34 2.34 0 0 0 3.319 1.915 2.34 2.34 0 0 1 2.33 4.033 2.34 2.34 0 0 0 0 3.831 2.34 2.34 0 0 1-2.33 4.033 2.34 2.34 0 0 0-3.319 1.915 2.34 2.34 0 0 1-4.659 0 2.34 2.34 0 0 0-3.32-1.915 2.34 2.34 0 0 1-2.33-4.033 2.34 2.34 0 0 0 0-3.831A2.34 2.34 0 0 1 6.35 6.051a2.34 2.34 0 0 0 3.319-1.915"/>` +
    `<circle cx="12" cy="12" r="3"/>`,
)
const BELL = LUCIDE(
  `<path d="M10.268 21a2 2 0 0 0 3.464 0"/>` +
    `<path d="M3.262 15.326A1 1 0 0 0 4 17h16a1 1 0 0 0 .74-1.673C19.41 13.956 18 12.499 18 8A6 6 0 0 0 6 8c0 4.499-1.411 5.956-2.738 7.326"/>`,
)

function Row(props: { label: string; children?: any }) {
  return (
    <View
      layout={{
        flexDirection: "row",
        alignItems: "center",
        justifyContent: "space-between",
        gap: space("lg"),
      }}
    >
      <Text muted>{props.label}</Text>
      {props.children}
    </View>
  )
}

function Value(props: { children?: any }) {
  return <Text>{props.children}</Text>
}

// Joins the names whose flag is true, e.g. list(["mouse", env.mouseSeen], ...).
function list(...entries: [string, boolean][]) {
  let names = entries.filter(([, on]) => on).map(([name]) => name)
  return names.length > 0 ? names.join(", ") : "none yet"
}

function App() {
  let [dark, setDark] = createSignal(true)
  let [name, setName] = createSignal("")
  let [notify, setNotify] = createSignal(true)
  let [agree, setAgree] = createSignal(false)
  let [plan, setPlan] = createSignal<unknown>("pro")
  let [volume, setVolume] = createSignal(40)
  let [fruit, setFruit] = createSignal<unknown>(undefined)

  // The QR code follows the name field debounced, not per keystroke: each data
  // change regenerates the whole module grid, which is too heavy to run at
  // typing frequency.
  // untrack: seed with the current name once; the debounced effect below owns
  // keeping it in sync, and a bare name() read here would warn (strict mode
  // flags top-level reactive reads in component bodies as one-shot).
  let [qrData, setQrData] = createSignal(untrack(name))
  createEffect(
    () => name(),
    (v) => {
      let timer = setTimeout(() => setQrData(v), 250)
      return () => clearTimeout(timer)
    },
  )

  // Follow the OS dark/light preference until the user toggles manually; the
  // manual toggle then owns the choice for the rest of the session.
  let userToggledTheme = false
  let toggleTheme = () => {
    userToggledTheme = true
    let next = !dark()
    setDark(next)
    setTheme(next ? darkTheme : lightTheme)
  }
  createEffect(
    () => env.systemTheme,
    (t) => {
      if (userToggledTheme || t === "unknown") return
      let isDark = t === "dark"
      setDark(isDark)
      setTheme(isDark ? darkTheme : lightTheme)
    },
  )

  // Policy override choices; "auto" hands the policy back to the resolver.
  let [densityChoice, setDensityChoice] = createSignal<unknown>("auto")
  let [motionChoice, setMotionChoice] = createSignal<unknown>("auto")
  let chooseDensity = (v: unknown) => {
    setDensityChoice(v)
    setPolicy({ density: v === "auto" ? undefined : (v as DensityPolicy) })
  }
  let chooseMotion = (v: unknown) => {
    setMotionChoice(v)
    setPolicy({ motion: v === "auto" ? undefined : (v as MotionPolicy) })
  }
  let [textScaleChoice, setTextScaleChoice] = createSignal<unknown>("auto")
  let chooseTextScale = (v: unknown) => {
    setTextScaleChoice(v)
    setPolicy({ textScale: v === "auto" ? undefined : (v as number) })
  }

  return (
    <Window
      title="Components gallery"
      layout={{ flexDirection: "column" }}
      style={{ backgroundColor: theme.color.background }}
    >
      <SafeArea>
        <ScrollView horizontal layout={{ flex: 1 }}>
          <View
            layout={{
              height: "100%",
              flexDirection: "column",
              flexWrap: "wrap",
              alignContent: "flex-start",
              justifyContent: "flex-start",
              gap: space("xl"),
              padding: space("xl"),
            }}
          >
            <Card title={dark() ? "Dark theme" : "Light theme"} layout={{ width: 360 }}>
              <Text muted>
                Toggle to recolor every control below.
              </Text>
              <Button onPress={toggleTheme}>Toggle theme</Button>
            </Card>

            <Card title="Environment" layout={{ width: 360 }}>
              <Row label="Window">
                <Value>
                  {`${env.windowSize.width} x ${env.windowSize.height} @ ${env.displayScale}x`}
                </Value>
              </Row>
              <Row label="System">
                <Value>{`${env.systemTheme} theme, ${env.orientation}`}</Value>
              </Row>
              <Row label="Devices">
                <Value>
                  {env.inputDevices
                    ? list(
                        ["mouse", env.inputDevices.mouse],
                        ["touch", env.inputDevices.touch],
                        ["keyboard", env.inputDevices.keyboard],
                      )
                    : "not reported"}
                </Value>
              </Row>
              <Row label="Inputs seen">
                <Value>
                  {list(
                    ["mouse", env.mouseSeen],
                    ["touch", env.touchSeen],
                    ["keyboard", env.keyboardSeen],
                  )}
                </Value>
              </Row>
              <Row label="Capabilities">
                <Value>
                  {list(
                    ["hover", capabilities.hover],
                    ["touch", capabilities.touch],
                    ["keyboard nav", capabilities.keyboardNav],
                  )}
                </Value>
              </Row>
              <Row label="Size class">
                <Value>{capabilities.windowSizeClass}</Value>
              </Row>
            </Card>

            <Card title="Policies" layout={{ width: 360 }}>
              <Row label="Interaction policy">
                <Value>{policy.interaction}</Value>
              </Row>
              <Row label="Focus ring">
                <Value>{policy.focusRing ? "visible" : "hidden"}</Value>
              </Row>
              <Row label="App policies">
                <Value>{`${policy.navigation}, ${policy.layout}`}</Value>
              </Row>
              <Divider />
              <Row label="Density policy">
                <Value>{policy.density}</Value>
              </Row>
              <RadioGroup
                value={densityChoice()}
                onChange={chooseDensity}
                layout={{ flexDirection: "row", flexWrap: "wrap", gap: space("md") }}
              >
                <Radio value="auto">Auto</Radio>
                <Radio value="comfortable">Comfortable</Radio>
                <Radio value="compact">Compact</Radio>
                <Radio value="dense">Dense</Radio>
              </RadioGroup>
              <Divider />
              <Row label="Motion policy">
                <Value>{policy.motion}</Value>
              </Row>
              <RadioGroup
                value={motionChoice()}
                onChange={chooseMotion}
                layout={{ flexDirection: "row", flexWrap: "wrap", gap: space("md") }}
              >
                <Radio value="auto">Auto</Radio>
                <Radio value="normal">Normal</Radio>
                <Radio value="reduced">Reduced</Radio>
                <Radio value="none">None</Radio>
              </RadioGroup>
              <Divider />
              <Row label="Text scale">
                <Value>{`${policy.textScale}x`}</Value>
              </Row>
              <RadioGroup
                value={textScaleChoice()}
                onChange={chooseTextScale}
                layout={{ flexDirection: "row", flexWrap: "wrap", gap: space("md") }}
              >
                <Radio value="auto">Auto</Radio>
                <Radio value={0.9}>0.9</Radio>
                <Radio value={1.0}>1.0</Radio>
                <Radio value={1.2}>1.2</Radio>
              </RadioGroup>
            </Card>

            <Card title="Buttons" layout={{ width: 360 }}>
              <View layout={{ flexDirection: "row", flexWrap: "wrap", gap: space("md"), alignItems: "center" }}>
                <Button>Primary</Button>
                <Button variant="secondary">Secondary</Button>
                <Button variant="ghost">Ghost</Button>
                <Button variant="danger">Danger</Button>
                <Button disabled>Disabled</Button>
              </View>
            </Card>

            <Card title="Text input" layout={{ width: 360 }}>
              <TextInput
                value={name()}
                onInput={setName}
                placeholder="Your name"
                layout={{ width: 320 }}
              />
            </Card>

            <Card title="Switch and checkbox" layout={{ width: 360 }}>
              <Row label="Notifications">
                <Switch value={notify()} onChange={setNotify} />
              </Row>
              <Row label="I agree to the terms">
                <Checkbox checked={agree()} onChange={setAgree} />
              </Row>
            </Card>

            <Card title="Radio group" layout={{ width: 360 }}>
              <RadioGroup value={plan()} onChange={setPlan}>
                <Radio value="free">Free</Radio>
                <Radio value="pro">Pro</Radio>
                <Radio value="team">Team</Radio>
              </RadioGroup>
            </Card>

            <Card title="Context menu" layout={{ width: 360 }}>
              <Text muted>
                Right-click (mouse) or long-press (touch) the box below. Touch
                policy presents a bottom sheet, desktop and hybrid a menu at
                the pointer.
              </Text>
              <ContextMenu
                items={[
                  { label: "Copy", onSelect: () => console.log("copy") },
                  { label: "Paste", onSelect: () => console.log("paste") },
                  { label: "Rename", disabled: true },
                  { label: "Delete", onSelect: () => console.log("delete") },
                ]}
              >
                <View
                  layout={{ padding: space("lg"), alignItems: "center" }}
                  style={{
                    backgroundColor: theme.color.surfaceAlt,
                    borderRadius: theme.radius.sm,
                  }}
                >
                  <Text>Secondary actions live here</Text>
                </View>
              </ContextMenu>
            </Card>

            <Card title="Select" layout={{ width: 360 }}>
              <Text muted>
                Forks on the interaction policy: desktop and hybrid anchor a
                dropdown, touch opens a bottom sheet.
              </Text>
              <Select
                value={fruit()}
                onChange={setFruit}
                placeholder="Pick a fruit"
                options={[
                  { value: "apple", label: "Apple" },
                  { value: "banana", label: "Banana" },
                  { value: "cherry", label: "Cherry" },
                  { value: "dragonfruit", label: "Dragonfruit" },
                ]}
                layout={{ width: 200 }}
              />
            </Card>

            <Card title="Slider" layout={{ width: 360 }}>
              <Row label={`Volume: ${Math.round(volume())}`}>
                <Slider
                  value={volume()}
                  onChange={setVolume}
                  min={0}
                  max={100}
                  step={1}
                  layout={{ width: 180 }}
                />
              </Row>
            </Card>

            <Card title="Progress and spinner" layout={{ width: 360 }}>
              <Row label="Loading">
                <Spinner />
              </Row>
              <Text muted>
                Determinate, tracking the volume slider:
              </Text>
              <ProgressBar value={volume() / 100} />
              <Text muted>
                Indeterminate:
              </Text>
              <ProgressBar />
            </Card>

            <Card title="Badges and divider" layout={{ width: 360 }}>
              <Row label="Status">
                <View layout={{ flexDirection: "row", gap: space("md"), alignItems: "center" }}>
                  <Badge>New</Badge>
                  <Badge variant="neutral">3</Badge>
                  <Badge variant="danger">Error</Badge>
                </View>
              </Row>
              <Divider />
              <Text muted>
                A divider separates content within a card.
              </Text>
            </Card>

            <Card title="Tooltip" layout={{ width: 360 }}>
              <Text muted>
                Rest the mouse on a button. Shows under desktop and hybrid
                interaction policies, never under touch.
              </Text>
              <View layout={{ flexDirection: "row", gap: space("md") }}>
                <Tooltip content="Saves your changes">
                  <Button>Hover me</Button>
                </Tooltip>
                <Tooltip content="Appears below the anchor" placement="bottom">
                  <Button>Below</Button>
                </Tooltip>
              </View>
            </Card>

            <Card title="Icons" layout={{ width: 360 }}>
              <Text muted>
                Lucide SVGs drawn through the core svg primitive. currentColor
                follows the theme; the last two are recolored explicitly.
              </Text>
              <View layout={{ flexDirection: "row", gap: space("lg"), alignItems: "center" }}>
                <Icon src={HOUSE} />
                <Icon src={SETTINGS} />
                <Icon src={BELL} />
                <Icon src={HEART} size={32} color={theme.color.danger} />
                <Icon src={STAR} size={32} color={theme.color.primary} />
              </View>
            </Card>

            <Card title="QR code" layout={{ width: 360 }}>
              <Text muted>
                Encodes your name, or a link if the field is empty.
              </Text>
              <View layout={{ alignItems: "center" }}>
                <QrCode data={qrData() || "https://solidjs.com"} />
              </View>
            </Card>
          </View>
        </ScrollView>
      </SafeArea>
    </Window>
  )
}

render(() => <App />)
