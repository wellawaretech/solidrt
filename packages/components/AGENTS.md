# @solidrt/components - agent notes

Higher-level components built on @solidrt/core primitives. Optional: an app can
be built with core primitives alone. For the underlying element model, events,
reactivity, and how to run/verify, see @solidrt/core and @solidrt/cli (their
AGENTS.md). Per-module prose is in docs/ (one file per module, generated into
the README); props are the typed, commented interfaces in src/.

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

Core intrinsics take these props flat instead - there are no `layout`/`style`
objects at that level - and the two compose freely in one tree.

## Traps

- `Window`/`View` do not paint on their own; they only paint when you set
  `style.backgroundColor`/`borderColor` etc. There is no separate background
  element to place by hand (that is the core-only pattern).
- There is no `onClick`. `onPress` comes from `Pressable`/`Button`; raw
  `View` only has `onPointerDown` and friends.
- `ScrollView` needs an explicit main-axis size - a height, or flex inside a
  sized parent. With neither it resolves to 0 and its content silently
  vanishes; `maxHeight` alone does not size it. The runtime warns.
- `Modal`/`Portal` cannot mount during the initial render: gate them behind
  a signal that starts false.
- Cover/contain images: give `Image` a `fit` prop (`"fill" | "cover" |
  "contain" | "none" | "scale-down"`, CSS object-fit semantics, centered)
  plus a box via `layout` in any form. Without `fit`, only NUMERIC layout
  sizes reach the image; `width: pct(100)` alone draws at intrinsic size.
  `fit="cover"` is the ported-web hero-image/thumbnail pattern.
- Reach for @solidrt/core directly only for what components does not wrap:
  raw intrinsics and the `d-` primitives (`d-rect`/`d-path`/`d-oval`) for
  vector art or perf-sensitive positioned drawing, device/GPU subpath
  imports (@solidrt/core/camera, /microphone, /gpu), gradients, and
  createImage/decodeImage below `Image`'s level. Dropping to a `<d-path>`
  for one custom shape never means giving up `View`/`Text` elsewhere.
- Focus navigation: `createFocusNav` moves real focus across `focusable`
  elements (Button is focusable by default; Pressable opt-in) - spatially on
  arrows/dpad, sequentially on Tab/Shift+Tab (reading order, wrapping).
  Attach its onKeyDown to the window; gamepad dpad/south wire automatically.
  A focused press control activates on Enter/Space/remote-select via
  createPress and draws Button's ring under `policy.focusRing` (true when a
  keyboard OR gamepad/remote is present). An open Modal traps navigation
  inside itself automatically.

## Exports

One bullet per module, generated from the first paragraph of its docs/ file.

<!-- BEGIN GENERATED: exports (bun scripts/build-components-docs.ts) -->
- `Window` - The root surface of an app: renders a core `<window>`, so `render()` accepts it. Applies `layout` and `style.backgroundColor` only (a window cannot be transformed or bordered), plus `title` and `fullscreen`.
- `View` - A general-purpose box. Spreads `layout` onto the underlying view, applies the transform from `style`, and draws a background and/or border when those style props are set. Takes all pointer event props.
- `Text` - Themed text in a layout box. `variant` picks a typography role from the theme's type scale (`caption`/`label`/`body`/`title`/`heading`, default `body`); `color` picks a semantic theme color (`text`, `textMuted`, `primary`, `onPrimary`, `danger`, default `text`), with `muted` as sugar for `color="textMuted"`. Font fields go in `layout` (they affect measurement) and individually override the role; `style.color` still wins over `color`.
- `Image` - Loads and displays an image from a URL or raw bytes (`src: string | Uint8Array`). URL loads are shared runtime-wide: mounts of the same URL reuse one fetch and one texture, and the bytes are cached on disk (fetched with `cache: "force-cache"` - no freshness check, so use versioned URLs for content that changes). Concurrent asset fetches are kept polite with a per-host limit; a failed load rejects the mounts sharing it and a later remount retries.
- `SafeArea` - Wraps its children in a view padded clear of system UI intrusions (status bars, home indicators, notches). Top and bottom insets are applied by default; pass `false` to opt out of an edge, or a number to apply the inset with that minimum padding.
- `TextInput` - Text input, single-line by default; `multiline` wraps at the field's width and edits across lines (Enter inserts a newline, Up/Down move by line; grows with content up to `maxRows` unless `layout.height` fixes the box, and scrolls to the caret). Controlled via `value`/`onInput`, or uncontrolled via `defaultValue`; `onSubmit` fires on Enter (single-line only). Also `placeholder`, `maxLength`, `autoFocus`, `disabled`, `onFocus`/`onBlur`, and `hints` for IME behavior (keyboard type, capitalization, autocorrect - identifier-like fields want `{ capitalize: "none", autocorrect: false }`).
- `RichTextEditor` - Edits a rich text `Document` (styled runs, paragraph attributes) in the same field as `TextInput`: always multiline, same caret, keys, wrapping, and scrolling. Controlled via `value`/`onInput`, or uncontrolled via `defaultValue` (start from `plainDocument("")`). Formatting is driven through `editorRef`, which hands you the document buffer - the component ships no toolbar; the app renders its own controls.
- `Document model` - The value model behind `RichTextEditor`: a `Document` is `{ text, runs, blocks }` - plain text plus attributed runs (inline formatting) and per-paragraph blocks. `plainDocument(text)` builds one from a string; `createDocumentBuffer(doc)` wraps one in the editing API (`format`, `formatBlock`, `insertAtom`, `attributes`, selection and edits) that `editorRef` hands out. `ATOM` is the inline-atom placeholder character (U+FFFC) for embedded objects.
- `ScrollView` - A scrollable region; vertical by default, `horizontal` to flip. Both the wheel and dragging scroll the content: the drag activates after a small movement threshold along the scroll axis, also when it starts on a pressable (the press is cancelled and its feedback retracts), and keeps scrolling when the pointer leaves the box. Scrolling glides: the offset springs to each new target (250 ms, critically damped), so a wheel notch never jumps and a burst of notches reads as one motion; a dragging finger is tracked exactly, without the spring. No momentum/fling yet.
- `Pressable` - A pressable box: `onPress` fires on a primary-button press released over the box; a drag out of the box (or a non-primary button) does not fire it, and a drag back in restores the pressed state. `children` and `style` may each be a function of the live `{ pressed, hovered, pending }` state, so the box restyles on press/hover without extra signals - read the state inside the prop or child expression, never eagerly into a local.
- `Button` - A themed press target over `Pressable`: a padded, centered box with a label. `variant` picks the visual role - `primary` (accent fill, the default), `secondary`, `ghost` (no fill until hover), `danger` (destructive) - with fill, hover tint, and label color from the matching theme tokens; no variant draws a border. `size` (`sm`/`md`/`lg`) pins a minimum width so a row of buttons lines up (a longer label still expands past it); omitted, the button sizes to its content. A string or number child renders as the themed label; any other child renders as-is (an icon, a row, ...).
- `createFocusNav` - Focus navigation for pointer-free control (TV remote, keyboard, gamepad), moving real focus across the elements declaring `focusable`. Two movement types over the same candidates: spatial (arrow keys, dpad) picks the nearest candidate in the pressed direction by on-screen boxes, and sequential (Tab / Shift+Tab) walks visual reading order - rows top to bottom, left to right - wrapping at the ends. Enter / remote center / gamepad south activates the focused control. Nothing is focused until the first navigation press; pointer input works unchanged throughout. Every interactive control (Button, Item, TextInput, RichTextEditor, Checkbox, Radio, Switch, Select and its options, SegmentedControl segments, Slider) is a candidate unless disabled, and draws the theme's `ring` color at `borderWidth.focus` while focused under the `focusRing` policy; the Slider steps its value with the arrow keys.
- `Switch` - An on/off toggle: the track fills with `primary` when on and `surfaceAlt` when off; the thumb slides across. Controlled via `value`/`onChange`, or uncontrolled via `defaultValue`. Built on `Pressable`, so `disabled` takes no pointer events. `style` overrides the track colors and radius.
- `Checkbox` - A checkbox: filled with `primary` and a drawn checkmark when checked, an empty bordered box otherwise. Controlled via `checked`/`onChange`, or uncontrolled via `defaultChecked`. The mark is the `theme.icons.check` slot when a theme sets one. `style` overrides the box colors, border, and radius.
- `RadioGroup / Radio` - A single-selection pair: `RadioGroup` owns the selected value (controlled via `value`/`onChange`, or uncontrolled via `defaultValue`) and shares it with its `Radio` children; each `Radio` is a ring with an inner dot when selected. A string/number child of `Radio` renders as a themed label beside the ring; anything else as-is. `disabled` on the group disables every option, on a `Radio` just that one.
- `Slider` - A horizontal slider: the groove fills up to the thumb, and pressing or dragging the track sets the value from the pointer position. Controlled via `value`/`onChange` (fires while dragging), or uncontrolled via `defaultValue` (defaults to `min`). `min`/`max` default to 0/100; `step` snaps to an increment, omitted the value is continuous. The drag keeps tracking when the pointer drifts off the track, and an enclosing ScrollView never takes it over.
- `Card` - A themed surface container: a padded column box with a `surface` fill, a subtle `border` stroke, and rounded corners, recoloring live on a theme switch. Pass a `title` for a heading, or lay out the content yourself; override paint via `style`, spacing/sizing via `layout`.
- `Item` - A list row: `startContent` (icon, avatar, checkbox), a `label` with an optional `description` under it, and `endContent` (badge, timestamp, action) pushed to the end. String/number label and description render as themed body and muted body text; anything else as-is. The dense-data workhorse: rows compose with `<For>` inside a plain column view or `ScrollView` - there is no List wrapper, because a column IS the list. Paddings and gaps are density-scaled, so a `<Density>` region compacts rows wholesale.
- `Field` - A form row: `label` above the control, the control itself (`children`, rendered as-is), and a help or error line below. `error` renders in the danger color and replaces `description` while set. It draws no chrome and does not reach into the control - error styling of the input itself stays the input's `style` prop, no hidden magic. The message line only occupies space while there is one; reserve the space with a constant `description` if the form must not jump when an error appears.
- `Divider` - A thin rule in the theme `border` color. It stretches across its container on the cross axis: full width inside a column, full height inside a row (pass `orientation="vertical"`). `thickness` defaults to 1px; add spacing with `layout` margins, and override the color via `style.backgroundColor`.
- `Badge` - A small rounded pill for counts, labels, and status. `variant` picks the role: `primary` (accent fill, the default), `neutral` (subtle surface), `danger`. A string/number child renders as the themed label, anything else as-is (an icon, a dot, ...). Override the fill via `style.backgroundColor` and the label color via `style.color`.
- `Spinner` - An indeterminate spinner: a 270-degree arc that rotates continuously, driven by core `onFrame`, so it participates in demand-driven rendering and stops when unmounted. `size` (diameter, default 24), `thickness` (default 3), `speed` (revolutions per second, default 1). Color comes from the theme `primary`; override via `style.color`.
- `ProgressBar` - A horizontal progress bar: determinate when given a `value` in `[0, 1]` (the fill grows from the left), indeterminate when `value` is undefined (a short segment slides back and forth, driven by core `onFrame`). Track is `surfaceAlt`, fill is `primary`; override via `style.backgroundColor` (track) and `style.color` (fill).
- `Portal` - Renders its child somewhere other than its lexical position: by default at the window root, so overlays (modals, menus, tooltips) escape the clipping and stacking of their surrounding layout; `mount` targets another node captured from a `ref` instead. A thin JSX wrapper over core `createPortal`. The child should be a single element with `position="absolute"`, since it is inserted into the window's flex root. Portals cannot mount during the app's initial render, so gate them behind a signal that starts false.
- `Modal` - A centered overlay rendered at the window root via core `createPortal`: it fills the window with a dimming backdrop (theme `scrim`; override via `backdropColor`, `"transparent"` for no dim) and centers `children` on top. Control visibility by mounting/unmounting it, e.g. `<Show when={open()}>`; the gating signal must start false since portals cannot mount during the initial render. Pressing the backdrop calls `onClose` (unless `dismissable` is false), pressing the content does not, and while mounted the modal traps `createFocusNav` inside itself.
- `Tooltip` - A hover-only affordance: under the `desktop`/`hybrid` interaction policies, resting a mouse pointer on the wrapped content shows a bubble near it after `delay` (default 500ms). Under the `touch` policy it never shows, so tooltip content must stay non-essential. The bubble is portal-mounted at the window root, clamped to the window edges, takes no pointer events, and hides on leave and on press. A string/number `content` renders as themed body text; anything else as-is. `placement` picks the side (`"top"`, the default, or `"bottom"`).
- `Select` - A single-choice picker whose presentation forks on the interaction policy: `desktop`/`hybrid` opens an anchored dropdown under the trigger (flipping above when there is no room), `touch` opens a bottom sheet over a scrim. Same contract either way: `options` is an `Option[]` (`{ value, label }`), controlled via `value`/`onChange` or uncontrolled via `defaultValue`; pressing outside closes without a change. `placeholder` shows in the trigger while nothing is selected. The option list is not scrollable yet, so keep it short. The trigger's chevron is the `theme.icons.chevronDown` slot when a theme sets one.
- `SegmentedControl` - A single-choice row of equal-width segments joined flush: only the control's outermost corners are rounded, hairline dividers separate the segments, and the active segment fills with the theme `primary`. Hovered segments tint with the theme `overlayHover` under non-touch interaction policies. `options` is an `Option[]`; controlled via `value`/`onChange`, or uncontrolled via `defaultValue`. Override the inactive fill via `style.backgroundColor` and the outer radius via `style.borderRadius`.
- `ContextMenu` - Secondary actions on the wrapped content. The opening gesture follows the physical pointer: right-click for a mouse, long-press (500ms, cancelled by finger travel) for touch. The presentation forks on the interaction policy: `touch` gets a bottom sheet over a scrim, `desktop`/`hybrid` an anchored menu at the pointer that flips up near the bottom edge. `items` is a `ContextMenuItem[]` (`{ label, onSelect?, disabled? }`); pressing outside closes without selecting.
- `NavShell` - An app shell that arranges primary navigation around the content per `policy.navigation`: bottom tabs under it (`bottomTabs`), a narrow rail (`rail`), or a wide sidebar (`sidebar`) beside it. The content is a single stable node; switching arrangement only flips the shell's flex direction and remounts the stateless nav strip, so page state survives a resize across a breakpoint. `items` is a `NavItem[]` (`{ value, label, icon? }`; the icon renders above the label in tabs/rail, beside it in the sidebar); controlled via `value`/`onChange`, or uncontrolled via `defaultValue`. Safe areas are the caller's concern: wrap the shell in `SafeArea`.
- `SplitView` - A list-detail container driven by `policy.layout`: `twoPane` shows the `list` pane (width `listWidth`, default `theme.size.splitViewList`) beside the `detail` pane, `singlePane` shows one at a time per `showDetail`. Keep pane state (selection, scroll) in the app, not in the panes: crossing a breakpoint re-arranges and can remount them. It draws no chrome and adds no padding; a back affordance in the single-pane detail is the app's to render (fork on `policy.layout`).
- `QrCode` - Renders a QR code for `data` out of primitives: same-color modules in a row collapse into one box, drawn on a light quiet-zone panel; the grid recomputes only when `data` or `level` changes. It paints black on white by default (not the theme) so it stays scannable through a theme switch; override `color`/`background` only if the contrast still holds. `moduleSize` (default 6) is pixels per module, `margin` (default 16) the quiet zone (keep it non-zero), `level` the error correction (`L`/`M`/`Q`/`H`, default `M`: higher tolerates more damage but caps data length sooner), `radius` the panel's corner radius.
- `Icon` - A thin themed wrapper over the core `parseSvg` primitive. `src` is a whole SVG document as a string; the component parses it once (memoized), maps the draws to `<d-path>` in a square `designSize`-fitted box (`size`, default 24) and, for monochrome icons that stroke/fill with `currentColor`, recolors it via `color` (default the theme text color). It carries no icon set and no name registry, so any `currentColor` SVG works (Lucide, Feather, Heroicons, ...) and only the icons you import are bundled. Multi-color documents keep their own fills. For a non-square box, use `parseSvg` directly.
- `Theming` - Appearance (colors, spacing, border, font roles) comes from one shared, reactive theme backed by a Solid store: reads are tracked, so switching the theme at runtime recolors the live UI without remounting. Two presets ship, `darkTheme` and `lightTheme` (default dark); `setTheme(preset)` switches, `setTheme(partial)` merges an override one level deep per category. Custom themes are authored with `defineTheme`.
- `Policies` - Theme answers "how does it look"; policies answer "how does it behave". `policy` is a second reactive layer derived from the platform facts in `@solidrt/core` (`capabilities`, `env`), so components adapt to touch vs. desktop, window size, and display without every app wiring that logic itself. Reads are reactive like `theme`: a window resize or the first mouse move on a touch-capable device updates every consuming component live.
- `Density` - `<Density value="compact">` overrides the density policy for its subtree: every density-scaled metric below - `space()`, control sizes (Checkbox, Switch, Radio, Slider), `Item` and `Button` paddings - resolves this value instead of the global `policy.density`. Regions nest; the nearest wins. Use it to tighten a toolbar, a data table, or a sidebar without per-child props.
- `Typography helpers` - `typeStyle(variant)` resolves a theme type-scale role (`caption`/`label`/`body`/`title`/`heading`) to font props ready to spread onto a `<text>` or `d-text`: `fontSize` carries `policy.textScale`, and `fontWeight` carries the low-DPI weight compensation. Reactive when called inside a tracked scope, like any theme/policy read. `Text` applies it for you; reach for the helpers when building custom text out of core primitives.
- `Spacing` - `space(token)` is density-scaled spacing: a `theme.spacing` token (`sm`/`md`/`lg`/`xl`) multiplied by the density scale of the nearest `<Density>` region (falling back to the global policy) and rounded to whole pixels. Use it for gaps and paddings that should tighten under `compact`/`dense` density; read `theme.spacing` directly only for distances that must not move with density. Reactive when called inside a tracked scope.
- `Layout and style` - Most components group their props into two objects, split by one rule: `layout` properties feed the layout engine (flexbox/grid, sizing, padding, margin, position - the core `LayoutProps` set) and changing them triggers a relayout; `style` properties are paint-only and never affect layout: `color`, `backgroundColor`, `borderColor`, `borderWidth`, `borderRadius`, `opacity`, and the transform (`x`, `y`, `scale`, `rotate`, `rotateX`/`rotateY` with `perspective`, `originX`/`originY`, `clipRadius`). Event handlers (`onPointerDown`, `onKeyDown`, ...) are top-level props, never inside `layout` or `style`.
<!-- END GENERATED: exports -->

## Internals worth knowing

- `TextInput` and `RichTextEditor` share the internal `EditorField` shell
  (editor-field.tsx: focus, keys, text session, tap-to-position, caret,
  scroll, placeholder). It takes a buffer factory and a `renderLine`; lines
  and caret come from core's `createTextEditorLayout` (prepareText +
  layoutNextLine, one d-text per line). rich-text-document.ts is the rich
  value model; each rich line is one d-text with a span per style interval,
  and font-affecting attributes also feed the geometry via prepareText
  `runs`, so caret and wrap follow the drawn glyphs.
- Gestures: the arena and the pan/scroll recognizers (`createPan`,
  `createScroll`) live in `@solidrt/core`, so every package arbitrates in
  the one app-wide arena. The press recognizer (press.ts) stays in this
  package: per-pointer claims, innermost press wins, pan steals on slop.
  `Slider` resolves the arena outright on pointer down, so an enclosing
  scroller never takes its drag over.
- `Button` press feedback is a reactive style read (scale) plus an overlay
  d-rect tinted with `theme.color.overlayHover`, so no nodes are recreated
  and hover composes over any fill, including a caller-set
  `style.backgroundColor`.

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
