# Theming

Appearance (colors, spacing, border, font roles) comes from one shared, reactive theme backed by a Solid store: reads are tracked, so switching the theme at runtime recolors the live UI without remounting. Two presets ship, `darkTheme` and `lightTheme` (default dark); `setTheme(preset)` switches, `setTheme(partial)` merges an override one level deep per category. Custom themes are authored with `defineTheme`.

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

The color tokens are `background` (window fill), `surface` (control/card fill), `surfaceAlt` (subtle raised/track fill), `text`, `textMuted`, `border`, `primary`/`onPrimary`, `secondary`/`onSecondary` (lower-emphasis accent), `danger` (validation/destructive), `scrim` (modal dim), `ring` (the focus ring; defaults to `text` so it stays visible on primary fills), and the feedback pair `overlayHover`/`overlayPressed`: translucent tints components draw OVER a control's own fill, so one token pair gives hover/pressed feedback on every fill color, including caller-set ones. Non-color tokens are `spacing`, `radius`, `borderWidth` (`sm` for borders, `focus` for the ring), `size` (app-wide default extents: `navRail` 72, `navSidebar` 220, `splitViewList` 320, `menuMinWidth` 120, `slider` 200; each overridable per instance through its layout or prop), and `text` (the type scale: `caption`/`label`/`body`/`title`/`heading` roles, each `{ size, lineHeight, weight }`, plus `fontFamily` and `monoFamily` for code).

## Spacing

Spacing is one base unit: `spacing` in a theme definition is a number (default 4) and the steps are multiples of it (`sm` 1x, `md` 2x, `lg` 4x, `xl` 5x). Components read them through `space()`, which applies the density policy on top, so a theme sets the rhythm and density tightens it. Pass an object (`spacing: { sm, md, lg, xl }`, any subset) to pin individual steps.

## Radius

Corner radius is set once: `radius` in a theme definition is a single number, the control radius (default 8), and the scale derives from it: `md` is the base (Button, TextInput, RichTextEditor, Select, SegmentedControl, QrCode), `sm` half of it (Checkbox, Item, NavShell items, Select and ContextMenu popups, Tooltip), `lg` one and a half (Card), and `full` the pill (Badge). Set `radius: 0` for a square theme, `radius: 12` for a soft one; buttons and inputs always match. Shapes derived from a control's own height (Switch, Slider, ProgressBar, Radio) are not on the scale. Pass an object (`radius: { sm, md, lg, full }`, any subset) to pin individual steps instead.

```jsx
setTheme({ radius: 4 })   // sm 2, md 4, lg 6
```

## Per-component overrides

`theme.components` restyles a component everywhere without wrapping it: a `StyleProps` object per component name, merged between the component's themed defaults and each instance's `style` prop (instance style still wins).

```jsx
setTheme({ components: { button: { borderRadius: 999 } } })   // pill buttons app-wide
```

Keys: `button`, `card`, `badge`, `switch`, `checkbox`, `radio`, `item`, `select`, `segmentedControl`, `textInput`, `richTextEditor`, `tooltip`, `divider`, `progressBar`, `spinner`.

## Icon slots

`theme.icons` holds semantic control glyphs as SVG document strings (the same currency as `Icon`): `chevronDown` (the Select trigger) and `check` (the Checkbox mark). Components draw their built-in vector paths by default; a theme that sets a slot swaps that glyph everywhere it appears, and the package still bundles no icon set.

```jsx
import ChevronDown from "lucide-static/icons/chevron-down.svg"

setTheme({ icons: { chevronDown: ChevronDown } })
```
