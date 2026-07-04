# @solidrt/components

A collection of components for [SolidRT](https://github.com/wellawaretech/solidrt) apps.

> LLM agents: see [AGENTS.md](./AGENTS.md) for a dense, self-contained quickstart.

## Installation

```sh
bun add @solidrt/components
```

## Theming

Appearance (colors, spacing, border, font size) is controlled via a shared, reactive theme. Reads are tracked, so switching the theme at runtime recolors the live UI without remounting.

Two presets ship out of the box, `darkTheme` and `lightTheme`. The default is dark. Switch by passing a full preset, or apply a targeted override with a partial:

```jsx
import { setTheme, darkTheme, lightTheme } from "@solidrt/components"

setTheme(lightTheme)                          // switch to light
setTheme(darkTheme)                           // switch to dark
setTheme({ color: { primary: "#ff2d55" } })   // override one token
```

`setTheme` merges one level deep per category, so a partial only touches the keys you pass.

The color tokens are `background` (window fill), `surface` (control/card fill), `surfaceAlt` (subtle raised/track fill), `text`, `textMuted`, `border`, `primary`, `onPrimary`, `danger`, and `scrim` (modal dim). Non-color tokens are `spacing`, `radius`, `borderWidth`, and `text` (font sizes), shared across presets.

## Policies

Theme answers "how does it look"; policies answer "how does it behave". Policies are a second reactive layer, derived from the platform facts in `@solidrt/core` (`capabilities`, `env`), so components adapt to touch vs. desktop, window size, and display without every app wiring that logic itself.

```jsx
import { policy, setPolicy, densityScale } from "@solidrt/components"

policy.interaction   // touch vs. desktop affordances (hover, long-press, ...)
policy.density       // control/spacing scale
```

`policy` fields:

| Field             | Type                                          | Description                                                                 |
| ----------------- | ---------------------------------------------- | ----------------------------------------------------------------------------- |
| `interaction`     | `"touch" \| "desktop" \| "hybrid"`             | Which interaction affordances a component shows (hover states vs. long-press). |
| `density`         | `"comfortable" \| "compact" \| "dense"`        | Control/hit-target/spacing scale; see `densityScale()`.                       |
| `motion`          | `"normal" \| "reduced" \| "none"`              | Animation intensity.                                                          |
| `focusRing`       | `boolean`                                      | Whether focused controls draw a visible focus indicator.                      |
| `textScale`       | `number`                                       | Multiplier on type-scale font sizes; defaults to the OS text-scale preference. |
| `textWeightDelta` | `number`                                       | Weight compensation (in steps of 100) for light-on-dark text on low-DPI displays. |
| `navigation`      | `"bottomTabs" \| "rail" \| "sidebar"`          | Recommended nav layout, derived from window size class.                       |
| `layout`          | `"singlePane" \| "twoPane"`                    | Recommended single vs. two-pane layout, derived from window size class.       |

Reads are reactive like `theme`, so a window resize or the first mouse move on a touch-capable device updates every consuming component live.

```jsx
setPolicy({ density: "compact" })    // pin a field, overriding the derived value
setPolicy({ density: undefined })    // hand it back to the resolver
```

`setPolicyResolver((caps) => Policies)` replaces the whole system-derivation function for full custom control; `defaultPolicyResolver` is exported to wrap or extend instead of replacing it outright.

`densityScale()` is a reactive multiplier (1 / 0.85 / 0.7 for comfortable/compact/dense) driven by `policy.density`, used internally for spacing and hit-target sizing.

## Layout and style

Most components group their props into two objects:

- `layout` - properties that feed the layout engine (flexbox/grid, sizing, padding, margin, position). Changing them triggers a relayout. This is the core `LayoutProps` set.
- `style` - paint-only properties that never affect layout: `color`, `backgroundColor`, `borderColor`, `borderWidth`, `borderRadius`, and the transform `x`, `y`, `rotate`, `scale`.

Event handlers (`onPointerDown`, `onKeyDown`, etc.) are passed directly as top-level props.

`StyleProps`:

| Prop              | Type                                       | Description                          |
| ----------------- | ------------------------------------------ | ------------------------------------ |
| `color`           | `string`                                   | Text color (used by `Text`).         |
| `backgroundColor` | `string`                                   | Fill color.                          |
| `borderColor`     | `string`                                   | Stroke color (when `borderWidth` set). |
| `borderWidth`     | `number`                                   | Border stroke width.                 |
| `borderRadius`    | `number \| [number, number, number, number]` | Corner radius.                     |
| `x` / `y`         | `number`                                   | Translation offset.                  |
| `rotate`          | `number`                                   | Rotation.                            |
| `scale`           | `number`                                   | Scale factor.                        |

## Components

### Window

The root surface of an app. Accepts `layout` and the `backgroundColor` from `style`; a window cannot be transformed or bordered.

```jsx
import { Window } from "@solidrt/components"

function App() {
  return (
    <Window title="My App" style={{ backgroundColor: "#111" }}>
      {/* ... */}
    </Window>
  )
}
```

**Props**

| Prop         | Type          | Default | Description                          |
| ------------ | ------------- | ------- | ------------------------------------ |
| `title`      | `string`      | -       | Window title.                        |
| `fullscreen` | `boolean`     | -       | Open fullscreen.                     |
| `vsync`      | `boolean`     | -       | Enable vsync.                        |
| `fps`        | `boolean`     | -       | Show an FPS counter.                 |
| `layout`     | `LayoutProps` | -       | Layout properties.                   |
| `style`      | `StyleProps`  | -       | Only `backgroundColor` is applied.   |
| `children`   | `any`         | -       | Content.                             |

### View

A general-purpose box. Spreads `layout` onto the underlying view, applies the transform from `style`, and draws a background and/or border when those style props are set.

```jsx
import { View } from "@solidrt/components"

<View
  layout={{ padding: 16, flexDirection: "column", gap: 8 }}
  style={{ backgroundColor: "#222", borderRadius: 8 }}
>
  {/* ... */}
</View>
```

**Props**

Accepts all pointer event props, plus:

| Prop       | Type                          | Description           |
| ---------- | ----------------------------- | --------------------- |
| `layout`   | `LayoutProps`                 | Layout properties.    |
| `style`    | `StyleProps`                  | Paint properties.     |
| `ref`      | `(node: { id: number }) => void` | Node reference.    |
| `children` | `any`                         | Content.              |

### Text

Renders text inside a layout box. Font properties live in `layout` (they affect measurement); `color` lives in `style`.

```jsx
import { Text } from "@solidrt/components"

<Text layout={{ fontSize: 18, maxLines: 2 }} style={{ color: "#fff" }}>
  Hello
</Text>
```

`layout` accepts all `LayoutProps` plus the font fields `fontFamily`, `fontSize`, `lineHeight`, `fontStyle`, `fontWeight`, `textAlign`, and `maxLines`.

**Props**

Accepts all pointer event props, plus:

| Prop       | Type                          | Description                       |
| ---------- | ----------------------------- | --------------------------------- |
| `layout`   | `TextLayoutProps`             | Layout properties plus font fields. |
| `style`    | `StyleProps`                  | `color` and transform.            |
| `ref`      | `(node: { id: number }) => void` | Node reference.                |
| `children` | `any`                         | Text content.                     |

### Image

Loads and displays an image from a URL or raw bytes.

```jsx
import { Image } from "@solidrt/components"

function Avatar() {
  return <Image src="https://example.com/avatar.png" layout={{ width: 64, height: 64 }} />
}
```

**Props**

Accepts `layout`, `style`, and all pointer event props, plus:

| Prop  | Type                   | Description                                |
| ----- | ---------------------- | ------------------------------------------ |
| `src` | `string \| Uint8Array` | URL to fetch, or raw image bytes to decode |

### TextInput

Single-line text input.

```jsx
import { TextInput } from "@solidrt/components"
import { createSignal } from "@solidjs/signals"

function NameField() {
  let [name, setName] = createSignal("")
  return (
    <TextInput
      value={name()}
      onInput={setName}
      onSubmit={(v) => console.log("submitted", v)}
      placeholder="Your name"
      layout={{ width: 240 }}
    />
  )
}
```

**Props**

| Prop           | Type                      | Default | Description                                                  |
| -------------- | ------------------------- | ------- | ------------------------------------------------------------ |
| `value`        | `string`                  | -       | Controlled value. If omitted, the component is uncontrolled. |
| `defaultValue` | `string`                  | `""`    | Initial value for uncontrolled use.                          |
| `onInput`      | `(value: string) => void` | -       | Fires on every change.                                       |
| `onSubmit`     | `(value: string) => void` | -       | Fires on Enter.                                              |
| `onFocus`      | `() => void`              | -       | Fires when the field gains focus.                            |
| `onBlur`       | `() => void`              | -       | Fires when the field loses focus.                            |
| `placeholder`  | `string`                  | -       | Shown when value is empty and the field is not focused.      |
| `maxLength`    | `number`                  | -       | Truncates input to this length.                              |
| `disabled`     | `boolean`                 | `false` | Ignores pointer and key events when true.                    |
| `autoFocus`    | `boolean`                 | `false` | Focuses on mount.                                            |
| `layout`       | `LayoutProps`             | -       | Layout properties (e.g. `width`).                            |
| `style`        | `StyleProps`              | -       | Overrides theme colors, border, and radius.                  |

### SafeArea

Wraps its children in a view that applies padding to avoid system UI intrusions (status bars, home indicators, notches, etc.).

```jsx
import { SafeArea } from "@solidrt/components"

function App() {
  return (
    <Window>
      <SafeArea top bottom>
        <Text>Content clear of system UI</Text>
      </SafeArea>
    </Window>
  )
}
```

Top and bottom insets are applied by default. Pass `false` to opt out of an edge, or a number to apply the inset with a minimum padding.

```jsx
// top only
<SafeArea bottom={false}>

// apply top and bottom insets, with a minimum of 16px each
<SafeArea top={16} bottom={16}>

// all four edges
<SafeArea top bottom left right>
```

**Props**

| Prop       | Type                | Default | Description                                            |
| ---------- | ------------------- | ------- | ------------------------------------------------------ |
| `top`      | `boolean \| number` | `true`  | Apply top inset. A number sets the minimum padding.    |
| `bottom`   | `boolean \| number` | `true`  | Apply bottom inset. A number sets the minimum padding. |
| `left`     | `boolean \| number` | `false` | Apply left inset. A number sets the minimum padding.   |
| `right`    | `boolean \| number` | `false` | Apply right inset. A number sets the minimum padding.  |
| `children` | `any`               | -       | Content to render inside the safe area.                |

### ScrollView

A scrollable region. Scrolls vertically by default; pass `horizontal` to scroll the other axis instead. Both the wheel and dragging scroll the content. There is no momentum/fling yet, and (with no pointer capture) a drag that leaves the box ends the gesture.

```jsx
import { ScrollView, Text } from "@solidrt/components"
import { For } from "@solidrt/core"

<ScrollView layout={{ height: 300 }} style={{ backgroundColor: "#111", borderRadius: 8 }}>
  <For each={items()}>{(item) => <Text style={{ color: "#fff" }}>{item}</Text>}</For>
</ScrollView>
```

**Props**

Accepts all pointer event props, plus:

| Prop         | Type                             | Description                                  |
| ------------ | -------------------------------- | -------------------------------------------- |
| `horizontal` | `boolean`                        | Scroll the horizontal axis instead of vertical. |
| `layout`     | `LayoutProps`                    | Layout of the outer box (e.g. `height`).     |
| `style`      | `StyleProps`                     | Background, border, and transform.           |
| `ref`        | `(node: { id: number }) => void` | Reference to the outer box.                   |
| `children`   | `any`                            | Scrollable content.                          |

The underlying geometry primitive `createScroll` is available from `@solidrt/core/scroll` for building custom scrollers.

### Pressable

A pressable box. `onPress` fires on a primary-button press released over the box; a drag out of the box (or a non-primary button) does not fire it. `children` and `style` may each be a function of the `{ pressed, hovered }` state, so the box can restyle on press/hover without extra signals.

```jsx
import { Pressable, Text } from "@solidrt/components"

<Pressable
  onPress={() => setCount((c) => c + 1)}
  layout={{ padding: 12 }}
  style={(s) => ({ backgroundColor: s.pressed ? "#333" : "#222", borderRadius: 8 })}
>
  <Text style={{ color: "#fff" }}>Tap me</Text>
</Pressable>
```

**Props**

Accepts all pointer event props, plus:

| Prop       | Type                                                | Description                                  |
| ---------- | --------------------------------------------------- | -------------------------------------------- |
| `onPress`  | `() => void`                                        | Fires on a completed press.                  |
| `disabled` | `boolean`                                           | Takes no pointer events when true.           |
| `layout`   | `LayoutProps`                                        | Layout properties.                           |
| `style`    | `StyleProps \| (state) => StyleProps`               | Paint properties, or a function of state.    |
| `children` | `any \| (state) => any`                             | Content, or a function of state.             |
| `ref`      | `(node: { id: number }) => void`                    | Node reference.                              |

### Button

Themed convenience over `Pressable`: a padded, centered, accent-colored box with a label that scales slightly on press. A string or number child is rendered as the themed label; any other child renders as-is. Colors come from the theme (`color.primary`, `color.onPrimary`); override per-button via `style`.

```jsx
import { Button } from "@solidrt/components"

<Button onPress={save} layout={{ minWidth: 120 }}>Save</Button>
```

**Props**

| Prop       | Type          | Description                                    |
| ---------- | ------------- | ---------------------------------------------- |
| `onPress`  | `() => void`  | Fires on a completed press.                    |
| `disabled` | `boolean`     | Mutes colors and ignores presses.              |
| `layout`   | `LayoutProps` | Overrides padding/sizing.                      |
| `style`    | `StyleProps`  | Overrides background, radius, etc.             |
| `children` | `any`         | Label text, or custom content.                 |

### Switch

An on/off toggle. The track fills with `primary` when on and `surfaceAlt` when off; the thumb slides across. Controlled via `value`/`onChange`, or uncontrolled via `defaultValue`. Built on `Pressable`, so `disabled` takes no pointer events.

```jsx
import { Switch } from "@solidrt/components"
import { createSignal } from "@solidjs/signals"

function NotifyToggle() {
  let [on, setOn] = createSignal(true)
  return <Switch value={on()} onChange={setOn} />
}
```

**Props**

| Prop           | Type                         | Default | Description                                     |
| -------------- | ---------------------------- | ------- | ----------------------------------------------- |
| `value`        | `boolean`                    | -       | Controlled state. Omit for uncontrolled.        |
| `defaultValue` | `boolean`                    | `false` | Initial value for uncontrolled use.             |
| `onChange`     | `(value: boolean) => void`   | -       | Fires with the new value on toggle.             |
| `disabled`     | `boolean`                    | `false` | Takes no pointer events when true.              |
| `layout`       | `LayoutProps`                | -       | Overrides sizing/positioning of the track.      |
| `style`        | `StyleProps`                 | -       | Overrides track colors and radius.              |

### Checkbox

A checkbox. When checked it fills with `primary` and draws a checkmark; otherwise it is an empty bordered box. Controlled via `checked`/`onChange`, or uncontrolled via `defaultChecked`.

```jsx
import { Checkbox } from "@solidrt/components"

<Checkbox checked={agree()} onChange={setAgree} />
```

**Props**

| Prop             | Type                        | Default | Description                                |
| ---------------- | --------------------------- | ------- | ------------------------------------------ |
| `checked`        | `boolean`                   | -       | Controlled state. Omit for uncontrolled.   |
| `defaultChecked` | `boolean`                   | `false` | Initial value for uncontrolled use.        |
| `onChange`       | `(checked: boolean) => void`| -       | Fires with the new state on toggle.        |
| `disabled`       | `boolean`                   | `false` | Takes no pointer events when true.         |
| `layout`         | `LayoutProps`               | -       | Overrides sizing.                          |
| `style`          | `StyleProps`                | -       | Overrides box colors, border, and radius.  |

### RadioGroup / Radio

A single-selection group. `RadioGroup` owns the selected value and shares it with its `Radio` children; each `Radio` is a ring with an inner dot when selected. A string/number child of `Radio` renders as a themed label beside the ring. Controlled via `value`/`onChange` on the group, or uncontrolled via `defaultValue`.

```jsx
import { RadioGroup, Radio } from "@solidrt/components"

<RadioGroup value={plan()} onChange={setPlan}>
  <Radio value="free">Free</Radio>
  <Radio value="pro">Pro</Radio>
  <Radio value="team">Team</Radio>
</RadioGroup>
```

**RadioGroup props**

| Prop           | Type                        | Default | Description                              |
| -------------- | --------------------------- | ------- | ---------------------------------------- |
| `value`        | `unknown`                   | -       | Controlled selected value.               |
| `defaultValue` | `unknown`                   | -       | Initial selection for uncontrolled use.  |
| `onChange`     | `(value: unknown) => void`  | -       | Fires with the newly selected value.     |
| `disabled`     | `boolean`                   | `false` | Disables every `Radio` in the group.     |
| `layout`       | `LayoutProps`               | -       | Layout of the group container.           |
| `children`     | `any`                       | -       | `Radio` elements.                        |

**Radio props**

| Prop       | Type          | Description                                          |
| ---------- | ------------- | --------------------------------------------------- |
| `value`    | `unknown`     | This option's value; selecting it sets the group.   |
| `disabled` | `boolean`     | Disables this option (also disabled by the group).  |
| `layout`   | `LayoutProps` | Layout of the option row.                           |
| `style`    | `StyleProps`  | Paint properties of the option row.                 |
| `children` | `any`         | A string/number label, or custom content.           |

### Slider

A horizontal slider. The groove fills up to the thumb; pressing or dragging the track sets the value from the pointer position. Controlled via `value`/`onChange`, or uncontrolled via `defaultValue`. The drag uses pointer capture, so it keeps tracking when the pointer drifts off the track (anywhere within the window).

```jsx
import { Slider } from "@solidrt/components"

<Slider value={volume()} onChange={setVolume} min={0} max={100} step={1} layout={{ width: 180 }} />
```

**Props**

| Prop           | Type                       | Default | Description                                  |
| -------------- | -------------------------- | ------- | -------------------------------------------- |
| `value`        | `number`                   | -       | Controlled value. Omit for uncontrolled.     |
| `defaultValue` | `number`                   | `min`   | Initial value for uncontrolled use.          |
| `min`          | `number`                   | `0`     | Lower bound.                                 |
| `max`          | `number`                   | `100`   | Upper bound.                                 |
| `step`         | `number`                   | -       | Snap increment. Omit for continuous.         |
| `onChange`     | `(value: number) => void`  | -       | Fires with the new value while dragging.     |
| `disabled`     | `boolean`                  | `false` | Takes no pointer events when true.           |
| `layout`       | `LayoutProps`              | -       | Layout of the track (e.g. `width`).          |
| `style`        | `StyleProps`               | -       | Transform only (`x`/`y`/`rotate`/`scale`).   |

### Card

A themed surface container: a padded column box with a `surface` fill, a subtle `border` stroke, and rounded corners. All colors come from the theme, so it recolors live on a theme switch. Pass a `title` for a heading, or lay out the content yourself. Override any paint via `style`, spacing/sizing via `layout`.

```jsx
import { Card } from "@solidrt/components"

<Card title="Profile" layout={{ width: 360 }}>
  <Text>Card body content.</Text>
</Card>
```

**Props**

| Prop       | Type          | Default   | Description                                          |
| ---------- | ------------- | --------- | ---------------------------------------------------- |
| `title`    | `string`      | -         | Optional heading rendered above the content.         |
| `children` | `any`         | -         | Card content.                                        |
| `layout`   | `LayoutProps` | -         | Box layout (e.g. `width`, `gap`, `padding`).         |
| `style`    | `StyleProps`  | -         | Paint overrides: `backgroundColor`, `borderColor`, `borderWidth`, `borderRadius`, transform. |

### Divider

A thin rule in the theme `border` color. It stretches across its container on the cross axis: full width inside a column, full height inside a row (pass `orientation="vertical"`). Add spacing with `layout` margins, and override the color via `style.backgroundColor`.

```jsx
import { Divider } from "@solidrt/components"

<Divider />
<Divider orientation="vertical" />
```

**Props**

| Prop          | Type                          | Default        | Description                            |
| ------------- | ----------------------------- | -------------- | -------------------------------------- |
| `orientation` | `"horizontal" \| "vertical"`  | `"horizontal"` | Rule direction.                        |
| `thickness`   | `number`                      | `1`            | Line thickness in pixels.              |
| `layout`      | `LayoutProps`                 | -              | Layout (e.g. margins for spacing).     |
| `style`       | `StyleProps`                  | -              | `backgroundColor` overrides the color. |

### Badge

A small rounded pill for counts, labels, and status. Accent `primary` fill with `onPrimary` text by default; a string/number child is rendered as the themed label, anything else as-is. Override the fill via `style.backgroundColor` and the label color via `style.color`.

```jsx
import { Badge } from "@solidrt/components"

<Badge>New</Badge>
<Badge style={{ backgroundColor: theme.color.danger }}>Error</Badge>
```

**Props**

| Prop       | Type          | Default | Description                                        |
| ---------- | ------------- | ------- | -------------------------------------------------- |
| `children` | `any`         | -       | String/number renders as the label; else as-is.    |
| `layout`   | `LayoutProps` | -       | Box layout (e.g. padding overrides).               |
| `style`    | `StyleProps`  | -       | `backgroundColor` (fill), `color` (label), transform. |

### Spinner

An indeterminate spinner: a 270-degree arc that rotates continuously. It is driven by core `onFrame`, so it participates in demand-driven rendering and stops when unmounted. Color comes from the theme `primary`; override via `style.color`.

```jsx
import { Spinner } from "@solidrt/components"

<Spinner />
<Spinner size={32} thickness={4} speed={1.5} />
```

**Props**

| Prop        | Type          | Default | Description                        |
| ----------- | ------------- | ------- | ---------------------------------- |
| `size`      | `number`      | `24`    | Overall diameter in pixels.        |
| `thickness` | `number`      | `3`     | Arc stroke width in pixels.        |
| `speed`     | `number`      | `1`     | Revolutions per second.            |
| `layout`    | `LayoutProps` | -       | Layout of the box.                 |
| `style`     | `StyleProps`  | -       | `color` sets the arc; plus transform. |

### ProgressBar

A horizontal progress bar. Determinate when given a `value` in `[0, 1]` (the fill grows from the left); indeterminate when `value` is undefined (a short segment slides back and forth, driven by core `onFrame`). Track and fill colors come from the theme; override the track via `style.backgroundColor` and the fill via `style.color`.

```jsx
import { ProgressBar } from "@solidrt/components"

<ProgressBar value={0.4} />   // determinate
<ProgressBar />               // indeterminate
```

**Props**

| Prop     | Type          | Default | Description                                         |
| -------- | ------------- | ------- | --------------------------------------------------- |
| `value`  | `number`      | -       | Progress in `[0, 1]`. Omit for an indeterminate bar. |
| `layout` | `LayoutProps` | -       | Layout (e.g. `width`, `height`).                    |
| `style`  | `StyleProps`  | -       | `backgroundColor` (track), `color` (fill).          |

### QrCode

Renders a QR code for `data` out of primitives: same-color modules in a row collapse into one box, drawn on a light quiet-zone panel. The grid recomputes only when `data` or `level` changes. It paints black on white by default (not the theme) so it stays scannable through a theme switch; override `color`/`background` only if the contrast still holds.

```jsx
import { QrCode } from "@solidrt/components"

<QrCode data="https://solidjs.com" />
<QrCode data={ticket()} moduleSize={8} level="L" />
```

**Props**

| Prop         | Type                       | Default        | Description                                                    |
| ------------ | -------------------------- | -------------- | -------------------------------------------------------------- |
| `data`       | `string`                   | -              | The string to encode (URL, pairing ticket, text, ...).         |
| `moduleSize` | `number`                   | `6`            | Pixels per module (the smallest square).                       |
| `margin`     | `number`                   | `16`           | Quiet-zone padding in pixels around the grid; keep non-zero.   |
| `color`      | `string`                   | `"#000000"`    | Dark-module color.                                             |
| `background` | `string`                   | `"#ffffff"`    | Panel/light-module color.                                      |
| `level`      | `"L" \| "M" \| "Q" \| "H"` | `"M"`          | Error-correction level; higher tolerates more damage but caps data length sooner. |
| `radius`     | `number`                   | `8`            | Corner radius of the background panel.                         |
| `layout`     | `LayoutProps`              | -              | Layout of the outer box.                                       |

### Icon

A thin themed wrapper over the core `<svg>` primitive. `src` is a whole SVG document as a string; the component draws it in a square box and, for monochrome icons that stroke/fill with `currentColor`, recolors it from the theme. It carries no icon set and no name registry, so any `currentColor` SVG works (Lucide, Feather, Heroicons) and only the icons you import are bundled. Multi-color documents keep their own fills. For a non-square box, use `<svg>` directly.

Icons are just SVG strings. Import them as assets (`import House from "lucide-static/icons/house.svg"`, resolved to a string), pull them from a string export, or inline a literal:

```jsx
import { Icon } from "@solidrt/components"
import House from "lucide-static/icons/house.svg"

<Icon src={House} />
<Icon src={House} size={32} color={theme.color.primary} />
```

**Props**

| Prop     | Type          | Default            | Description                                                    |
| -------- | ------------- | ------------------ | -------------------------------------------------------------- |
| `src`    | `string`      | -                  | The SVG document to draw.                                      |
| `size`   | `number`      | `24`               | Square box side in pixels.                                     |
| `color`  | `string`      | `theme.color.text` | Drives `currentColor`; explicit fills/strokes still win.       |
| `layout` | `LayoutProps` | -                  | Layout of the box.                                             |

## License

MIT. Copyright (c) 2026 Antoine van Wel.
