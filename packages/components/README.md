<!-- GENERATED FILE, do not edit: edit docs/*.md and the interfaces in src/, then run `bun scripts/build-components-docs.ts`. -->

# @solidrt/components

A collection of components for [SolidRT](https://github.com/wellawaretech/solidrt) apps, built on the `@solidrt/core` primitives. Optional: an app can be built with core primitives alone, and a component is just a function returning core elements, so you can always drop down underneath.

> LLM agents: see [AGENTS.md](./AGENTS.md) for a dense, self-contained quickstart.

## Installation

```sh
bun add @solidrt/components   # peers: @solidrt/core, @solidjs/signals
```

Per-component prose lives in `docs/`, one file per module; the props are the typed, commented interfaces in `src/` (this package ships its source, so your editor shows them on hover). The README is generated from both.

## Theming

Appearance (colors, spacing, border, font roles) comes from one shared, reactive theme backed by a Solid store: reads are tracked, so switching the theme at runtime recolors the live UI without remounting. Four presets ship, `darkTheme` and `lightTheme` (the flat look, default dark) and `litDarkTheme` and `litLightTheme` (the same palette under a light, see below); `setTheme(preset)` switches, `setTheme(partial)` merges an override one level deep per category. A resolved theme carries every key of every category (unset ones as `undefined`), so switching to one replaces the previous theme outright, per-component overrides and icon slots included; a partial keeps whatever it does not name. Custom themes are authored with `defineTheme`.

```jsx
import { setTheme, darkTheme, lightTheme } from "@solidrt/components"

setTheme(lightTheme)                          // switch to light
setTheme(darkTheme)                           // switch to dark
setTheme({ color: { primary: "#ff2d55" } })   // override one token
```

### Authoring with defineTheme

`defineTheme(definition, scheme?)` resolves a definition into a theme. Any color may be a single value or a `[light, dark]` pair; the `scheme` argument picks the side. Pairs are opt-in per token, and a definition without any needs no scheme at all - modes are a per-theme choice, not a framework requirement (a game ships one look, not two). The built-in presets are one definition resolved twice, so they cannot drift apart.

```jsx
import { defineTheme, setTheme } from "@solidrt/components"

let def = {
  color: {
    background: ["#ffffff", "#101014"],   // [light, dark]
    primary: "#ff2d55",                   // same in both
    /* ... every color token ... */
  },
  text: { base: 15, ratio: 1.25 },
}

setTheme(defineTheme(def, "dark"))
```

The type scale derives from `text.base` (the body size, default 14) and `text.ratio` (default 1.26): caption sits one step under body, label is body at an emphasized weight, title and heading sit one and two steps above, rounded to whole pixels, with per-role `text.roles` overrides for sizes, line heights, and weights. De-emphasis is a color (`textMuted`), not a size: caption is for small glanceable text (badges, tab labels, timestamps) and stays in the full text color.

### Tokens

The color tokens are `background` (window fill), `surface` (control/card fill), `surfaceAlt` (subtle raised/track fill), `text`, `textMuted`, `border`, `primary`/`onPrimary`, `secondary`/`onSecondary` (lower-emphasis accent), `danger` (validation/destructive), `scrim` (modal dim), `ring` (the focus ring; defaults to `text` so it stays visible on primary fills), `thumb` (optional: the cap riding a track, the Switch knob and the Slider thumb; unset they are `onPrimary` and `primary`, so a palette whose `onPrimary` is dark sets a light cap here), and the feedback pair `overlayHover`/`overlayPressed`: translucent tints components draw OVER a control's own fill, so one token pair gives hover/pressed feedback on every fill color, including caller-set ones. Non-color tokens are `spacing`, `radius`, `borderWidth` (`sm` for borders, `focus` for the ring), `size` (app-wide default extents: `navRail` 72, `navSidebar` 220, `splitViewList` 320, `menuMinWidth` 120, `slider` 200; each overridable per instance through its layout or prop), and `text` (the type scale: `caption`/`label`/`body`/`title`/`heading` roles, each `{ size, lineHeight, weight }`, plus `fontFamily` and `monoFamily` for code).

### Spacing

Spacing is one base unit: `spacing` in a theme definition is a number (default 4) and the steps are multiples of it (`sm` 1x, `md` 2x, `lg` 4x, `xl` 5x). Components read them through `space()`, which applies the density policy on top, so a theme sets the rhythm and density tightens it. Pass an object (`spacing: { sm, md, lg, xl }`, any subset) to pin individual steps.

### Radius

Corner radius is set once: `radius` in a theme definition is a single number, the control radius (default 8), and the scale derives from it: `md` is the base (Button, TextInput, RichTextEditor, Select, SegmentedControl, QrCode), `sm` half of it (Checkbox, Item, NavShell items, Select and ContextMenu popups, Tooltip), `lg` one and a half (Card), and `full` the pill (Badge). Set `radius: 0` for a square theme, `radius: 12` for a soft one; buttons and inputs always match. Shapes derived from a control's own height (Switch, Slider, ProgressBar, Radio) take `full` too, through `pill(size)`: half the size, capped by `radius.full`, so `full: 0` squares every pill and circle in the library (`radius: { sm: 0, md: 0, lg: 0, full: 0 }` is the all-square theme) while the default 9999 leaves them round. `pill` is exported for a custom control with a round part. Pass an object (`radius: { sm, md, lg, full }`, any subset) to pin individual steps instead.

```jsx
setTheme({ radius: 4 })   // sm 2, md 4, lg 6
```

### Motion

`motion` holds the three durations (ms) every built-in component transition draws from, so one theme edit retimes the whole package: `fast` (default 100) is press/hover feedback, `base` (150) the color and opacity fades - state changes, the theme cross-fade (a `setTheme` fades every themed color rather than snapping), popup enter/exit - and `slow` (250) the travel of a control's moving parts (switch knob, segmented indicator, progress fill) and of a face's shadows as it sinks or lifts. `policy.motion` gates whether these play at all; a per-instance `transition` prop overrides them per property.

```jsx
setTheme({ motion: { base: 250, slow: 400 } })   // a slower, calmer app
```

### Light, materials and elevation

Every component draws its faces through `Surface`, and three theme sections decide what a face looks like. `light` is the theme's one light in screen space (`direction` [x right, y down, z into the screen], a `color`, and `ambient`, the fill on the shaded side): shadows fall along it, the sheen and the bevel run from its lit edge to the shaded one, and faces take on its color (a white light leaves every base color as it is). `material` assigns each role its layers: `surface` (Card, Item, the NavShell strips), `control` (a secondary Button, the fields, the Select trigger, the tracks and grooves, the Switch knob and the Slider thumb), `accent` (a primary or danger Button, Switch on, a checked Checkbox, the Radio dot, the Slider and ProgressBar fills, the SegmentedControl indicator, a Badge) and `overlay` (the Tooltip bubble, the Select dropdown, the ContextMenu menu, the sheets). A `Material` is `{ sheen?, bevel?, glass?, glow? }`: `sheen` (0..1) a fill gradient along the light, `bevel` (0..1) a one-pixel stroke from the light color to black, `glass` a frosted backdrop (`{ blur, tint? }`, a deliberate overlay, never a surface material: every glass face captures the pixels under it each frame), `glow` a halo (`{ radius, color? }`; without a color a part glows in its own fill, so a danger button glows red, and a fill with no color to take casts nothing; a pressable face widens it while hovered or pressed). `elevation` maps the four levels (`flat`, `raised`, `floating`, `overlay`) to heights: a face at height h casts a key shadow h px along the light and blurred 2.5 h px, plus a tight contact shadow; a press sinks the face (the key shadow fades toward the contact shadow) on the native shadow transition, and `shadow` (`{ color, strength }`) is what they are cast in.

The stock presets are flat: every material `{}` and every height 0, so nothing casts or shines and the components look as they always have. `litDarkTheme` and `litLightTheme` are the same palette under the light: a sheen and a bevel per role, `raised` 2, `floating` 6, `overlay` 14, a light cap for the Switch knob and Slider thumb, and shadows 1.7 times stronger on dark, where they otherwise vanish. Card and Button stand raised by default (a ghost or disabled button flat), the Switch knob, the Slider thumb and the SegmentedControl indicator too; Item, Badge and the Checkbox sit flat whatever the theme, since a screen holds dozens of them and a blurred shadow per row is what a scrolling list cannot afford on a tablet or a TV (measured in the runtime's notes); popups float. Per component, `theme.components.<name>.material` merges layers over the role's, `.elevation` sets the level and `.glow` the halo alone, and an instance's `style` does the same over that.

```jsx
setTheme(litDarkTheme)                                                        // the lit look
setTheme({ light: { ...theme.light, direction: [-0.4, 1, 0.8] } })            // light from the right
setTheme({ material: { accent: { sheen: 1, bevel: 0.9, glow: { radius: 14 } } } })   // glowing accents
setTheme({ components: { item: { elevation: "raised" } } })                   // raised rows, deliberately
```

### Window finish

`finish` is the theme's window finish: a core window shader declaration (`program`, `params`, and the optional `textures`, `previous`, `vertexCount`) that `Window` declares on the window as it is, so one theme can carry a vignette, scanlines or a color grade. `Window` adds one uniform, `uScale`, the display scale, for effects counted in logical pixels. Strength and layer switches are the theme's own `params`; a theme without a `program` has no finish. An app with a slider updates a param by spreading the current ones, because `setTheme` merges one level deep and a partial replaces the whole `params` object it names: `setTheme({ finish: { params: { ...theme.finish.params, uStrength: v } } })`. The theme's module links the program itself, once, at init (the client brings the GPU up before the bundle runs); the package ships no finish, and the stock presets have none.

```jsx
import { compileShader, linkProgram, destroyShader, glsl } from "@solidrt/core/gpu"

let fs = compileShader("fragment", glsl`...`, { header: true })   // reads uSource, uScale and its own params
let vs = compileShader("vertex", glsl`...`)                        // the covering triangle
let program = linkProgram(vs, fs, { label: "crt-finish" })
destroyShader(vs); destroyShader(fs)

export let crtTheme = defineTheme({ color: { /* ... */ }, finish: { program, params: { uStrength: 0.4, uScan: 1 } } })
```

### Per-component overrides

`theme.components` restyles a component everywhere without wrapping it: a `StyleProps` object per component name, merged between the component's themed defaults and each instance's `style` prop (instance style still wins). Every themed component draws the `borderColor`/`borderWidth` it is given (Item rows, the SegmentedControl, Badge, the Slider and ProgressBar grooves included), so one theme boxes the whole library; `material`, `elevation` and `glow` restyle its faces (see Light, materials and elevation).

```jsx
setTheme({ components: { button: { borderRadius: 999 } } })   // pill buttons app-wide
```

Keys: `button`, `card`, `badge`, `switch`, `checkbox`, `radio`, `slider`, `item`, `select`, `segmentedControl`, `textInput`, `richTextEditor`, `tooltip`, `divider`, `progressBar`, `spinner`, `contextMenu`, `navShell`.

### Icon slots

`theme.icons` holds semantic control glyphs as SVG document strings (the same currency as `Icon`): `chevronDown` (the Select trigger) and `check` (the Checkbox mark). Components draw their built-in vector paths by default; a theme that sets a slot swaps that glyph everywhere it appears, and the package still bundles no icon set.

```jsx
import ChevronDown from "lucide-static/icons/chevron-down.svg"

setTheme({ icons: { chevronDown: ChevronDown } })
```

API: `theme`, `setTheme`, `defineTheme`, `darkTheme`, `lightTheme`, `litDarkTheme`, `litLightTheme`, `pill`, `Theme`, `ThemeDefinition`, `ThemeColor`, `ThemeValue`, `Light`, `ShadowTone`, `ThemedComponent`, `TextStyle`, `TextVariant` - typed and commented in [src/theme.ts](./src/theme.ts).

## Policies

Theme answers "how does it look"; policies answer "how does it behave". `policy` is a second reactive layer derived from the platform facts in `@solidrt/core` (`capabilities`, `env`), so components adapt to touch vs. desktop, window size, and display without every app wiring that logic itself. Reads are reactive like `theme`: a window resize or the first mouse move on a touch-capable device updates every consuming component live.

The fields:

- `interaction` (`"touch" | "desktop" | "hybrid"`) - which affordances a component shows (hover states vs. long-press). `Tooltip`, `Select`, and `ContextMenu` fork on it.
- `density` (`"comfortable" | "compact" | "dense"`) - control/hit-target/spacing scale; drives `densityScale()` (1 / 0.85 / 0.7). A `<Density>` region overrides it per subtree.
- `motion` (`"normal" | "reduced" | "none"`) - animation intensity. Gates every built-in component transition: `reduced` keeps the color/opacity fades (a fade is not movement) but snaps everything that travels or scales, and halves the indeterminate `Spinner`/`ProgressBar` speeds; `none` snaps it all and parks the indeterminate loops.
- `focusRing` (`boolean`) - whether focused controls draw a visible focus indicator (true when a keyboard or gamepad/remote is present).
- `textScale` (`number`) - multiplier on type-scale font sizes; defaults to the OS text-scale preference.
- `navigation` (`"bottomTabs" | "rail" | "sidebar"`) - recommended nav layout, derived from the pane count: `sidebar` beside a two-pane layout, `bottomTabs` under a single pane (a side strip spends the width a narrow window is short of). `rail` is never derived; set it for a content-dense two-pane app. `NavShell` follows it.
- `layout` (`"singlePane" | "twoPane"`) - recommended pane count, derived from the window size class. `SplitView` follows it.

```jsx
setPolicy({ density: "compact" })    // pin a field, overriding the derived value
setPolicy({ density: undefined })    // hand it back to the resolver
```

`setPolicyResolver((caps) => Policies)` replaces the whole system-derivation function for full custom control; `defaultPolicyResolver` is exported to wrap or extend instead of replacing it outright.

API: `policy`, `setPolicy`, `setPolicyResolver`, `defaultPolicyResolver`, `Policies`, `PolicyResolver`, `InteractionPolicy`, `DensityPolicy`, `MotionPolicy`, `NavigationPolicy`, `LayoutPolicy` - typed and commented in [src/policy.ts](./src/policy.ts).

## Layout and style

Most components group their props into two objects, split by one rule: `layout` properties feed the layout engine (flexbox/grid, sizing, padding, margin, position - the core `LayoutProps` set) and changing them triggers a relayout; `style` properties are paint-only and never affect layout: `color`, `backgroundColor`, `borderColor`, `borderWidth`, `borderRadius`, `opacity`, `glow` (a halo on the component's accent part; `null` clears the theme's), `material` (layers merged over the role's material for the component's faces) and `elevation` (the level its raised face stands at), see Theming, and the transform (`x`, `y`, `scale`, `rotate`, `rotateX`/`rotateY` with `perspective`, `originX`/`originY`, `clipRadius`). Event handlers (`onPointerDown`, `onKeyDown`, ...) are top-level props, never inside `layout` or `style`.

`StyleProps` is that paint set. `TextLayoutProps` extends `LayoutProps` with the font fields (`fontFamily`, `fontSize`, `lineHeight`, `fontStyle`, `fontWeight`, `textAlign`, `maxLines`) because text shaping affects measurement; note `lineHeight` is a multiplier of `fontSize` (the theme uses 1.3-1.6), not a pixel value. `Option` (`{ value, label }`) is the shared shape of the single-choice controls (`Select`, `SegmentedControl`): shared shapes go through this module so components never import a sibling.

`TransitionProps` (`transition`, `onTransitionEnd`) is the third top-level group, in the component's own vocabulary rather than core's: a declaration names the view-level properties (`opacity`, `x`, `y`, `scale*`, `rotate*`, `origin*`, `perspective`, `clipRadius`) and the style ones (`backgroundColor`, `borderColor`, `borderWidth`, `borderRadius`), plus `all`, a shorthand string, `stagger` and `layout` (the layout slide of the root view, as in core: `true` borrows `all`, and `all` alone never slides) - `<Button transition={{ backgroundColor: { duration: 300 }, opacity: "200ms ease-out", layout: true }}>`. Core's paint names (`color`, `radius`, `strokeWidth`) are rejected by the types: a component is a root view plus the rects it draws for `style`, and `splitTransition` hands each entry to the node that owns it (the background rect gets `backgroundColor`/`borderRadius`, the stroke rect `borderColor`/`borderWidth`/`borderRadius`, the root view the rest). `onTransitionEnd` reports the component name (`backgroundColor`, not `color`). `Text` adds `color` (its text node), `ScrollView` adds `scrollX`/`scrollY` (its viewport).

Controls with a moving part of their own name it as an extra entry: `Switch` `knob` (the thumb's travel), `SegmentedControl` `indicator` (the active-segment slide), `ProgressBar` `fill` (the determinate glide) - `<Switch transition={{ knob: "150ms" }}>` retimes just that part. `Slider` deliberately has no parts: its thumb and fill track the drag 1:1, and a transition would rubber-band it.

The components also ship built-in motion with no props at all: state and theme colors fade, a press shrinks the free-standing controls on a quick spring and fades the overlay tints, marks (checkmark, radio dot) pop in and out, moving parts travel on springs, and the overlays (`Modal`, `Tooltip`, the `Select`/`ContextMenu` popups) fade in and out. Timing comes from `theme.motion` (`fast`/`base`/`slow`), and `policy.motion` gates it: `reduced` keeps the fades but snaps everything that moves, `none` snaps it all. A caller's `transition` entry overrides the built-in for that property, and `transition={null}` suppresses a component's built-ins outright.

API: `StyleProps`, `Glow`, `Glass`, `Material`, `MaterialRole`, `ElevationLevel`, `FontProps`, `TextLayoutProps`, `EditorLayoutProps`, `Option`, `TransitionProps`, `ComponentTransition`, `TransitionViewProp`, `TransitionStyleProp`, `TransitionScrollProp` - typed and commented in [src/types.ts](./src/types.ts).

## Typography helpers

`typeStyle(variant)` resolves a theme type-scale role (`caption`/`label`/`body`/`title`/`heading`) to font props ready to spread onto a `<text>` or `d-text`: `fontSize` carries `policy.textScale`, `fontWeight` is the role's own. Reactive when called inside a tracked scope, like any theme/policy read. `Text` applies it for you; reach for the helpers when building custom text out of core primitives.

`typeScaleStyles()` is the type scale as font styles (every role, plus the mono family at body size), and `warmTypeScale()` hands them to core's `warmText`, which makes their glyph cells ahead of the first screen on the text engine's worker. `Window` does this on mount and again when the theme or the text policy changes; an app that draws its own window calls it once at startup.

There is no weight compensation: the text engine blends glyph coverage with DirectWrite's gamma-aware recipe, so light-on-dark text keeps its stems at every display scale.

API: `typeStyle`, `typeScaleStyles`, `warmTypeScale` - typed and commented in [src/typography.ts](./src/typography.ts).

## Spacing

`space(token)` is density-scaled spacing: a `theme.spacing` token (`sm`/`md`/`lg`/`xl`) multiplied by the density scale of the nearest `<Density>` region (falling back to the global policy) and rounded to whole pixels. Use it for gaps and paddings that should tighten under `compact`/`dense` density; read `theme.spacing` directly only for distances that must not move with density. Reactive when called inside a tracked scope.

```jsx
<View layout={{ padding: space("md"), gap: space("sm") }} />
```

API: `space` - typed and commented in [src/spacing.ts](./src/spacing.ts).

## Components

### Window

The root surface of an app: renders a core `<window>`, so `render()` accepts it. Applies `layout` and `style.backgroundColor` only (a window cannot be transformed or bordered), plus `title` and `fullscreen`.

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

The window takes the theme's finish (see Theming) whenever the theme has one; a theme without a finish declares no shader at all. `shader` is the window's own finish, a core window shader declaration forwarded as-is; set, it replaces the theme's, and `shader={null}` opts out of it. A window shader runs past the point snapshots read, so a finish is checked with `sol render`, not `get_snapshot`.

API: `Window`, `WindowProps` - typed and commented in [src/window.tsx](./src/window.tsx).

### View

A general-purpose box. Spreads `layout` onto the underlying view, applies the transform from `style`, and draws a background and/or border when those style props are set. Takes all pointer event props.

```jsx
import { View } from "@solidrt/components"

<View
  layout={{ padding: 16, flexDirection: "column", gap: 8 }}
  style={{ backgroundColor: "#222", borderRadius: 8 }}
>
  {/* ... */}
</View>
```

API: `View`, `ViewProps` - typed and commented in [src/view.tsx](./src/view.tsx).

### Text

Themed text in a layout box. `variant` picks a typography role from the theme's type scale (`caption`/`label`/`body`/`title`/`heading`, default `body`); `color` picks a semantic theme color (`text`, `textMuted`, `primary`, `onPrimary`, `danger`, default `text`), with `muted` as sugar for `color="textMuted"`. Font fields go in `layout` (they affect measurement) and individually override the role; `style.color` still wins over `color`.

Font sizes carry `policy.textScale` (the OS text-size preference) and weights carry the low-DPI light-on-dark compensation; use the core `<text>` primitive for text that must not scale.

```jsx
import { Text } from "@solidrt/components"

<Text variant="title">Section</Text>
<Text muted layout={{ maxLines: 2 }}>Supporting copy that may wrap.</Text>
<Text layout={{ fontSize: 18 }} style={{ color: "#fff" }}>Custom</Text>
```

Note that `lineHeight` is a multiplier of `fontSize` (the theme uses 1.3-1.6), not a pixel value.

API: `Text`, `TextProps`, `TextColor` - typed and commented in [src/text.tsx](./src/text.tsx).

### Image

Loads and displays an image from a URL or raw bytes (`src: string | Uint8Array`). URL loads are shared runtime-wide: mounts of the same URL reuse one fetch and one texture, and the bytes are cached on disk (fetched with `cache: "force-cache"` - no freshness check, so use versioned URLs for content that changes). Concurrent asset fetches are kept polite with a per-host limit; a failed load rejects the mounts sharing it and a later remount retries.

```jsx
import { Image } from "@solidrt/components"

<Image
  src="https://example.com/avatar.png"
  fallback={PLACEHOLDER_PNG}
  layout={{ width: 64, height: 64 }}
/>
```

With `fit` the image fills whatever box `layout` gives the component - numbers, `pct()`, or flex - and the fit decides how the pixels map into it (CSS object-fit, centered; `"cover"` is the ported-web-hero-image answer). Without `fit`, only numeric layout sizes reach the image; anything else draws at intrinsic size.

```jsx
<Image src={hero} fit="cover" layout={{ width: pct(100), height: 240 }} />
```

A failing `src` is contained by the component: the `fallback` shows, or the `backgroundColor` placeholder stays; the error does not propagate to an outer `<Errored>` boundary. `onLoad` fires each time a source finishes loading, `onError` when `src` fails.

API: `Image`, `ImageProps` - typed and commented in [src/image.tsx](./src/image.tsx).

### SafeArea

Wraps its children in a view padded clear of system UI intrusions (status bars, home indicators, notches). Top and bottom insets are applied by default; pass `false` to opt out of an edge, or a number to apply the inset with that minimum padding.

```jsx
import { SafeArea } from "@solidrt/components"

<SafeArea top bottom>...</SafeArea>       // the default edges
<SafeArea bottom={false}>...</SafeArea>   // top only
<SafeArea top={16} bottom={16}>...</SafeArea>  // insets with a 16px minimum
<SafeArea top bottom left right>...</SafeArea> // all four edges
```

API: `SafeArea` - typed and commented in [src/safe-area.tsx](./src/safe-area.tsx).

### TextInput

Text input, single-line by default; `multiline` wraps at the field's width and edits across lines (Enter inserts a newline, Up/Down move by line; grows with content up to `maxRows` unless its layout sizes the box, and scrolls to the caret). Controlled via `value`/`onInput`, or uncontrolled via `defaultValue`; `onSubmit` fires on Enter (single-line only), `onCancel` on Escape or the platform's back, each followed by the blur. Also `placeholder`, `maxLength`, `autoFocus`, `disabled`, `onFocus`/`onBlur`, and `hints` for IME behavior (keyboard type, capitalization, autocorrect - identifier-like fields want `{ capitalize: "none", autocorrect: false }`). The font fields of `layout` (`fontSize`, `fontFamily`, `lineHeight`, `fontStyle`, `fontWeight`) shape the text as on `Text`, with the rows, caret and scrolling following.

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

`style` overrides the themed colors, border, and radius (`borderWidth: 0` draws no border at all, and no focus ring). The mouse cursor is the I-beam over the field. `autoFocus` focuses on mount (the on-screen keyboard still waits for a tap). A tap anywhere outside the field blurs it.

Leaving the field: Enter submits (`onSubmit` with the value, then the blur); Escape and the platform's back (the Android button, a pad's back button) cancel (`onCancel`, then the blur). `onCancel` only reports: the value stays as it is, and reverting it is the app's call. `onBlur` runs after either, and on a tap outside.

Keys the focused field keeps for itself, which never reach a parent `onKeyDown` or the window's shortcuts: Backspace, Delete, Left and Right, Home and End, Enter (and a remote's center key), Escape, Ctrl/Cmd+A and Ctrl/Cmd+V, Ctrl/Cmd+C and Ctrl/Cmd+X when text is selected, and every printable key while typing. A multiline field also keeps Up and Down. Everything else bubbles on (Tab, Up and Down in a single-line field, PageUp, function keys, Ctrl+S, ...), so a chat field that wants Up for its history wraps the field in a `View` with an `onKeyDown`.

A multiline field sizes like a flex item. Unconstrained, it grows with its content, up to `maxRows` rows and then scrolls. Sized by its layout it is a fixed box that scrolls to the caret: an explicit `height`, or `flexGrow: 1` in a parent with a height (the field fills what is left and scrolls once the text outgrows it), or a parent too small for the content (the field shrinks to fit rather than overflowing).

```jsx
<View layout={{ flexDirection: "column", height: 400 }}>
  <Text>Notes</Text>
  <TextInput multiline layout={{ flexGrow: 1, fontSize: 18 }} />
</View>
```

API: `TextInput`, `TextInputProps` - typed and commented in [src/text-input.tsx](./src/text-input.tsx).

### RichTextEditor

Edits a rich text `Document` (styled runs, paragraph attributes) in the same field as `TextInput`: always multiline, same caret, keys, wrapping, scrolling and sizing, and the same font fields on `layout` (the base font the document's runs style from). Controlled via `value`/`onInput`, or uncontrolled via `defaultValue` (start from `plainDocument("")`). Formatting is driven through `editorRef`, which hands you the document buffer - the component ships no toolbar; the app renders its own controls.

```jsx
import { RichTextEditor, plainDocument } from "@solidrt/components"

function Notes() {
  let editor
  return (
    <>
      <Button onPress={() => editor.format({ bold: editor.attributes().bold ? null : true })}>B</Button>
      <RichTextEditor
        defaultValue={plainDocument("Start typing...")}
        editorRef={(e) => (editor = e)}
        layout={{ width: 320 }}
        maxRows={10}
      />
    </>
  )
}
```

Drawn attributes - inline: `bold`, `italic`, `underline`, `code` (mono), `color` (a color string), `link` (a URL string: primary color, underlined); block: `heading: 1 | 2 | 3`. Other attributes are carried in the document and ignored by the drawing; font-affecting ones feed the text geometry too, so caret and wrap follow the drawn glyphs. Inline atoms (U+FFFC) render as their placeholder character for now.

API: `RichTextEditor`, `RichTextEditorProps` - typed and commented in [src/rich-text-editor.tsx](./src/rich-text-editor.tsx).

### Document model

The value model behind `RichTextEditor`: a `Document` is `{ text, runs, blocks }` - plain text plus attributed runs (inline formatting) and per-paragraph blocks. `plainDocument(text)` builds one from a string; `createDocumentBuffer(doc)` wraps one in the editing API (`format`, `formatBlock`, `insertAtom`, `attributes`, selection and edits) that `editorRef` hands out. `ATOM` is the inline-atom placeholder character (U+FFFC) for embedded objects.

The shapes (`Document`, `DocumentRun`, `Attributes`, `AttributePatch`, `DocumentBuffer`) are exported so an app can build, inspect, persist, and transform documents outside the editor.

API: `createDocumentBuffer`, `plainDocument`, `ATOM`, `Document`, `DocumentRun`, `DocumentBuffer`, `DocumentBufferOptions`, `Attributes`, `AttributePatch` - typed and commented in [src/rich-text-document.ts](./src/rich-text-document.ts).

### ScrollView

A scrollable region; vertical by default, `horizontal` to flip. Both the wheel and dragging scroll the content: the drag activates after a small movement threshold along the scroll axis, also when it starts on a pressable (the press is cancelled and its feedback retracts), and keeps scrolling when the pointer leaves the box. Scrolling glides: the offset springs to each new target (250 ms, critically damped), so a wheel notch never jumps and a burst of notches reads as one motion; a dragging finger is tracked exactly, without the spring. A lift at speed flings: the content keeps moving from the finger's release speed and decays (iOS's deceleration), as one runtime-side animation to a projected destination, clamped to the range (it slows into an edge, no bounce). A finger landing on a moving list holds it where it is, a tap included.

```jsx
import { ScrollView, Text } from "@solidrt/components"
import { For } from "@solidrt/core"

<ScrollView layout={{ height: 300 }} style={{ backgroundColor: "#111", borderRadius: 8 }}>
  <For each={items()}>{(item) => <Text>{item}</Text>}</For>
</ScrollView>
```

`scrollRef` hands out the scroll handle from `createScroll`: `offset()` and `range()` (the largest reachable offset, refreshed each layout) are reactive; `scrollTo({ x, y, behavior })` and `scrollBy({ x, y, behavior })` clamp to the range, an omitted axis stays put, and `behavior: "instant"` writes without the spring (the web's word; `"auto"` and `"smooth"` are the default motion). Scroll policies are written against it in the app. A transcript that opens at its newest message and then follows growth, without yanking a reader who has scrolled back:

```tsx
let [scroll, setScroll] = createSignal<Scroll>()
createEffect(
  () => scroll()?.range(),
  (r, prev) =>
    untrack(() => {
      let s = scroll()
      if (!s || !r) return
      if (!prev || prev.y === 0) s.scrollTo({ y: Infinity, behavior: "instant" })
      else if (s.offset().y >= prev.y - 1) s.scrollTo({ y: Infinity })
    }),
)
<ScrollView scrollRef={setScroll}>...</ScrollView>
```

The range changes whenever the content or the viewport changes size. The first fill (nothing was scrollable before it, whether it mounted with the view or arrived a second later) lands instantly, as a chat opens at its end; after that the view follows the end only if it was at the previous end, and the spring makes that follow a glide. The handle arrives during mount, outside any reactive scope, so a signal setter can be passed as the ref; holding it in a signal lets the effect track its arrival. The offset is read untracked: the policy reacts to the range, not to every scroll.

A `scrollX`/`scrollY` entry in `transition` replaces the default spring: `transition={{ scrollY: { duration: 400, bounce: 0.2 } }}` (keep it a spring rather than a tween, because the wheel retargets mid-flight). The other entries animate the box itself and its background/border as on any component.

The underlying geometry primitive `createScroll` is available from `@solidrt/core` for building custom scrollers.

API: `ScrollView`, `ScrollViewProps` - typed and commented in [src/scroll-view.tsx](./src/scroll-view.tsx).

### Dismissible

Swipe-to-dismiss. The content follows the finger sideways; a swipe that qualifies (enough travel and speed, within 30 degrees of the axis, core's swipe recognizer) carries it out of the box at its own speed and reports `onDismiss` with the direction, a drag that stops short springs back. `direction` narrows which way dismisses (`"left"`, `"right"`, default both); a drag the other way is not started, and a vertical drag is left to an enclosing ScrollView. The recognizer takes the pointer at its movement slop, so a pressable row still presses on a tap and retracts once the drag is one. Mouse and touch alike.

```jsx
import { Dismissible, Item } from "@solidrt/components"
import { For } from "@solidrt/core"

<For each={mails()}>
  {(mail) => (
    <Dismissible direction="left" onDismiss={() => archive(mail.id)}>
      <Item label={mail.subject} description={mail.from} onPress={() => open(mail)} />
    </Dismissible>
  )}
</For>
```

The box does not remove itself: after `onDismiss` the content sits parked past the edge until the caller drops the row (a `layout` or `exit` transition on the row then plays the collapse).

API: `Dismissible`, `DismissibleProps`, `DismissDirection` - typed and commented in [src/dismissible.tsx](./src/dismissible.tsx).

### Carousel

A pager. The children are the pages, each one box wide, laid side by side and moved with the finger: a horizontal swipe (core's swipe recognizer) turns the page, a drag that stops short snaps to the nearest one, and the page settles under a spring. Only horizontal drags are taken, so a vertical ScrollView around or inside it scrolls as before. `index` controls the page (pair it with `onChange`); without it the carousel keeps its own page and still reports turns.

```jsx
import { Carousel, Card, Text } from "@solidrt/components"

<Carousel layout={{ height: 200 }} onChange={(i) => setPage(i)}>
  <Card title="One"><Text>First page</Text></Card>
  <Card title="Two"><Text>Second page</Text></Card>
  <Card title="Three"><Text>Third page</Text></Card>
</Carousel>
```

Page indicators are the app's: read the index from `onChange` and draw dots beside it.

API: `Carousel`, `CarouselProps` - typed and commented in [src/carousel.tsx](./src/carousel.tsx).

### Pressable

A pressable box: `onPress` fires on a primary-button press released over the box; a drag out of the box (or a non-primary button) does not fire it, and a drag back in restores the pressed state. `children` and `style` may each be a function of the live `{ pressed, hovered, focused, pending }` state, so the box restyles on press/hover without extra signals - read the state inside the prop or child expression, never eagerly into a local.

```jsx
import { Pressable, Text } from "@solidrt/components"

<Pressable
  onPress={() => setCount((c) => c + 1)}
  layout={{ padding: 12 }}
  style={(s) => ({ backgroundColor: s.pressed ? "#333" : "#222", borderRadius: 8 })}
>
  <Text>Tap me</Text>
</Pressable>
```

The mouse cursor is the pointer hand over the box (`cursor` overrides it). `disabled` takes no pointer events. When pressables nest, the innermost one wins the press. An `onPress` returning a promise sets `pending` until it settles; presses meanwhile are ignored, so async actions cannot double-fire.

API: `Pressable`, `PressableProps`, `PressState` - typed and commented in [src/pressable.tsx](./src/pressable.tsx).

### createPress

The press recognizer the package's own controls are built on, for a custom control that wants its own root view instead of wrapping `Pressable`: `onPress` fires on a primary-button down followed by an up over the node, a drag out retracts the pressed state and a drag back in restores it, nested recognizers resolve innermost-first through core's arena, and the node's `ref` registers `onPress` as its focus-navigation `select` action so a remote or keyboard activates it while it holds focus. Spread `handlers` on the root view, attach `ref`, and read `pressed()`, `hovered()`, `focused()` and `pending()` (or `state()`, the live object the `Pressable` render prop receives) inside the props that style on them. Options are read at event time, so a component's reactive props object can be passed as is.

```jsx
import { createPress, theme } from "@solidrt/components"

function Chip(props) {
  let press = createPress(props)
  return (
    <view ref={press.ref} {...press.handlers} padding={8} focusable cursor="pointer">
      <d-rect color={press.pressed() ? theme.color.overlayPressed : theme.color.surfaceAlt} radius={theme.radius.full} />
      <text color={theme.color.text}>{props.label}</text>
    </view>
  )
}
```

`registerNavAction` (from `createFocusNav`'s module) is the piece underneath: a control that activates without `createPress` registers its own `select` action with it.

API: `createPress`, `PressOptions` - typed and commented in [src/press.ts](./src/press.ts).

### Button

A themed press target over `Pressable`: a padded, centered box with a label. A press shrinks it slightly on a quick spring and tints it with `overlayPressed`; hover tints with `overlayHover` (non-touch policies). `variant` picks the visual role - `primary` (accent fill, the default), `secondary`, `ghost` (no fill until hover), `danger` (destructive) - with fill, tints, and label color from the matching theme tokens; no variant draws a border. `size` (`sm`/`md`/`lg`) pins a minimum width so a row of buttons lines up (a longer label still expands past it); omitted, the button sizes to its content. A string or number child renders as the themed label; any other child renders as-is (an icon, a row, ...).

```jsx
import { Button } from "@solidrt/components"

<Button onPress={save}>Save</Button>
<Button variant="ghost" onPress={cancel}>Cancel</Button>
<Button variant="danger" size="md" onPress={remove}>Delete</Button>
```

Press feedback is a slight scale; hover feedback is the theme's `overlayHover` tint drawn over the fill (non-touch interaction policies only), so it composes with any background, including a caller-set `style.backgroundColor`. `disabled` mutes the colors and takes no pointer events.

An `onPress` returning a promise makes the button an async action: while it is unsettled a centered spinner replaces the label (geometry unchanged, so nothing shifts) and further presses are ignored - a save or submit cannot double-fire.

```jsx
<Button onPress={async () => { await save() }}>Save</Button>
```

A focused Button (see `createFocusNav`) draws a ring in the theme `ring` color under the `focusRing` policy (text-colored by default so it stays visible on primary-filled buttons) and activates on the navigation's `select` action (Enter, Space, a remote's center key or a pad's south button under the standard bindings). `focusable` (default true) opts out of focus-navigation candidacy; disabled buttons are never candidates.

API: `Button`, `ButtonProps`, `ButtonVariant` - typed and commented in [src/button.tsx](./src/button.tsx).

### createFocusNav

Focus navigation for pointer-free control (TV remote, keyboard, gamepad), moving real focus across the elements declaring `focusable`. It consumes three UI actions from an input map and reads no device: `navigate` (vec2) moves spatially, picking the nearest candidate in the pressed direction by on-screen boxes; `cycle` (axis) walks visual reading order - rows top to bottom, left to right - wrapping at the ends; `select` (button) activates the focused control; a held direction repeats. Created bare, the nav binds the standard set itself (`uiBindings`: arrows, Tab and Shift+Tab, Enter, Space and the remote center key on the keyboard; dpad, left stick and south on every pad) on a map of its own, reachable as `nav.input` for rebinding; given `input`, it consumes the app's map instead. Spread `nav.handlers` on the window. Nothing is focused until the first navigation press; pointer input works unchanged throughout. Every interactive control (Button, Item, TextInput, RichTextEditor, Checkbox, Radio, Switch, Select and its options, SegmentedControl segments, Slider) is a candidate unless disabled, and draws the theme's `ring` color at `borderWidth.focus` while focused under the `focusRing` policy; the Slider steps its value with the arrow keys.

```jsx
import { createFocusNav } from "@solidrt/components"

function App() {
  let nav = createFocusNav()
  return <Window {...nav.handlers}>...</Window>
}
```

Spreading the map's handlers on the window is what keeps it cooperative: key events bubble from the focused node, so a focused TextInput keeps its caret keys (and a typed Space) and navigation only sees what nothing else consumed. Pads need no handler; their sources poll.

Rebind on the nav's own map, or bring a map that also holds the app's actions:

```jsx
let nav = createFocusNav()
nav.input.bind("select", gamepad().button("east"))

let input = createInputMap({ ...uiActions, pause: "button" })
input.bind(uiBindings({ keyboard, gamepad: gamepad() }))
input.bind("pause", keyboard.key("Escape"), gamepad().button("start"))
let nav = createFocusNav({ input })
return <Window {...input.handlers}>...</Window>
```

When the focused control disappears (an action replacing it, a screen change), focus lands on the nearest candidate to where it sat as soon as the successor is laid out - the ring follows a Disconnect button into the Connect button that replaces it. A deliberate blur (tapping outside, dismissing the keyboard) stays blurred; the next press resumes at the nearest candidate.

An open `Modal` traps navigation inside itself with no extra wiring (topmost wins when stacked); pass `scope: () => nodeOrNull` to trap into some other subtree instead. `move`/`tab`/`activate` are exposed for custom triggers.

API: `createFocusNav`, `registerNavAction`, `uiActions`, `uiBindings`, `FocusNavOptions`, `UiActions`, `UiDevices` - typed and commented in [src/focus-nav.ts](./src/focus-nav.ts).

### Switch

An on/off toggle: the track fills with `primary` when on and `surfaceAlt` when off (a fade), and the thumb springs across - the `knob` transition entry retimes that travel. Controlled via `value`/`onChange`, or uncontrolled via `defaultValue`. Built on `Pressable`, so `disabled` takes no pointer events. The thumb is `theme.color.thumb` (else `onPrimary`), track and thumb follow `radius.full` through `pill` (square in an all-square theme), and the on track glows with the theme's accent glow. `style` overrides the track colors and radius.

```jsx
import { Switch } from "@solidrt/components"
import { createSignal } from "@solidjs/signals"

function NotifyToggle() {
  let [on, setOn] = createSignal(true)
  return <Switch value={on()} onChange={setOn} />
}
```

API: `Switch`, `SwitchProps` - typed and commented in [src/switch.tsx](./src/switch.tsx).

### Checkbox

A checkbox: filled with `primary` and a drawn checkmark when checked, an empty bordered box otherwise - the fill fades and the mark pops in and out. Controlled via `checked`/`onChange`, or uncontrolled via `defaultChecked`. The mark is the `theme.icons.check` slot when a theme sets one. The checked fill glows with the theme's accent glow. `style` overrides the box colors, border, and radius.

```jsx
import { Checkbox } from "@solidrt/components"

<Checkbox checked={agree()} onChange={setAgree} />
```

API: `Checkbox`, `CheckboxProps` - typed and commented in [src/checkbox.tsx](./src/checkbox.tsx).

### RadioGroup / Radio

A single-selection pair: `RadioGroup` owns the selected value (controlled via `value`/`onChange`, or uncontrolled via `defaultValue`) and shares it with its `Radio` children; each `Radio` is a ring with an inner dot when selected - the ring color fades and the dot pops in and out. Ring and dot follow `radius.full` through `pill` (square in an all-square theme), and the dot glows with the theme's accent glow. A string/number child of `Radio` renders as a themed label beside the ring; anything else as-is. `disabled` on the group disables every option, on a `Radio` just that one.

```jsx
import { RadioGroup, Radio } from "@solidrt/components"

<RadioGroup value={plan()} onChange={setPlan}>
  <Radio value="free">Free</Radio>
  <Radio value="pro">Pro</Radio>
  <Radio value="team">Team</Radio>
</RadioGroup>
```

API: `RadioGroup`, `Radio`, `RadioGroupProps`, `RadioProps` - typed and commented in [src/radio.tsx](./src/radio.tsx).

### Slider

A horizontal slider: the groove fills up to the thumb, and pressing or dragging the track sets the value from the pointer position. Controlled via `value`/`onChange` (fires while dragging), or uncontrolled via `defaultValue` (defaults to `min`). `min`/`max` default to 0/100; `step` snaps to an increment, omitted the value is continuous. The drag keeps tracking when the pointer drifts off the track, and an enclosing ScrollView never takes it over. The groove is `surfaceAlt`, the fill `primary` (glowing with the theme's accent glow) and the thumb `theme.color.thumb` (else `primary`); `style` and `theme.components.slider` restyle them as ProgressBar's do: `backgroundColor` the groove, `color` the fill, `borderRadius` the groove's ends (round through `pill`, square under `radius.full: 0`), `borderColor`/`borderWidth` a box around the groove.

```jsx
import { Slider } from "@solidrt/components"

<Slider value={volume()} onChange={setVolume} min={0} max={100} step={1} layout={{ width: 180 }} />
```

API: `Slider`, `SliderProps` - typed and commented in [src/slider.tsx](./src/slider.tsx).

### Card

A themed surface container: a padded column box with a `surface` fill, a subtle `border` stroke, and rounded corners, recoloring live on a theme switch. Pass a `title` for a heading, or lay out the content yourself; override paint via `style`, spacing/sizing via `layout`.

```jsx
import { Card } from "@solidrt/components"

<Card title="Profile" layout={{ width: 360 }}>
  <Text>Card body content.</Text>
</Card>
```

API: `Card`, `CardProps` - typed and commented in [src/card.tsx](./src/card.tsx).

### Item

A list row: `startContent` (icon, avatar, checkbox), a `label` with an optional `description` under it, and `endContent` (badge, timestamp, action) pushed to the end. String/number label and description render as themed body and muted body text; anything else as-is. The dense-data workhorse: rows compose with `<For>` inside a plain column view or `ScrollView` - there is no List wrapper, because a column IS the list. Paddings and gaps are density-scaled, so a `<Density>` region compacts rows wholesale.

```jsx
import { Item, Badge, Icon } from "@solidrt/components"
import { For } from "@solidrt/core"

<view flexDirection="column">
  <For each={issues()}>
    {(issue) => (
      <Item
        startContent={<Icon src={Bug} />}
        label={issue.title}
        description={issue.assignee}
        endContent={<Badge variant="neutral">{issue.id}</Badge>}
        selected={issue.id === current()}
        onPress={() => setCurrent(issue.id)}
      />
    )}
  </For>
</view>
```

With `onPress` the row is interactive: hover/pressed overlay tints (no scale - rows sit flush in a list), focusable for spatial navigation, Enter/remote activation, and a focus ring under the `focusRing` policy. An async `onPress` (returning a promise) is not re-fired until it settles. Without `onPress` the row attaches no press recognizer, so controls inside it (a Switch in a settings row) and enclosing pressables receive pointer events untouched; interactivity is decided at mount. `selected` fills the row with `surfaceAlt`; `disabled` dims the row and takes no pointer events. Separate rows with `Divider` where needed, or box them with `borderColor`/`borderWidth` (per instance, or every row through `theme.components.item`).

API: `Item`, `ItemProps` - typed and commented in [src/item.tsx](./src/item.tsx).

### Field

A form row: `label` above the control, the control itself (`children`, rendered as-is), and a help or error line below. `error` renders in the danger color and replaces `description` while set. It draws no chrome and does not reach into the control - error styling of the input itself stays the input's `style` prop, no hidden magic. The message line only occupies space while there is one; reserve the space with a constant `description` if the form must not jump when an error appears.

```jsx
import { Field, TextInput } from "@solidrt/components"

<Field label="Email" description="Used for receipts only." error={emailError()}>
  <TextInput value={email()} onInput={setEmail} />
</Field>
```

API: `Field`, `FieldProps` - typed and commented in [src/field.tsx](./src/field.tsx).

### Divider

A thin rule in the theme `border` color. It stretches across its container on the cross axis: full width inside a column, full height inside a row (pass `orientation="vertical"`). `thickness` defaults to 1px; add spacing with `layout` margins, and override the color via `style.backgroundColor`.

```jsx
import { Divider } from "@solidrt/components"

<Divider />
<Divider orientation="vertical" />
```

API: `Divider`, `DividerProps` - typed and commented in [src/divider.tsx](./src/divider.tsx).

### Badge

A small rounded pill for counts, labels, and status. `variant` picks the role: `primary` (accent fill, the default), `neutral` (subtle surface), `danger`. A string/number child renders as the themed label, anything else as-is (an icon, a dot, ...). Override the fill via `style.backgroundColor` and the label color via `style.color`; `borderColor`/`borderWidth` box it. The accent variants glow with the theme's accent glow.

```jsx
import { Badge } from "@solidrt/components"

<Badge>New</Badge>
<Badge variant="danger">Error</Badge>
```

API: `Badge`, `BadgeProps`, `BadgeVariant` - typed and commented in [src/badge.tsx](./src/badge.tsx).

### Spinner

An indeterminate spinner: a 270-degree arc that rotates continuously, driven by core `onFrame`, so it participates in demand-driven rendering and stops when unmounted. `size` (diameter, default 24), `thickness` (default 3), `speed` (revolutions per second, default 1). Color comes from the theme `primary`; override via `style.color`.

```jsx
import { Spinner } from "@solidrt/components"

<Spinner />
<Spinner size={32} thickness={4} speed={1.5} />
```

API: `Spinner`, `SpinnerProps` - typed and commented in [src/spinner.tsx](./src/spinner.tsx).

### ProgressBar

A horizontal progress bar: determinate when given a `value` in `[0, 1]` (the fill grows from the left, gliding to each new value - the `fill` transition entry retimes it), indeterminate when `value` is undefined (a short segment slides back and forth, driven by core `onFrame`). Track is `surfaceAlt`, fill is `primary`; override via `style.backgroundColor` (track) and `style.color` (fill), and box the track with `borderColor`/`borderWidth`. The ends follow `radius.full` through `pill`, and the fill glows with the theme's accent glow.

```jsx
import { ProgressBar } from "@solidrt/components"

<ProgressBar value={0.4} />   // determinate
<ProgressBar />               // indeterminate
```

API: `ProgressBar`, `ProgressBarProps` - typed and commented in [src/progress-bar.tsx](./src/progress-bar.tsx).

### Portal

Renders its child somewhere other than its lexical position: by default at the window root, so overlays (modals, menus, tooltips) escape the clipping and stacking of their surrounding layout; `mount` targets another node captured from a `ref` instead. A thin JSX wrapper over core `createPortal`. The child should be a single element with `position="absolute"`, since it is inserted into the window's flex root. Portals cannot mount during the app's initial render, so gate them behind a signal that starts false.

```jsx
import { Portal } from "@solidrt/components"

<Show when={open()}>
  <Portal>
    <view position="absolute" right={16} bottom={16}>
      <Card>Saved</Card>
    </view>
  </Portal>
</Show>
```

API: `Portal`, `PortalProps` - typed and commented in [src/portal.tsx](./src/portal.tsx).

### Modal

A centered overlay rendered at the window root via core `createPortal`: it fills the window with a dimming backdrop (theme `scrim`; override via `backdropColor`, `"transparent"` for no dim) and centers `children` on top, the whole overlay fading in at mount and out on removal (an exiting modal takes no hits). Control visibility by mounting/unmounting it, e.g. `<Show when={open()}>`; the gating signal must start false since portals cannot mount during the initial render. Pressing the backdrop, Escape and the platform's back call `onClose` (unless `dismissable` is false, which keeps the modal up and still stops a back from reaching the screen beneath), pressing the content does not, and while mounted the modal traps `createFocusNav` inside itself.

```jsx
import { Modal, Card, Button } from "@solidrt/components"

<Show when={open()}>
  <Modal onClose={() => setOpen(false)}>
    <Card>
      <Button onPress={() => setOpen(false)}>Close</Button>
    </Card>
  </Modal>
</Show>
```

API: `Modal`, `ModalProps` - typed and commented in [src/modal.tsx](./src/modal.tsx).

### Tooltip

A hover-only affordance: under the `desktop`/`hybrid` interaction policies, resting a mouse pointer on the wrapped content shows a bubble near it after `delay` (default 500ms). Under the `touch` policy it never shows, so tooltip content must stay non-essential. The bubble is portal-mounted at the window root, clamped to the window edges, takes no pointer events, fades in and out, hides on leave and on press, and glows with the theme's overlay glow. A string/number `content` renders as themed body text; anything else as-is. `placement` picks the side (`"top"`, the default, or `"bottom"`).

```jsx
import { Tooltip, Button } from "@solidrt/components"

<Tooltip content="Save (Ctrl+S)">
  <Button onPress={save}>Save</Button>
</Tooltip>
```

API: `Tooltip`, `TooltipProps` - typed and commented in [src/tooltip.tsx](./src/tooltip.tsx).

### Select

A single-choice picker whose presentation forks on the interaction policy: `desktop`/`hybrid` opens an anchored dropdown under the trigger (flipping above when there is no room), `touch` opens a bottom sheet over a scrim. Same contract either way: `options` is an `Option[]` (`{ value, label }`), controlled via `value`/`onChange` or uncontrolled via `defaultValue`; pressing outside, Escape and the platform's back close without a change. `placeholder` shows in the trigger while nothing is selected. Both presentations fade in and out, the dropdown glows with the theme's overlay glow, and the trigger's chevron flips while open. The option list is not scrollable yet, so keep it short. The chevron is the `theme.icons.chevronDown` slot when a theme sets one.

```jsx
import { Select } from "@solidrt/components"

let options = [
  { value: "s", label: "Small" },
  { value: "m", label: "Medium" },
  { value: "l", label: "Large" },
]

<Select options={options} value={size()} onChange={setSize} placeholder="Size" />
```

API: `Select`, `SelectProps` - typed and commented in [src/select.tsx](./src/select.tsx).

### SegmentedControl

A single-choice row of equal-width segments joined flush: only the control's outermost corners are rounded, hairline dividers separate the segments, and the active segment is one `primary` indicator that springs between segments on a selection change - the `indicator` transition entry retimes it. Hovered segments tint with the theme `overlayHover` under non-touch interaction policies. `options` is an `Option[]`; controlled via `value`/`onChange`, or uncontrolled via `defaultValue`. Override the inactive fill via `style.backgroundColor` and the outer radius via `style.borderRadius`, and box the control with `borderColor`/`borderWidth`. The indicator glows with the theme's accent glow.

```jsx
import { SegmentedControl } from "@solidrt/components"

<SegmentedControl
  options={[{ value: "day", label: "Day" }, { value: "week", label: "Week" }]}
  value={range()}
  onChange={setRange}
/>
```

API: `SegmentedControl`, `SegmentedControlProps` - typed and commented in [src/segmented-control.tsx](./src/segmented-control.tsx).

### ContextMenu

Secondary actions on the wrapped content. The opening gesture follows the physical pointer: right-click for a mouse, long-press (500 ms, cancelled by finger travel; core's long-press recognizer) for touch and pen. The long-press wins the finger at its timer: a pressable inside retracts and does not fire on the lift, and a scroll that started first keeps the finger. The presentation forks on the interaction policy: `touch` gets a bottom sheet over a scrim, `desktop`/`hybrid` an anchored menu at the pointer that flips up near the bottom edge. Both presentations fade in and out, and the anchored menu glows with the theme's overlay glow. `items` is a `ContextMenuItem[]` (`{ label, onSelect?, disabled? }`); pressing outside, Escape and the platform's back close without selecting.

```jsx
import { ContextMenu } from "@solidrt/components"

<ContextMenu
  items={[
    { label: "Rename", onSelect: rename },
    { label: "Delete", onSelect: remove },
    { label: "Share", disabled: true },
  ]}
>
  <Card>{file.name}</Card>
</ContextMenu>
```

API: `ContextMenu`, `ContextMenuProps`, `ContextMenuItem` - typed and commented in [src/context-menu.tsx](./src/context-menu.tsx).

### NavShell

An app shell that arranges primary navigation around the content per `policy.navigation`: bottom tabs under it (`bottomTabs`), a narrow rail (`rail`), or a wide sidebar (`sidebar`) beside it. The content is a single stable node; switching arrangement only flips the shell's flex direction and remounts the stateless nav strip, so page state survives a resize across a breakpoint. `items` is a `NavItem[]` (`{ value, label, icon? }`; the icon renders above the label in tabs/rail, beside it in the sidebar); controlled via `value`/`onChange`, or uncontrolled via `defaultValue`. Safe areas are the caller's concern: wrap the shell in `SafeArea`. With `@solidrt/router` the shell is the tabs route's component, `value`/`onChange` wired to `useTabs()` (`active()`/`select(path)`) and `<Outlet />` as the content; the router then keeps a stack per tab and the tabs mounted.

```jsx
import { NavShell, Icon } from "@solidrt/components"

let items = [
  { value: "home", label: "Home", icon: <Icon src={House} /> },
  { value: "settings", label: "Settings", icon: <Icon src={Cog} /> },
]

<NavShell items={items} value={page()} onChange={setPage} layout={{ flex: 1 }}>
  <Show when={page() === "home"} fallback={<Settings />}>
    <Home />
  </Show>
</NavShell>
```

API: `NavShell`, `NavShellProps`, `NavItem` - typed and commented in [src/nav-shell.tsx](./src/nav-shell.tsx).

### SplitView

A list-detail container driven by `policy.layout`: `twoPane` shows the `list` pane (width `listWidth`, default `theme.size.splitViewList`) beside the `detail` pane, `singlePane` shows one at a time per `showDetail`. Keep pane state (selection, scroll) in the app, not in the panes: crossing a breakpoint re-arranges and can remount them. It draws no chrome and adds no padding; a back affordance in the single-pane detail is the app's to render (fork on `policy.layout`).

```jsx
import { SplitView } from "@solidrt/components"

<SplitView
  layout={{ flex: 1 }}
  list={<Inbox onOpen={setSelected} />}
  detail={<Message id={selected()} onBack={() => setSelected(null)} />}
  showDetail={selected() !== null}
/>
```

API: `SplitView`, `SplitViewProps` - typed and commented in [src/split-view.tsx](./src/split-view.tsx).

### QrCode

Renders a QR code for `data` out of primitives: same-color modules in a row collapse into one box, drawn on a light quiet-zone panel; the grid recomputes only when `data` or `level` changes. It paints black on white by default (not the theme) so it stays scannable through a theme switch; override `color`/`background` only if the contrast still holds. `moduleSize` (default 6) is pixels per module, `margin` (default 16) the quiet zone (keep it non-zero), `level` the error correction (`L`/`M`/`Q`/`H`, default `M`: higher tolerates more damage but caps data length sooner), `radius` the panel's corner radius.

```jsx
import { QrCode } from "@solidrt/components"

<QrCode data="https://solidjs.com" />
<QrCode data={ticket()} moduleSize={8} level="L" />
```

API: `QrCode`, `QrCodeProps` - typed and commented in [src/qrcode.tsx](./src/qrcode.tsx).

### Icon

A thin themed wrapper over the core `parseSvg` primitive. `src` is a whole SVG document as a string; the component parses it once (memoized), maps the draws to `<d-path>` in a square `designSize`-fitted box (`size`, default 24) and, for monochrome icons that stroke/fill with `currentColor`, recolors it via `color` (default the theme text color). It carries no icon set and no name registry, so any `currentColor` SVG works (Lucide, Feather, Heroicons, ...) and only the icons you import are bundled. Multi-color documents keep their own fills. For a non-square box, use `parseSvg` directly.

Icons are just SVG strings: import them as assets (`import House from "lucide-static/icons/house.svg"`, resolved to a string), pull them from a string export, or inline a literal.

```jsx
import { Icon } from "@solidrt/components"
import House from "lucide-static/icons/house.svg"

<Icon src={House} />
<Icon src={House} size={32} color={theme.color.primary} />
```

API: `Icon`, `IconProps` - typed and commented in [src/icon.tsx](./src/icon.tsx).

### Surface

The one way a component draws a face: a role's material (`theme.material`) under the theme light (`theme.light`) at an elevation (`theme.elevation`), stacked as layers that fill the view it is rendered in - a contact shadow, the glow, the fill (tinted by the light, with the sheen gradient along it, carrying the key shadow the elevation casts), the hover/pressed tint, the bevel and the outline. Every themed component draws its chrome through it, so a theme's light, materials and elevation restyle the whole library at once; under the stock presets (flat materials, zero elevation) a face is a plain fill, the tint and the outline, the look the package has always had. A custom control uses it the same way: render it as the first child of the laid-out view it dresses, pick a `role` (never a material), give it the base `fill`, the `radius`, the press state for the tint and the sink, and an `outline` for its border or focus ring.

```jsx
import { Surface, createPress, theme } from "@solidrt/components"

function Tile(props) {
  let press = createPress(props)
  return (
    <view ref={press.ref} {...press.handlers} padding={16} cursor="pointer" focusable>
      <Surface role="surface" elevation="raised" radius={theme.radius.lg} pressed={press.pressed()} hovered={press.hovered()} />
      <text color={theme.color.text}>{props.label}</text>
    </view>
  )
}
```

A pressed face sinks (its key shadow fades toward the contact shadow, its lit edge moves to the far side) on the native shadow transition; a `sunken` face is a well or a groove (shaded like a pressed face, casting nothing); `lifted` with `liftTo` retargets the key shadow to a higher level (the Card hover lift); `chrome={false}` keeps only the tint (a ghost button at rest). `material` merges layers over the role's, `glow` overrides that layer alone. Shadows cast from opaque casters hidden under the fill, since a transparent caster's blurred shadow composites in a save layer of its own, which a tiled GPU pays for per shadow; only a glass material (a frosted backdrop, the overlay role's option) casts from a transparent one. `facePaint` gives the same derivation as plain paint values, for a part a control draws itself (a slider fill, a segmented indicator); `roleFill`, `roleMaterial` and `surfaceSinks` are the pieces it is built from.

API: `Surface`, `facePaint`, `roleFill`, `roleMaterial`, `surfaceSinks`, `SurfaceProps`, `FacePaint` - typed and commented in [src/surface.tsx](./src/surface.tsx).

### lightFrom

The light helpers for an app that moves the theme light. `theme.light.direction` is the way the light travels in screen space (x right, y down, z into the screen); `lightFrom(px, py)` turns a point on the unit disk - where the light comes from, a light pad's coordinates - into that direction, and `lightSource(light)` is its inverse, so a pad shows the current light. `lightAxis(direction)` is the quantized, interned axis every face derives its paint from: equal steps return the same object, which is why a dragged light costs a face a re-derivation only when the axis steps.

```jsx
import { lightFrom, setTheme, theme } from "@solidrt/components"

// A pad: the pointer's position over it, -1..1 per axis, becomes the light.
setTheme({ light: { ...theme.light, direction: lightFrom(px, py) } })
```

The light is data for event-rate changes (a drag, a preset switch, a time of day), not a per-frame animation: every lit face re-derives its paint on each move.

API: `lightFrom`, `lightSource`, `lightAxis`, `LightAxis` - typed and commented in [src/light.ts](./src/light.ts).

### PressFeedback

The hover/pressed tint every pressable face draws over its own fill: one always-mounted detached rect in `overlayHover` while hovered and `overlayPressed` while pressed, at alpha 0 otherwise, fading at the theme's feedback speed. Surface draws it as its tint layer; it is exported for a control that draws its own rows over a face it does not own (the option rows of a Select or ContextMenu).

```jsx
import { PressFeedback, createPress } from "@solidrt/components"

let press = createPress(props)
<view ref={press.ref} {...press.handlers} padding={8}>
  <PressFeedback pressed={press.pressed()} hovered={press.hovered()} radius={6} />
  <text>{props.label}</text>
</view>
```

API: `PressFeedback` - typed and commented in [src/motion.tsx](./src/motion.tsx).

### Density

`<Density value="compact">` overrides the density policy for its subtree: every density-scaled metric below - `space()`, control sizes (Checkbox, Switch, Radio, Slider), `Item` and `Button` paddings - resolves this value instead of the global `policy.density`. Regions nest; the nearest wins. Use it to tighten a toolbar, a data table, or a sidebar without per-child props.

```jsx
import { Density, Item } from "@solidrt/components"

<Density value="dense">
  <For each={rows()}>{(r) => <Item label={r.name} />}</For>
</Density>
```

`densityScale()` is the reactive multiplier behind it (1 / 0.85 / 0.7 for comfortable/compact/dense): the nearest `<Density>` above the calling scope, falling back to `policy.density`. Call it during component setup or inside JSX/thunks when building custom density-aware components.

API: `Density`, `DensityProps`, `densityScale` - typed and commented in [src/density.tsx](./src/density.tsx).

## License

MIT. Copyright (c) 2026 Antoine van Wel.
