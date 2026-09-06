import {
  render,
  createSignal,
  createEffect,
  untrack,
  env,
  capabilities,
  For,
  Show,
  pct,
} from "@solidrt/core"
import {
  policy,
  setPolicy,
  type DensityPolicy,
  type MotionPolicy,
  Window,
  View,
  Text,
  Image,
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
  SplitView,
  Item,
  Pressable,
  type PressState,
  Modal,
  SegmentedControl,
  Field,
  Density,
  theme,
  space,
  typeWeight,
  setTheme,
  darkTheme,
  lightTheme,
  SafeArea,
} from "@solidrt/components"

import icon from "../assets/icon.png" with { type: "binary" }
// The puzzle mark, the demos project's app icon: a multi-color SVG keeps its
// own fills through Icon.
import mark from "../assets/icon.svg" with { type: "text" }

// A gallery of @solidrt/components, itself built from them: a SplitView whose
// list pane picks a group of components and whose detail pane shows that
// group's cards. Two-pane on a wide window, one pane at a time (with a Back
// button) on a narrow one - resize to watch policy.layout flip the shell.
// The whole thing recolors live when the theme switches: every component
// reads theme.* reactively, so the "Dark theme" switch reflows the palette
// with no remount. The "Environment" group shows the environment ->
// capabilities -> policies cascade and lets you force density/motion
// overrides to watch every control adapt.

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
const SUN = LUCIDE(
  `<circle cx="12" cy="12" r="4"/><path d="M12 2v2"/><path d="M12 20v2"/>` +
    `<path d="m4.93 4.93 1.41 1.41"/><path d="m17.66 17.66 1.41 1.41"/><path d="M2 12h2"/>` +
    `<path d="M20 12h2"/><path d="m6.34 17.66-1.41 1.41"/><path d="m19.07 4.93-1.41 1.41"/>`,
)
const MOON = LUCIDE(`<path d="M12 3a6 6 0 0 0 9 9 9 9 0 1 1-9-9Z"/>`)
const ARROW_LEFT = LUCIDE(`<path d="m12 19-7-7 7-7"/><path d="M19 12H5"/>`)
const BELL = LUCIDE(
  `<path d="M10.268 21a2 2 0 0 0 3.464 0"/>` +
    `<path d="M3.262 15.326A1 1 0 0 0 4 17h16a1 1 0 0 0 .74-1.673C19.41 13.956 18 12.499 18 8A6 6 0 0 0 6 8c0 4.499-1.411 5.956-2.738 7.326"/>`,
)

const TAP_TARGET = 44
// Breathing room between the list's cards and the scroller's clip edge, so a
// focus ring on the outermost card is not cut off.
const LIST_GUTTER = 2
// Reading widths, as the player sets them: single-pane runs up to the
// expanded breakpoint (840), so past these the column is centered rather than
// stretched. The detail column is wider than the list's because it holds
// label-and-value rows and controls, not prose.
const COLUMN_MAX_WIDTH = 440
const DETAIL_MAX_WIDTH = 640

// An icon-only press target, as the player's back arrow: a square tap
// target with a hover tint and no fill, no label.
function IconButton(props: { src: string; onPress: () => void }) {
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
        backgroundColor: s.hovered ? theme.color.overlayHover : "transparent",
        borderRadius: theme.radius.md,
      })}
    >
      <Icon src={props.src} size={22} />
    </Pressable>
  )
}

// A group in the list, shaped like the player's app rows: a pressable Card
// with a title and a muted line, filled while selected or hovered. Only
// two-pane shows the selection - single-pane shows the list without the
// detail, and a highlighted row with nothing selected on screen reads as a
// stray highlight.
function GroupCard(props: {
  label: string
  description: string
  active: boolean
  onPress: () => void
}) {
  return (
    <Pressable focusable onPress={props.onPress}>
      {(s: PressState) => (
        <Card
          layout={{ flexDirection: "row", alignItems: "center", gap: space("lg") }}
          style={{
            backgroundColor:
              props.active || s.hovered ? theme.color.surfaceAlt : theme.color.surface,
          }}
        >
          <View layout={{ flexDirection: "column", flexGrow: 1, gap: 2 }}>
            <Text variant="title">{props.label}</Text>
            <Text variant="body" muted>
              {props.description}
            </Text>
          </View>
        </Card>
      )}
    </Pressable>
  )
}

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
  let [notes, setNotes] = createSignal("")
  let [fixedNotes, setFixedNotes] = createSignal("")
  let [range, setRange] = createSignal<unknown>("week")
  // Starts false: a Modal is a portal, and portals cannot mount during the
  // initial render.
  let [modalOpen, setModalOpen] = createSignal(false)
  let [rowDensity, setRowDensity] = createSignal<unknown>("comfortable")
  let [currentIssue, setCurrentIssue] = createSignal("SRT-12")
  let issues = [
    { id: "SRT-12", title: "Tooltip lingers after the anchor unmounts", assignee: "Antoine" },
    { id: "SRT-15", title: "Select sheet ignores safe area on Android", assignee: "unassigned" },
    { id: "SRT-19", title: "Slider thumb misses the first pixel", assignee: "Antoine" },
  ]

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
    let on = !dark()
    setDark(on)
    setTheme(on ? darkTheme : lightTheme)
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
  let [weightDeltaChoice, setWeightDeltaChoice] = createSignal<unknown>("auto")
  let chooseWeightDelta = (v: unknown) => {
    setWeightDeltaChoice(v)
    setPolicy({ textWeightDelta: v === "auto" ? undefined : (v as number) })
  }

  // The gallery's groups: one list row each, one set of cards each. The
  // cards read the signals above, so a group's content is built fresh each
  // time it is shown (the detail reads content() once per selection) and
  // picks up the current state.
  let sections = [
    {
      value: "environment",
      label: "Environment",
      description: "Facts, capabilities and policies",
      content: () => (
        <>
          <Card title="Environment">
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
          <Card title="Policies">
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
              <Radio value={1.1}>1.1</Radio>
            </RadioGroup>
            <Divider />
            <Row label="Text weight delta">
              <Value>
                {`base +${policy.textWeightDelta}, body ${typeWeight(
                  theme.text.body.weight,
                  theme.text.body.size * policy.textScale,
                )}, title ${typeWeight(theme.text.title.weight, theme.text.title.size * policy.textScale)}`}
              </Value>
            </Row>
            <RadioGroup
              value={weightDeltaChoice()}
              onChange={chooseWeightDelta}
              layout={{ flexDirection: "row", flexWrap: "wrap", gap: space("md") }}
            >
              <Radio value="auto">Auto</Radio>
              <Radio value={0}>0</Radio>
              <Radio value={100}>+100</Radio>
              <Radio value={200}>+200</Radio>
            </RadioGroup>
          </Card>
        </>
      ),
    },
    {
      value: "buttons",
      label: "Buttons and badges",
      description: "Buttons, badges, dividers, icons",
      content: () => (
        <>
          <Card title="Buttons">
            <View
              layout={{
                flexDirection: "row",
                flexWrap: "wrap",
                gap: space("md"),
                alignItems: "center",
              }}
            >
              <Button>Primary</Button>
              <Button variant="secondary">Secondary</Button>
              <Button variant="ghost">Ghost</Button>
              <Button variant="danger">Danger</Button>
              <Button disabled>Disabled</Button>
            </View>
          </Card>
          <Card title="Badges and divider">
            <Row label="Status">
              <View layout={{ flexDirection: "row", gap: space("md"), alignItems: "center" }}>
                <Badge>New</Badge>
                <Badge variant="neutral">3</Badge>
                <Badge variant="danger">Error</Badge>
              </View>
            </Row>
            <Divider />
            <Text muted>A divider separates content within a card.</Text>
          </Card>
          <Card title="Pressable">
            <Text muted>
              The primitive under every button: style and children can read the live press state.
            </Text>
            <Pressable
              onPress={() => console.log("pressed")}
              layout={{ padding: space("lg"), alignItems: "center" }}
              style={(s: PressState) => ({
                backgroundColor: s.pressed
                  ? theme.color.primary
                  : s.hovered
                    ? theme.color.surfaceAlt
                    : theme.color.surface,
                borderRadius: theme.radius.md,
              })}
            >
              {(s: PressState) => (
                <Text color={s.pressed ? "onPrimary" : "text"}>
                  {s.pressed ? "Pressed" : s.hovered ? "Hovered" : "Idle"}
                </Text>
              )}
            </Pressable>
          </Card>
          <Card title="Icons">
            <Text muted>
              Lucide SVGs parsed by the core parseSvg primitive into d-path draws. currentColor
              follows the theme; the last two are recolored explicitly.
            </Text>
            <View layout={{ flexDirection: "row", gap: space("lg"), alignItems: "center" }}>
              <Icon src={HOUSE} />
              <Icon src={SETTINGS} />
              <Icon src={BELL} />
              <Icon src={HEART} color={theme.color.danger} />
              <Icon src={STAR} color={theme.color.primary} />
            </View>
          </Card>
        </>
      ),
    },
    {
      value: "selection",
      label: "Selection",
      description: "Toggles, radio, segments, select",
      content: () => (
        <>
          <Card title="Switch and checkbox">
            <Row label="Notifications">
              <Switch value={notify()} onChange={setNotify} />
            </Row>
            <Row label="I agree to the terms">
              <Checkbox checked={agree()} onChange={setAgree} />
            </Row>
          </Card>
          <Card title="Radio group">
            <RadioGroup value={plan()} onChange={setPlan}>
              <Radio value="free">Free</Radio>
              <Radio value="pro">Pro</Radio>
              <Radio value="team">Team</Radio>
            </RadioGroup>
          </Card>
          <Card title="Segmented control">
            <SegmentedControl
              options={[
                { value: "day", label: "Day" },
                { value: "week", label: "Week" },
                { value: "month", label: "Month" },
              ]}
              value={range()}
              onChange={setRange}
            />
          </Card>
          <Card title="Select">
            <Text muted>
              Forks on the interaction policy: desktop and hybrid anchor a dropdown, touch opens a
              bottom sheet.
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
        </>
      ),
    },
    {
      value: "text",
      label: "Text",
      description: "A formatted paragraph and text fields",
      content: () => (
        <>
          <Card title="Paragraph">
            <Text>
              Lorem ipsum dolor sit amet,{" "}
              <span textDecoration="underline">consectetur adipiscing elit</span>, sed do eiusmod
              tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis
              nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat. Duis
              aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu fugiat
              nulla pariatur. Excepteur sint occaecat cupidatat non proident, <span fontStyle="italic">sunt in culpa qui
              officia deserunt mollit anim id est laborum</span>.
            </Text>
          </Card>
          <Card title="Text input">
            <Field label="Single line" description="Enter submits.">
              <TextInput
                value={name()}
                onInput={setName}
                onSubmit={(v) => console.log("submitted", v)}
                placeholder="Your name"
                layout={{ width: pct(100) }}
              />
            </Field>
            <Field
              label="Multiline"
              description="A layout.height pins the box; it scrolls to the caret."
            >
              <TextInput
                multiline
                value={fixedNotes()}
                onInput={setFixedNotes}
                placeholder="Notes in a fixed box"
                layout={{ width: pct(100), height: 72 }}
              />
            </Field>
            <Field
              label="Multiline, auto-grow"
              description="Grows with the text up to 4 rows, then scrolls."
            >
              <TextInput
                multiline
                maxRows={4}
                value={notes()}
                onInput={setNotes}
                placeholder="Notes"
                layout={{ width: pct(100) }}
              />
            </Field>
          </Card>
        </>
      ),
    },
    {
      value: "slider",
      label: "Slider and progress",
      description: "Slider, progress bar, spinner",
      content: () => (
        <>
          <Card title="Slider">
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
          <Card title="Progress and spinner">
            <Row label="Loading">
              <Spinner />
            </Row>
            <Text muted>Determinate, tracking the volume slider:</Text>
            <ProgressBar value={volume() / 100} />
            <Text muted>Indeterminate:</Text>
            <ProgressBar />
          </Card>
        </>
      ),
    },
    {
      value: "overlays",
      label: "Overlays",
      description: "Context menu, tooltip and modal",
      content: () => (
        <>
          <Card title="Context menu">
            <Text muted>
              Right-click (mouse) or long-press (touch) the box below. Touch policy presents a
              bottom sheet, desktop and hybrid a menu at the pointer.
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
          <Card title="Modal">
            <Text muted>
              A centered card over a scrim, portaled to the window root. The backdrop closes it.
            </Text>
            <Button onPress={() => setModalOpen(true)}>Open modal</Button>
            <Show when={modalOpen()}>
              <Modal onClose={() => setModalOpen(false)}>
                <Card title="Modal" layout={{ width: 360 }}>
                  <Text muted>Focus navigation stays inside while this is open.</Text>
                  <Button onPress={() => setModalOpen(false)}>Close</Button>
                </Card>
              </Modal>
            </Show>
          </Card>
          <Card title="Tooltip">
            <Text muted>
              Rest the mouse on a button. Shows under desktop and hybrid interaction policies, never
              under touch.
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
        </>
      ),
    },
    {
      value: "lists",
      label: "Lists",
      description: "Item rows under a Density region",
      content: () => (
        <>
          <Card title="Items">
            <Text muted>
              A column of Item rows is the list; the segments set the Density of the region around
              them.
            </Text>
            <SegmentedControl
              options={[
                { value: "comfortable", label: "Comfortable" },
                { value: "compact", label: "Compact" },
                { value: "dense", label: "Dense" },
              ]}
              value={rowDensity()}
              onChange={setRowDensity}
            />
            <Density value={rowDensity() as DensityPolicy}>
              <View layout={{ flexDirection: "column" }}>
                <For each={issues}>
                  {(issue) => (
                    <Item
                      startContent={<Icon src={BELL} />}
                      label={issue.title}
                      description={issue.assignee}
                      endContent={<Badge variant="neutral">{issue.id}</Badge>}
                      selected={issue.id === currentIssue()}
                      onPress={() => setCurrentIssue(issue.id)}
                    />
                  )}
                </For>
                <Item
                  label="Disabled row"
                  description="Takes no pointer events"
                  disabled
                  onPress={() => {}}
                />
              </View>
            </Density>
          </Card>
        </>
      ),
    },
    {
      value: "misc",
      label: "Misc",
      description: "QR code and image",
      content: () => (
        <>
          <Card title="QR code">
            <View layout={{ alignItems: "center" }}>
              <QrCode data="https://solidjs.com" />
            </View>
          </Card>
          <Card title="Image">
            <Text muted>
              Fetches, decodes, and uploads an image, then shows it as a GPU texture. Rounded via a
              clipping border radius.
            </Text>
            <View layout={{ alignItems: "center" }}>
              <Image src={icon} style={{ borderRadius: theme.radius.md }} />
            </View>
          </Card>
        </>
      ),
    },
  ]

  // Selection lives here, not in the panes: SplitView remounts them when the
  // layout policy crosses a breakpoint. showDetail only matters single-pane.
  // Nothing starts selected: a preselected row with no visible detail reads
  // as a stray highlight in single-pane.
  let [current, setCurrent] = createSignal<string | undefined>(undefined)
  let [showDetail, setShowDetail] = createSignal(false)
  let open = (value: string) => {
    setCurrent(value)
    setShowDetail(true)
  }
  let section = () => sections.find((s) => s.value === current())
  let twoPane = () => policy.layout === "twoPane"

  return (
    <Window
      title="Components gallery"
      layout={{ flexDirection: "column" }}
      style={{ backgroundColor: theme.color.background }}
    >
      <SafeArea>
        <SplitView
          layout={{ flex: 1 }}
          listWidth={380}
          showDetail={showDetail()}
          list={
            <View layout={{ flexGrow: 1, flexDirection: "column", alignItems: "center" }}>
              <View
                layout={{
                  flexDirection: "column",
                  flexGrow: 1,
                  width: pct(100),
                  maxWidth: twoPane() ? undefined : COLUMN_MAX_WIDTH,
                  padding: space("xl"),
                  gap: space("xl"),
                }}
              >
                <View
                  layout={{
                    flexDirection: "row",
                    justifyContent: "space-between",
                    alignItems: "center",
                  }}
                >
                  <View layout={{ flexDirection: "row", alignItems: "center", gap: space("md") }}>
                    <Icon src={mark} size={40} />
                    <Text variant="heading">Gallery</Text>
                  </View>
                  <Tooltip content={dark() ? "Light theme" : "Dark theme"}>
                    <IconButton src={dark() ? SUN : MOON} onPress={toggleTheme} />
                  </Tooltip>
                </View>
                <ScrollView layout={{ flexGrow: 1 }}>
                  <View
                    layout={{ flexDirection: "column", gap: space("md"), padding: LIST_GUTTER }}
                  >
                    <For each={sections}>
                      {(s) => (
                        <GroupCard
                          label={s.label}
                          description={s.description}
                          active={twoPane() && s.value === current()}
                          onPress={() => open(s.value)}
                        />
                      )}
                    </For>
                  </View>
                </ScrollView>
              </View>
            </View>
          }
          detail={
            <Show
              when={section()}
              fallback={
                <View layout={{ flexGrow: 1, alignItems: "center", justifyContent: "center" }}>
                  <Text variant="heading" muted>
                    Pick a group
                  </Text>
                </View>
              }
            >
              {(s) => (
                <ScrollView layout={{ flexGrow: 1 }}>
                  <View layout={{ flexGrow: 1, alignItems: twoPane() ? "flex-start" : "center" }}>
                    <View
                      layout={{
                        flexDirection: "column",
                        gap: space("lg"),
                        width: pct(100),
                        maxWidth: DETAIL_MAX_WIDTH,
                        padding: space("xl"),
                      }}
                    >
                      <View
                        layout={{ flexDirection: "row", alignItems: "center", gap: space("md") }}
                      >
                        <Show when={!twoPane()}>
                          <IconButton src={ARROW_LEFT} onPress={() => setShowDetail(false)} />
                        </Show>
                        <View layout={{ flexDirection: "column", flexGrow: 1, gap: 2 }}>
                          <Text variant="heading">{s().label}</Text>
                          <Text variant="body" muted>
                            {s().description}
                          </Text>
                        </View>
                      </View>
                      {s().content()}
                    </View>
                  </View>
                </ScrollView>
              )}
            </Show>
          }
        />
      </SafeArea>
    </Window>
  )
}

render(() => <App />)
