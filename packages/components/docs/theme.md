# Theming

Appearance (colors, spacing, border, font roles) comes from one shared, reactive theme backed by a Solid store: reads are tracked, so switching the theme at runtime recolors the live UI without remounting. Four presets ship, `darkTheme` and `lightTheme` (the flat look, default dark) and `litDarkTheme` and `litLightTheme` (the same palette under a light, see below); `setTheme(preset)` switches, `setTheme(partial)` merges an override one level deep per category. A resolved theme carries every key of every category (unset ones as `undefined`), so switching to one replaces the previous theme outright, per-component overrides and icon slots included; a partial keeps whatever it does not name. Custom themes are authored with `defineTheme`.

```jsx
import { setTheme, darkTheme, lightTheme } from "@solidrt/components"

setTheme(lightTheme)                          // switch to light
setTheme(darkTheme)                           // switch to dark
setTheme({ color: { primary: "#ff2d55" } })   // override one token
```

## Authoring with defineTheme

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

## Tokens

The color tokens are `background` (window fill), `surface` (control/card fill), `surfaceAlt` (subtle raised/track fill), `text`, `textMuted`, `border`, `primary`/`onPrimary`, `secondary`/`onSecondary` (lower-emphasis accent), `danger` (validation/destructive), `scrim` (modal dim), `ring` (the focus ring; defaults to `text` so it stays visible on primary fills), `thumb` (optional: the cap riding a track, the Switch knob and the Slider thumb; unset they are `onPrimary` and `primary`, so a palette whose `onPrimary` is dark sets a light cap here), and the feedback pair `overlayHover`/`overlayPressed`: translucent tints components draw OVER a control's own fill, so one token pair gives hover/pressed feedback on every fill color, including caller-set ones. Non-color tokens are `spacing`, `radius`, `borderWidth` (`sm` for borders, `focus` for the ring), `size` (app-wide default extents: `navRail` 72, `navSidebar` 220, `splitViewList` 320, `menuMinWidth` 120, `slider` 200; each overridable per instance through its layout or prop), and `text` (the type scale: `caption`/`label`/`body`/`title`/`heading` roles, each `{ size, lineHeight, weight }`, plus `fontFamily` and `monoFamily` for code).

## Spacing

Spacing is one base unit: `spacing` in a theme definition is a number (default 4) and the steps are multiples of it (`sm` 1x, `md` 2x, `lg` 4x, `xl` 5x). Components read them through `space()`, which applies the density policy on top, so a theme sets the rhythm and density tightens it. Pass an object (`spacing: { sm, md, lg, xl }`, any subset) to pin individual steps.

## Radius

Corner radius is set once: `radius` in a theme definition is a single number, the control radius (default 8), and the scale derives from it: `md` is the base (Button, TextInput, RichTextEditor, Select, SegmentedControl, QrCode), `sm` half of it (Checkbox, Item, NavShell items, Select and ContextMenu popups, Tooltip), `lg` one and a half (Card), and `full` the pill (Badge). Set `radius: 0` for a square theme, `radius: 12` for a soft one; buttons and inputs always match. Shapes derived from a control's own height (Switch, Slider, ProgressBar, Radio) take `full` too, through `pill(size)`: half the size, capped by `radius.full`, so `full: 0` squares every pill and circle in the library (`radius: { sm: 0, md: 0, lg: 0, full: 0 }` is the all-square theme) while the default 9999 leaves them round. `pill` is exported for a custom control with a round part. Pass an object (`radius: { sm, md, lg, full }`, any subset) to pin individual steps instead.

```jsx
setTheme({ radius: 4 })   // sm 2, md 4, lg 6
```

## Motion

`motion` holds the three durations (ms) every built-in component transition draws from, so one theme edit retimes the whole package: `fast` (default 100) is press/hover feedback, `base` (150) the color and opacity fades - state changes, the theme cross-fade (a `setTheme` fades every themed color rather than snapping), popup enter/exit - and `slow` (250) the travel of a control's moving parts (switch knob, segmented indicator, progress fill) and of a face's shadows as it sinks or lifts. `policy.motion` gates whether these play at all; a per-instance `transition` prop overrides them per property.

```jsx
setTheme({ motion: { base: 250, slow: 400 } })   // a slower, calmer app
```

## Light, materials and elevation

Every component draws its faces through `Surface`, and three theme sections decide what a face looks like. `light` is the theme's one light in screen space (`direction` [x right, y down, z into the screen], a `color`, and `ambient`, the fill on the shaded side): shadows fall along it, the sheen and the bevel run from its lit edge to the shaded one, and faces take on its color (a white light leaves every base color as it is). `material` assigns each role its layers: `surface` (Card, Item, the NavShell strips), `control` (a secondary Button, the fields, the Select trigger, the tracks and grooves, the Switch knob and the Slider thumb), `accent` (a primary or danger Button, Switch on, a checked Checkbox, the Radio dot, the Slider and ProgressBar fills, the SegmentedControl indicator, a Badge) and `overlay` (the Tooltip bubble, the Select dropdown, the ContextMenu menu, the sheets). A `Material` is `{ sheen?, bevel?, glass?, glow? }`: `sheen` (0..1) a fill gradient along the light, `bevel` (0..1) a one-pixel stroke from the light color to black, `glass` a frosted backdrop (`{ blur, tint? }`, a deliberate overlay, never a surface material: every glass face captures the pixels under it each frame), `glow` a halo (`{ radius, color? }`; without a color a part glows in its own fill, so a danger button glows red, and a fill with no color to take casts nothing; a pressable face widens it while hovered or pressed). `elevation` maps the four levels (`flat`, `raised`, `floating`, `overlay`) to heights: a face at height h casts a key shadow h px along the light and blurred 2.5 h px, plus a tight contact shadow; a press sinks the face (the key shadow fades toward the contact shadow) on the native shadow transition, and `shadow` (`{ color, strength }`) is what they are cast in.

The stock presets are flat: every material `{}` and every height 0, so nothing casts or shines and the components look as they always have. `litDarkTheme` and `litLightTheme` are the same palette under the light: a sheen and a bevel per role, `raised` 2, `floating` 6, `overlay` 14, a light cap for the Switch knob and Slider thumb, and shadows 1.7 times stronger on dark, where they otherwise vanish. Card and Button stand raised by default (a ghost or disabled button flat), the Switch knob, the Slider thumb and the SegmentedControl indicator too; Item, Badge and the Checkbox sit flat whatever the theme, since a screen holds dozens of them and a blurred shadow per row is what a scrolling list cannot afford on a tablet or a TV (measured in the runtime's notes); popups float. Per component, `theme.components.<name>.material` merges layers over the role's, `.elevation` sets the level and `.glow` the halo alone, and an instance's `style` does the same over that.

```jsx
setTheme(litDarkTheme)                                                        // the lit look
setTheme({ light: { ...theme.light, direction: [-0.4, 1, 0.8] } })            // light from the right
setTheme({ material: { accent: { sheen: 1, bevel: 0.9, glow: { radius: 14 } } } })   // glowing accents
setTheme({ components: { item: { elevation: "raised" } } })                   // raised rows, deliberately
```

## Window finish

`finish` is the theme's window finish: a core window shader declaration (`program`, `params`, and the optional `textures`, `previous`, `vertexCount`) that `Window` declares on the window as it is, so one theme can carry a vignette, scanlines or a color grade. `Window` adds one uniform, `uScale`, the display scale, for effects counted in logical pixels. Strength and layer switches are the theme's own `params`; a theme without a `program` has no finish. An app with a slider updates a param by spreading the current ones, because `setTheme` merges one level deep and a partial replaces the whole `params` object it names: `setTheme({ finish: { params: { ...theme.finish.params, uStrength: v } } })`. The theme's module links the program itself, once, at init (the client brings the GPU up before the bundle runs); the package ships no finish, and the stock presets have none.

```jsx
import { compileShader, linkProgram, destroyShader, glsl } from "@solidrt/core/gpu"

let fs = compileShader("fragment", glsl`...`, { header: true })   // reads uSource, uScale and its own params
let vs = compileShader("vertex", glsl`...`)                        // the covering triangle
let program = linkProgram(vs, fs, { label: "crt-finish" })
destroyShader(vs); destroyShader(fs)

export let crtTheme = defineTheme({ color: { /* ... */ }, finish: { program, params: { uStrength: 0.4, uScan: 1 } } })
```

## Per-component overrides

`theme.components` restyles a component everywhere without wrapping it: a `StyleProps` object per component name, merged between the component's themed defaults and each instance's `style` prop (instance style still wins). Every themed component draws the `borderColor`/`borderWidth` it is given (Item rows, the SegmentedControl, Badge, the Slider and ProgressBar grooves included), so one theme boxes the whole library; `material`, `elevation` and `glow` restyle its faces (see Light, materials and elevation).

```jsx
setTheme({ components: { button: { borderRadius: 999 } } })   // pill buttons app-wide
```

Keys: `button`, `card`, `badge`, `switch`, `checkbox`, `radio`, `slider`, `item`, `select`, `segmentedControl`, `textInput`, `richTextEditor`, `tooltip`, `divider`, `progressBar`, `spinner`, `contextMenu`, `navShell`.

## Icon slots

`theme.icons` holds semantic control glyphs as SVG document strings (the same currency as `Icon`): `chevronDown` (the Select trigger) and `check` (the Checkbox mark). Components draw their built-in vector paths by default; a theme that sets a slot swaps that glyph everywhere it appears, and the package still bundles no icon set.

```jsx
import ChevronDown from "lucide-static/icons/chevron-down.svg"

setTheme({ icons: { chevronDown: ChevronDown } })
```
