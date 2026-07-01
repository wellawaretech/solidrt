# @solidrt/components - agent notes

Higher-level components built on @solidrt/core primitives. Optional: an app can
be built with core primitives alone. For the underlying element model, events,
reactivity, and how to run/verify, see @solidrt/core and @solidrt/cli (their
AGENTS.md). Full prop tables are in this package's README.

## Install

```sh
bun add @solidrt/components   # peers: @solidrt/core, @solidjs/signals
```

## Props: layout vs style (the non-obvious split)

Most components group props into two objects, plus top-level event handlers:

- `layout={{...}}` - the core LayoutProps: flex/grid, sizing, padding/margin,
  position. For `Text`, also the font fields (fontSize, fontWeight, ...).
  Changing these relayouts.
- `style={{...}}` - paint only, never affects layout: `backgroundColor`,
  `borderColor`, `borderWidth`, `borderRadius`, `color` (Text), and the
  transform `x`/`y`/`rotate`/`scale`.
- Event handlers (`onPointerDown`, `onKeyDown`, ...) are top-level props, NOT
  inside `layout`/`style`.

## Exports

- `Window` - root surface; renders a core `<window>`, so `render()` accepts it.
  Applies `layout` and `style.backgroundColor` only (a window cannot be
  transformed or bordered). Also: `title`, `fullscreen`, `vsync`, `fps`.
- `View` - general box; draws a background/border when the matching `style`
  props are set.
- `Text` - text in a layout box; font fields go in `layout`, `color` in `style`.
- `Image` - fetches/decodes/uploads an image: `src: string | Uint8Array`.
- `TextInput` - single-line input; `value`/`onInput`/`onSubmit`, controlled or
  uncontrolled, plus `placeholder`, `maxLength`, `autoFocus`, `disabled`.
- `ScrollView` - scrollable region; vertical by default, `horizontal` to flip.
  Wheel + drag, no momentum yet. Backed by `createScroll` from
  `@solidrt/core/scroll` (headless offset+clamp geometry).
- `Pressable` - pressable box; `onPress` on a primary press released inside,
  `disabled` opts out. `children`/`style` can be functions of `{ pressed,
  hovered }`. No pointer capture: a drag out cancels via onPointerLeave.
- `Button` - themed Pressable: accent box + label (string/number child), scales
  on press (via reactive style); colors from `theme.color.primary`/`onPrimary`.
- `Switch` - on/off toggle; `value`/`onChange`, controlled or uncontrolled
  (`defaultValue`). Track fills `primary`/`surfaceAlt`, thumb slides.
- `Checkbox` - `checked`/`onChange` (or `defaultChecked`); checkmark drawn via a
  core `<d-path>` when checked.
- `RadioGroup` + `Radio` - single-selection pair; the group owns
  `value`/`onChange` (or `defaultValue`) and shares it to `Radio` children via a
  module-local context (not a cross-component dependency). `Radio value=...`;
  string/number child renders as a themed label.
- `Slider` - horizontal; `value`/`onChange` (or `defaultValue`), `min`/`max`/
  `step`. Pointer x mapped to value via `getBoundingBox`; uses core
  `setPointerCapture` so a drag keeps tracking off the track (within the window).
- `Card` - themed surface container: padded column box, `surface` fill, `border`
  stroke, rounded. Optional `title` heading; override paint via `style`.
- `Divider` - thin rule in `border` color; `orientation` (default horizontal),
  `thickness`. Stretches across the cross axis (full width in a column).
- `Badge` - small rounded pill for counts/labels/status; `primary`/`onPrimary`
  by default, override via `style.backgroundColor`/`style.color`.
- `Spinner` - indeterminate rotating arc, driven by core `onFrame`; `size`,
  `thickness`, `speed` (rev/s). Color from `primary`, override via `style.color`.
- `ProgressBar` - horizontal; determinate with `value` in [0,1] (fill grows from
  the left), indeterminate when `value` is undefined (segment slides, `onFrame`).
  Track `surfaceAlt` / fill `primary`; override via `style.backgroundColor`/`color`.
- `QrCode` - QR for `data: string`, built from primitives (same-color row runs
  merged into one box on a light panel); grid memoized on `data`/`level`.
  `moduleSize`/`margin`/`radius`/`level` (L/M/Q/H). Paints black-on-white by
  default (NOT the theme) to stay scannable; override `color`/`background` only
  if contrast holds. Deps on `qrcode-generator`.
- `SafeArea` - pads children clear of system UI (notches, status bars); top and
  bottom on by default, pass `false`/a number per edge.
- `theme` / `setTheme` / `darkTheme` / `lightTheme` - shared REACTIVE appearance
  (backed by a Solid store, so reads are tracked and switching recolors the live
  UI). `setTheme(lightTheme)`/`setTheme(darkTheme)` switch presets;
  `setTheme({...})` merges a one-level-deep override. Default is dark. Color
  tokens: `background`, `surface`, `surfaceAlt`, `text`, `textMuted`, `border`,
  `primary`, `onPrimary`, `danger`, `scrim`.

## Minimal app (verified to render)

```tsx
import { render } from "@solidrt/core"
import { Window, View, Text } from "@solidrt/components"
import { createSignal } from "@solidjs/signals"

function App() {
  let [count, setCount] = createSignal(0)
  return (
    <Window title="App" style={{ backgroundColor: "#0b0f17" }}
      layout={{ flexDirection: "column", alignItems: "center", justifyContent: "center", gap: 24 }}>
      <Text layout={{ fontSize: 48, fontWeight: 800 }} style={{ color: "#1f6feb" }}>
        {count()}
      </Text>
      <View onPointerDown={() => setCount((c) => c + 1)}
        layout={{ padding: 16 }} style={{ backgroundColor: "#1f6feb", borderRadius: 12 }}>
        <Text style={{ color: "#ffffff" }}>increment</Text>
      </View>
    </Window>
  )
}

render(() => <App />)
```
