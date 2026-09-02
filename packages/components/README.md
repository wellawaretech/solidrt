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

Appearance (colors, spacing, border, font roles) comes from one shared, reactive theme backed by a Solid store: reads are tracked, so switching the theme at runtime recolors the live UI without remounting. Two presets ship, `darkTheme` and `lightTheme` (default dark); `setTheme(preset)` switches, `setTheme(partial)` merges an override one level deep per category. Custom themes are authored with `defineTheme`.

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

The color tokens are `background` (window fill), `surface` (control/card fill), `surfaceAlt` (subtle raised/track fill), `text`, `textMuted`, `border`, `primary`/`onPrimary`, `secondary`/`onSecondary` (lower-emphasis accent), `danger` (validation/destructive), `scrim` (modal dim), `ring` (the focus ring; defaults to `text` so it stays visible on primary fills), and the feedback pair `overlayHover`/`overlayPressed`: translucent tints components draw OVER a control's own fill, so one token pair gives hover/pressed feedback on every fill color, including caller-set ones. Non-color tokens are `spacing`, `radius`, `borderWidth` (`sm` for borders, `focus` for the ring), `size` (app-wide default extents: `navRail` 72, `navSidebar` 220, `splitViewList` 320, `menuMinWidth` 120, `slider` 200; each overridable per instance through its layout or prop), and `text` (the type scale: `caption`/`label`/`body`/`title`/`heading` roles, each `{ size, lineHeight, weight }`, plus `fontFamily` and `monoFamily` for code).

### Spacing

Spacing is one base unit: `spacing` in a theme definition is a number (default 4) and the steps are multiples of it (`sm` 1x, `md` 2x, `lg` 4x, `xl` 5x). Components read them through `space()`, which applies the density policy on top, so a theme sets the rhythm and density tightens it. Pass an object (`spacing: { sm, md, lg, xl }`, any subset) to pin individual steps.

### Radius

Corner radius is set once: `radius` in a theme definition is a single number, the control radius (default 8), and the scale derives from it: `md` is the base (Button, TextInput, RichTextEditor, Select, SegmentedControl, QrCode), `sm` half of it (Checkbox, Item, NavShell items, Select and ContextMenu popups, Tooltip), `lg` one and a half (Card), and `full` the pill (Badge). Set `radius: 0` for a square theme, `radius: 12` for a soft one; buttons and inputs always match. Shapes derived from a control's own height (Switch, Slider, ProgressBar, Radio) are not on the scale. Pass an object (`radius: { sm, md, lg, full }`, any subset) to pin individual steps instead.

```jsx
setTheme({ radius: 4 })   // sm 2, md 4, lg 6
```

### Motion

`motion` holds the three durations (ms) every built-in component transition draws from, so one theme edit retimes the whole package: `fast` (default 100) is press/hover feedback, `base` (150) the color and opacity fades - state changes, the theme cross-fade (a `setTheme` fades every themed color rather than snapping), popup enter/exit - and `slow` (250) the travel of a control's moving parts (switch knob, segmented indicator, progress fill). `policy.motion` gates whether these play at all; a per-instance `transition` prop overrides them per property.

```jsx
setTheme({ motion: { base: 250, slow: 400 } })   // a slower, calmer app
```

### Per-component overrides

`theme.components` restyles a component everywhere without wrapping it: a `StyleProps` object per component name, merged between the component's themed defaults and each instance's `style` prop (instance style still wins).

```jsx
setTheme({ components: { button: { borderRadius: 999 } } })   // pill buttons app-wide
```

Keys: `button`, `card`, `badge`, `switch`, `checkbox`, `radio`, `item`, `select`, `segmentedControl`, `textInput`, `richTextEditor`, `tooltip`, `divider`, `progressBar`, `spinner`.

### Icon slots

`theme.icons` holds semantic control glyphs as SVG document strings (the same currency as `Icon`): `chevronDown` (the Select trigger) and `check` (the Checkbox mark). Components draw their built-in vector paths by default; a theme that sets a slot swaps that glyph everywhere it appears, and the package still bundles no icon set.

```jsx
import ChevronDown from "lucide-static/icons/chevron-down.svg"

setTheme({ icons: { chevronDown: ChevronDown } })
```

API: `theme`, `setTheme`, `defineTheme`, `darkTheme`, `lightTheme`, `Theme`, `ThemeDefinition`, `ThemeColor`, `ThemedComponent`, `TextStyle`, `TextVariant` - typed and commented in [src/theme.ts](./src/theme.ts).

## Policies

Theme answers "how does it look"; policies answer "how does it behave". `policy` is a second reactive layer derived from the platform facts in `@solidrt/core` (`capabilities`, `env`), so components adapt to touch vs. desktop, window size, and display without every app wiring that logic itself. Reads are reactive like `theme`: a window resize or the first mouse move on a touch-capable device updates every consuming component live.

The fields:

- `interaction` (`"touch" | "desktop" | "hybrid"`) - which affordances a component shows (hover states vs. long-press). `Tooltip`, `Select`, and `ContextMenu` fork on it.
- `density` (`"comfortable" | "compact" | "dense"`) - control/hit-target/spacing scale; drives `densityScale()` (1 / 0.85 / 0.7). A `<Density>` region overrides it per subtree.
- `motion` (`"normal" | "reduced" | "none"`) - animation intensity. Gates every built-in component transition: `reduced` keeps the color/opacity fades (a fade is not movement) but snaps everything that travels or scales, and halves the indeterminate `Spinner`/`ProgressBar` speeds; `none` snaps it all and parks the indeterminate loops.
- `focusRing` (`boolean`) - whether focused controls draw a visible focus indicator (true when a keyboard or gamepad/remote is present).
- `textScale` (`number`) - multiplier on type-scale font sizes; defaults to the OS text-scale preference.
- `textWeightDelta` (`number`) - weight compensation (steps of 100) for light-on-dark text on low-DPI displays.
- `navigation` (`"bottomTabs" | "rail" | "sidebar"`) - recommended nav layout, derived from the pane count: `sidebar` beside a two-pane layout, `bottomTabs` under a single pane (a side strip spends the width a narrow window is short of). `rail` is never derived; set it for a content-dense two-pane app. `NavShell` follows it.
- `layout` (`"singlePane" | "twoPane"`) - recommended pane count, derived from the window size class. `SplitView` follows it.

```jsx
setPolicy({ density: "compact" })    // pin a field, overriding the derived value
setPolicy({ density: undefined })    // hand it back to the resolver
```

`setPolicyResolver((caps) => Policies)` replaces the whole system-derivation function for full custom control; `defaultPolicyResolver` is exported to wrap or extend instead of replacing it outright.

API: `policy`, `setPolicy`, `setPolicyResolver`, `defaultPolicyResolver`, `Policies`, `PolicyResolver`, `InteractionPolicy`, `DensityPolicy`, `MotionPolicy`, `NavigationPolicy`, `LayoutPolicy` - typed and commented in [src/policy.ts](./src/policy.ts).

## Layout and style

Most components group their props into two objects, split by one rule: `layout` properties feed the layout engine (flexbox/grid, sizing, padding, margin, position - the core `LayoutProps` set) and changing them triggers a relayout; `style` properties are paint-only and never affect layout: `color`, `backgroundColor`, `borderColor`, `borderWidth`, `borderRadius`, `opacity`, and the transform (`x`, `y`, `scale`, `rotate`, `rotateX`/`rotateY` with `perspective`, `originX`/`originY`, `clipRadius`). Event handlers (`onPointerDown`, `onKeyDown`, ...) are top-level props, never inside `layout` or `style`.

`StyleProps` is that paint set. `TextLayoutProps` extends `LayoutProps` with the font fields (`fontFamily`, `fontSize`, `lineHeight`, `fontStyle`, `fontWeight`, `textAlign`, `maxLines`) because text shaping affects measurement; note `lineHeight` is a multiplier of `fontSize` (the theme uses 1.3-1.6), not a pixel value. `Option` (`{ value, label }`) is the shared shape of the single-choice controls (`Select`, `SegmentedControl`): shared shapes go through this module so components never import a sibling.

`TransitionProps` (`transition`, `onTransitionEnd`) is the third top-level group, in the component's own vocabulary rather than core's: a declaration names the view-level properties (`opacity`, `x`, `y`, `scale*`, `rotate*`, `origin*`, `perspective`, `clipRadius`) and the style ones (`backgroundColor`, `borderColor`, `borderWidth`, `borderRadius`), plus `all`, a shorthand string, and `stagger` - `<Button transition={{ backgroundColor: { duration: 300 }, opacity: "200ms ease-out" }}>`. Core's paint names (`color`, `radius`, `strokeWidth`) are rejected by the types: a component is a root view plus the rects it draws for `style`, and `splitTransition` hands each entry to the node that owns it (the background rect gets `backgroundColor`/`borderRadius`, the stroke rect `borderColor`/`borderWidth`/`borderRadius`, the root view the rest). `onTransitionEnd` reports the component name (`backgroundColor`, not `color`). `Text` adds `color` (its text node), `ScrollView` adds `scrollX`/`scrollY` (its viewport).

Controls with a moving part of their own name it as an extra entry: `Switch` `knob` (the thumb's travel), `SegmentedControl` `indicator` (the active-segment slide), `ProgressBar` `fill` (the determinate glide) - `<Switch transition={{ knob: "150ms" }}>` retimes just that part. `Slider` deliberately has no parts: its thumb and fill track the drag 1:1, and a transition would rubber-band it.

The components also ship built-in motion with no props at all: state and theme colors fade, a press shrinks the free-standing controls on a quick spring and fades the overlay tints, marks (checkmark, radio dot) pop in and out, moving parts travel on springs, and the overlays (`Modal`, `Tooltip`, the `Select`/`ContextMenu` popups) fade in and out. Timing comes from `theme.motion` (`fast`/`base`/`slow`), and `policy.motion` gates it: `reduced` keeps the fades but snaps everything that moves, `none` snaps it all. A caller's `transition` entry overrides the built-in for that property, and `transition={null}` suppresses a component's built-ins outright.

API: `StyleProps`, `TextLayoutProps`, `Option`, `TransitionProps`, `ComponentTransition`, `TransitionViewProp`, `TransitionStyleProp`, `TransitionScrollProp` - typed and commented in [src/types.ts](./src/types.ts).

## Typography helpers

`typeStyle(variant)` resolves a theme type-scale role (`caption`/`label`/`body`/`title`/`heading`) to font props ready to spread onto a `<text>` or `d-text`: `fontSize` carries `policy.textScale`, and `fontWeight` carries the low-DPI weight compensation. Reactive when called inside a tracked scope, like any theme/policy read. `Text` applies it for you; reach for the helpers when building custom text out of core primitives.

The compensation exists because the renderer rasterizes glyphs unhinted and composites in nonlinear sRGB, which thins light-on-dark text on low-DPI displays as glyphs shrink. `typeWeight(weight, size, onDark?)` adds `policy.textWeightDelta` (0 on high-DPI displays) plus one extra step below 16px; dark-on-light text passes through untouched. `lightOnDark(text, fill)` computes the polarity for a known pair of colors (Button uses it for its fills); omitted, the theme's own palette polarity is used.

API: `typeStyle`, `typeWeight`, `lightOnDark` - typed and commented in [src/typography.ts](./src/typography.ts).

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

Text input, single-line by default; `multiline` wraps at the field's width and edits across lines (Enter inserts a newline, Up/Down move by line; grows with content up to `maxRows` unless `layout.height` fixes the box, and scrolls to the caret). Controlled via `value`/`onInput`, or uncontrolled via `defaultValue`; `onSubmit` fires on Enter (single-line only). Also `placeholder`, `maxLength`, `autoFocus`, `disabled`, `onFocus`/`onBlur`, and `hints` for IME behavior (keyboard type, capitalization, autocorrect - identifier-like fields want `{ capitalize: "none", autocorrect: false }`).

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

`style` overrides the themed colors, border, and radius. `autoFocus` focuses on mount (the on-screen keyboard still waits for a tap).

API: `TextInput`, `TextInputProps` - typed and commented in [src/text-input.tsx](./src/text-input.tsx).

### RichTextEditor

Edits a rich text `Document` (styled runs, paragraph attributes) in the same field as `TextInput`: always multiline, same caret, keys, wrapping, and scrolling. Controlled via `value`/`onInput`, or uncontrolled via `defaultValue` (start from `plainDocument("")`). Formatting is driven through `editorRef`, which hands you the document buffer - the component ships no toolbar; the app renders its own controls.

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

A scrollable region; vertical by default, `horizontal` to flip. Both the wheel and dragging scroll the content: the drag activates after a small movement threshold along the scroll axis, also when it starts on a pressable (the press is cancelled and its feedback retracts), and keeps scrolling when the pointer leaves the box. Scrolling glides: the offset springs to each new target (250 ms, critically damped), so a wheel notch never jumps and a burst of notches reads as one motion; a dragging finger is tracked exactly, without the spring. No momentum/fling yet.

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

The range changes whenever the content or the viewport changes size. The first fill (nothing was scrollable before it, whether it mounted with the view or arrived a second later) lands instantly, as a chat opens at its end; after that the view follows the end only if it was at the previous end, and the spring makes that follow a glide. The handle arrives once the component has settled, after an effect's first compute, so hold it in a signal (a setter can be passed as the ref) rather than a plain variable, which the effect would find unset and never track. The offset is read untracked: the policy reacts to the range, not to every scroll.

A `scrollX`/`scrollY` entry in `transition` replaces the default spring: `transition={{ scrollY: { duration: 400, bounce: 0.2 } }}` (keep it a spring rather than a tween, because the wheel retargets mid-flight). The other entries animate the box itself and its background/border as on any component.

The underlying geometry primitive `createScroll` is available from `@solidrt/core` for building custom scrollers.

API: `ScrollView`, `ScrollViewProps` - typed and commented in [src/scroll-view.tsx](./src/scroll-view.tsx).

### Pressable

A pressable box: `onPress` fires on a primary-button press released over the box; a drag out of the box (or a non-primary button) does not fire it, and a drag back in restores the pressed state. `children` and `style` may each be a function of the live `{ pressed, hovered, pending }` state, so the box restyles on press/hover without extra signals - read the state inside the prop or child expression, never eagerly into a local.

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

`disabled` takes no pointer events. When pressables nest, the innermost one wins the press. An `onPress` returning a promise sets `pending` until it settles; presses meanwhile are ignored, so async actions cannot double-fire.

API: `Pressable`, `PressableProps`, `PressState` - typed and commented in [src/pressable.tsx](./src/pressable.tsx).

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

A focused Button (see `createFocusNav`) draws a ring in the theme `ring` color under the `focusRing` policy (text-colored by default so it stays visible on primary-filled buttons) and activates on Enter, Space, or a remote's center key. `focusable` (default true) opts out of focus-navigation candidacy; disabled buttons are never candidates.

API: `Button`, `ButtonProps`, `ButtonVariant` - typed and commented in [src/button.tsx](./src/button.tsx).

### createFocusNav

Focus navigation for pointer-free control (TV remote, keyboard, gamepad), moving real focus across the elements declaring `focusable`. Two movement types over the same candidates: spatial (arrow keys, dpad) picks the nearest candidate in the pressed direction by on-screen boxes, and sequential (Tab / Shift+Tab) walks visual reading order - rows top to bottom, left to right - wrapping at the ends. Enter / remote center / gamepad south activates the focused control. Nothing is focused until the first navigation press; pointer input works unchanged throughout. Every interactive control (Button, Item, TextInput, RichTextEditor, Checkbox, Radio, Switch, Select and its options, SegmentedControl segments, Slider) is a candidate unless disabled, and draws the theme's `ring` color at `borderWidth.focus` while focused under the `focusRing` policy; the Slider steps its value with the arrow keys.

```jsx
import { createFocusNav } from "@solidrt/components"

function App() {
  let nav = createFocusNav()
  return <window onKeyDown={nav.onKeyDown}>...</window>
}
```

Attaching `nav.onKeyDown` on the window is what keeps it cooperative: key events bubble from the focused node, so a focused TextInput keeps its arrow keys and navigation only sees what nothing else consumed. Gamepad dpad/south are wired automatically.

When the focused control disappears (an action replacing it, a screen change), focus lands on the nearest candidate to where it sat as soon as the successor is laid out - the ring follows a Disconnect button into the Connect button that replaces it. A deliberate blur (tapping outside, dismissing the keyboard) stays blurred; the next press resumes at the nearest candidate.

An open `Modal` traps navigation inside itself with no extra wiring (topmost wins when stacked); pass `scope: () => nodeOrNull` to trap into some other subtree instead. `move`/`tab`/`activate` are exposed for custom triggers.

API: `createFocusNav`, `FocusNavOptions` - typed and commented in [src/focus-nav.ts](./src/focus-nav.ts).

### Switch

An on/off toggle: the track fills with `primary` when on and `surfaceAlt` when off (a fade), and the thumb springs across - the `knob` transition entry retimes that travel. Controlled via `value`/`onChange`, or uncontrolled via `defaultValue`. Built on `Pressable`, so `disabled` takes no pointer events. `style` overrides the track colors and radius.

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

A checkbox: filled with `primary` and a drawn checkmark when checked, an empty bordered box otherwise - the fill fades and the mark pops in and out. Controlled via `checked`/`onChange`, or uncontrolled via `defaultChecked`. The mark is the `theme.icons.check` slot when a theme sets one. `style` overrides the box colors, border, and radius.

```jsx
import { Checkbox } from "@solidrt/components"

<Checkbox checked={agree()} onChange={setAgree} />
```

API: `Checkbox`, `CheckboxProps` - typed and commented in [src/checkbox.tsx](./src/checkbox.tsx).

### RadioGroup / Radio

A single-selection pair: `RadioGroup` owns the selected value (controlled via `value`/`onChange`, or uncontrolled via `defaultValue`) and shares it with its `Radio` children; each `Radio` is a ring with an inner dot when selected - the ring color fades and the dot pops in and out. A string/number child of `Radio` renders as a themed label beside the ring; anything else as-is. `disabled` on the group disables every option, on a `Radio` just that one.

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

A horizontal slider: the groove fills up to the thumb, and pressing or dragging the track sets the value from the pointer position. Controlled via `value`/`onChange` (fires while dragging), or uncontrolled via `defaultValue` (defaults to `min`). `min`/`max` default to 0/100; `step` snaps to an increment, omitted the value is continuous. The drag keeps tracking when the pointer drifts off the track, and an enclosing ScrollView never takes it over.

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

With `onPress` the row is interactive: hover/pressed overlay tints (no scale - rows sit flush in a list), focusable for spatial navigation, Enter/remote activation, and a focus ring under the `focusRing` policy. An async `onPress` (returning a promise) is not re-fired until it settles. Without `onPress` the row attaches no press recognizer, so controls inside it (a Switch in a settings row) and enclosing pressables receive pointer events untouched; interactivity is decided at mount. `selected` fills the row with `surfaceAlt`; `disabled` dims the row and takes no pointer events. Separate rows with `Divider` where needed.

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

A small rounded pill for counts, labels, and status. `variant` picks the role: `primary` (accent fill, the default), `neutral` (subtle surface), `danger`. A string/number child renders as the themed label, anything else as-is (an icon, a dot, ...). Override the fill via `style.backgroundColor` and the label color via `style.color`.

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

A horizontal progress bar: determinate when given a `value` in `[0, 1]` (the fill grows from the left, gliding to each new value - the `fill` transition entry retimes it), indeterminate when `value` is undefined (a short segment slides back and forth, driven by core `onFrame`). Track is `surfaceAlt`, fill is `primary`; override via `style.backgroundColor` (track) and `style.color` (fill).

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

A centered overlay rendered at the window root via core `createPortal`: it fills the window with a dimming backdrop (theme `scrim`; override via `backdropColor`, `"transparent"` for no dim) and centers `children` on top, the whole overlay fading in at mount and out on removal (an exiting modal takes no hits). Control visibility by mounting/unmounting it, e.g. `<Show when={open()}>`; the gating signal must start false since portals cannot mount during the initial render. Pressing the backdrop calls `onClose` (unless `dismissable` is false), pressing the content does not, and while mounted the modal traps `createFocusNav` inside itself.

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

A hover-only affordance: under the `desktop`/`hybrid` interaction policies, resting a mouse pointer on the wrapped content shows a bubble near it after `delay` (default 500ms). Under the `touch` policy it never shows, so tooltip content must stay non-essential. The bubble is portal-mounted at the window root, clamped to the window edges, takes no pointer events, fades in and out, and hides on leave and on press. A string/number `content` renders as themed body text; anything else as-is. `placement` picks the side (`"top"`, the default, or `"bottom"`).

```jsx
import { Tooltip, Button } from "@solidrt/components"

<Tooltip content="Save (Ctrl+S)">
  <Button onPress={save}>Save</Button>
</Tooltip>
```

API: `Tooltip`, `TooltipProps` - typed and commented in [src/tooltip.tsx](./src/tooltip.tsx).

### Select

A single-choice picker whose presentation forks on the interaction policy: `desktop`/`hybrid` opens an anchored dropdown under the trigger (flipping above when there is no room), `touch` opens a bottom sheet over a scrim. Same contract either way: `options` is an `Option[]` (`{ value, label }`), controlled via `value`/`onChange` or uncontrolled via `defaultValue`; pressing outside closes without a change. `placeholder` shows in the trigger while nothing is selected. Both presentations fade in and out, and the trigger's chevron flips while open. The option list is not scrollable yet, so keep it short. The chevron is the `theme.icons.chevronDown` slot when a theme sets one.

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

A single-choice row of equal-width segments joined flush: only the control's outermost corners are rounded, hairline dividers separate the segments, and the active segment is one `primary` indicator that springs between segments on a selection change - the `indicator` transition entry retimes it. Hovered segments tint with the theme `overlayHover` under non-touch interaction policies. `options` is an `Option[]`; controlled via `value`/`onChange`, or uncontrolled via `defaultValue`. Override the inactive fill via `style.backgroundColor` and the outer radius via `style.borderRadius`.

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

Secondary actions on the wrapped content. The opening gesture follows the physical pointer: right-click for a mouse, long-press (500ms, cancelled by finger travel) for touch. The presentation forks on the interaction policy: `touch` gets a bottom sheet over a scrim, `desktop`/`hybrid` an anchored menu at the pointer that flips up near the bottom edge. Both presentations fade in and out. `items` is a `ContextMenuItem[]` (`{ label, onSelect?, disabled? }`); pressing outside closes without selecting.

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

An app shell that arranges primary navigation around the content per `policy.navigation`: bottom tabs under it (`bottomTabs`), a narrow rail (`rail`), or a wide sidebar (`sidebar`) beside it. The content is a single stable node; switching arrangement only flips the shell's flex direction and remounts the stateless nav strip, so page state survives a resize across a breakpoint. `items` is a `NavItem[]` (`{ value, label, icon? }`; the icon renders above the label in tabs/rail, beside it in the sidebar); controlled via `value`/`onChange`, or uncontrolled via `defaultValue`. Safe areas are the caller's concern: wrap the shell in `SafeArea`.

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
