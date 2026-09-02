import { createStore } from "@solidjs/signals"
import type { FontWeight } from "@solidrt/core"
import type { StyleProps } from "./types"

export type TextStyle = {
  size: number
  lineHeight: number
  weight: FontWeight
}

// The type scale's role names. <Text variant> and theme.text are keyed by these.
export type TextVariant = "caption" | "label" | "body" | "title" | "heading"

// The components whose chrome accepts a theme-level paint override (see
// Theme["components"]). Plain containers (View, Pressable, ...) are not
// themed, so they take no override either.
export type ThemedComponent =
  | "button"
  | "card"
  | "badge"
  | "switch"
  | "checkbox"
  | "radio"
  | "item"
  | "select"
  | "segmentedControl"
  | "textInput"
  | "richTextEditor"
  | "tooltip"
  | "divider"
  | "progressBar"
  | "spinner"

// A resolved theme: every value is a plain string/number ready to be read by
// a component. Authoring happens through defineTheme, which is where
// [light, dark] pairs and the type-scale expansion live; a Theme itself has
// no notion of modes.
export type Theme = {
  text: {
    // Passed through to the core font stack: "sans" | "mono" | a family name.
    fontFamily: string
    // The monospace family for code (RichTextEditor inline code).
    monoFamily: string
    caption: TextStyle
    label: TextStyle
    body: TextStyle
    title: TextStyle
    heading: TextStyle
  }
  color: {
    // Window fill.
    background: string
    // Control/card fill.
    surface: string
    // Subtle raised/track fill (switch off-state, slider track, ...).
    surfaceAlt: string
    text: string
    textMuted: string
    border: string
    primary: string
    onPrimary: string
    // Lower-emphasis accent: the puzzle mark's darker blue.
    secondary: string
    onSecondary: string
    // Validation / destructive.
    danger: string
    // Overlay dim behind modals.
    scrim: string
    // Hover/pressed feedback tints: translucent colors DRAWN OVER a control's
    // own fill (one token pair for every control, instead of a hover variant
    // per fill color), so feedback works over any background, including a
    // caller-set style.backgroundColor. Non-touch interaction policies only.
    overlayHover: string
    overlayPressed: string
    // The focus ring (spatial nav under the focusRing policy), drawn at
    // borderWidth.focus by every focusable control. Defaults to text so it
    // stays visible on primary-filled controls.
    ring: string
    // The text-selection highlight, drawn by the editable fields behind the
    // selected glyphs; translucent so the text stays readable. Defaults to
    // overlayPressed (a neutral scheme-aware tint that needs no color math).
    selection: string
  }
  // Gaps and paddings, multiples of one base unit (sm 1x, md 2x, lg 4x,
  // xl 5x); read through space() where density should apply.
  spacing: { sm: number; md: number; lg: number; xl: number }
  // Corner radii. md is THE control radius (Button, TextInput, Select,
  // SegmentedControl); sm is one step under it (Checkbox, Item, menus,
  // Tooltip), lg one step over (Card), full is the pill.
  radius: { sm: number; md: number; lg: number; full: number }
  borderWidth: { sm: number; focus: number }
  // Motion durations (ms). Every built-in transition the components declare
  // draws its timing here, so one theme edit retimes the whole package:
  // fast is press/hover feedback, base the color/opacity fades (state
  // changes, the theme cross-fade, popup enter/exit), slow the travel of a
  // control's moving parts (switch knob, segmented indicator, progress
  // fill). See motion.tsx; policy.motion gates them.
  motion: { fast: number; base: number; slow: number }
  // Default extents of the components that have one: the panes and rails an
  // app lays its screens around, and the smallest sensible popup/track. Each
  // is a per-instance layout override away (listWidth, layout.width, ...);
  // the theme sets the app-wide default.
  size: { navRail: number; navSidebar: number; splitViewList: number; menuMinWidth: number; slider: number }
  // Semantic control glyphs, as SVG document strings (the Icon currency).
  // Components draw their built-in vector paths by default; a theme that sets
  // a slot swaps that glyph everywhere it appears. The package still bundles
  // no icon set.
  icons: { chevronDown?: string; check?: string }
  // Per-component paint overrides: merged between a component's themed
  // defaults and the instance's style prop, so a theme can restyle every
  // Button (say, pill corners) without wrapping the component. Instance
  // style still wins.
  components: { [K in ThemedComponent]?: StyleProps }
}

// -- Authoring ---------------------------------------------------------------

// A color in a theme definition: one value, or a [light, dark] pair resolved
// by defineTheme's scheme argument. Pairs are opt-in per token; a definition
// without any needs no scheme at all (a game ships one look, not two).
export type ThemeColor = string | [light: string, dark: string]

export type ThemeDefinition = {
  color: { [K in Exclude<keyof Theme["color"], "ring" | "selection">]: ThemeColor } & {
    ring?: ThemeColor
    selection?: ThemeColor
  }
  text?: {
    fontFamily?: string
    monoFamily?: string
    // The body font size; the other roles derive from it. Default 14.
    base?: number
    // The step between adjacent roles (caption, label, body, title, heading
    // sit at base * ratio^(-2..2), rounded to whole px). Default 1.26.
    ratio?: number
    // Per-role overrides of the derived size and the default line heights
    // and weights.
    roles?: { [K in TextVariant]?: Partial<TextStyle> }
  }
  // One base unit (the steps derive from it, see deriveSpacing) or explicit
  // steps. Default 4.
  spacing?: number | Partial<Theme["spacing"]>
  // One base (the control radius; the steps derive from it, see
  // deriveRadius) or explicit steps. Default 8.
  radius?: number | Partial<Theme["radius"]>
  borderWidth?: Partial<Theme["borderWidth"]>
  motion?: Partial<Theme["motion"]>
  size?: Partial<Theme["size"]>
  icons?: Theme["icons"]
  components?: Theme["components"]
}

const SPACING_BASE = 4

// The spacing scale from its base unit.
function deriveSpacing(base: number): Theme["spacing"] {
  return { sm: base, md: base * 2, lg: base * 4, xl: base * 5 }
}
const RADIUS_BASE = 8
const RADIUS_FULL = 9999

// The radius scale from its base: sm half, lg one and a half, full the pill.
function deriveRadius(base: number): Theme["radius"] {
  return { sm: Math.round(base / 2), md: base, lg: Math.round(base * 1.5), full: RADIUS_FULL }
}
const BORDER_WIDTH = { sm: 1, focus: 2 }
// The motion timing scale (ms): fast enough that feedback reads as
// immediate, base tuned so a state fade registers without dragging, slow
// long enough that a part's travel reads as movement rather than a flicker.
const MOTION = { fast: 100, base: 150, slow: 250 }
const SIZE = { navRail: 72, navSidebar: 220, splitViewList: 320, menuMinWidth: 120, slider: 200 }

// Line height and weight per role; body is the base text, label is body at
// an emphasized weight (form labels, key/value keys, tags), caption the one
// small role (badges, tab labels, timestamps), title and heading sit above
// body (card and page headings). De-emphasis is a color (textMuted), not a
// size: caption is small text to be glanced at, so keep it in the full text
// color rather than stacking small and muted.
const ROLE_DEFAULTS: { [K in TextVariant]: { step: number; lineHeight: number; weight: TextStyle["weight"] } } = {
  caption: { step: -1, lineHeight: 1.3, weight: 400 },
  label: { step: 0, lineHeight: 1.5, weight: 600 },
  body: { step: 0, lineHeight: 1.5, weight: 400 },
  title: { step: 1, lineHeight: 1.4, weight: 700 },
  heading: { step: 2, lineHeight: 1.3, weight: 700 },
}

/**
 * Resolves a theme definition into a Theme. `scheme` picks the side of every
 * [light, dark] color pair; a definition without pairs needs no scheme
 * (modes are a per-theme choice, not a framework requirement). The type
 * scale expands from text.base and text.ratio, with text.roles overriding
 * per role. Throws on a pair without a scheme (throw-in-dev policy).
 */
export function defineTheme(def: ThemeDefinition, scheme?: "light" | "dark"): Theme {
  let color = {} as Theme["color"]
  for (let key in def.color) {
    let k = key as keyof Theme["color"]
    let value = def.color[k]
    if (value == null) continue
    if (Array.isArray(value)) {
      if (!scheme) throw new Error(`Theme color "${key}" is a [light, dark] pair; pass a scheme to defineTheme`)
      color[k] = value[scheme === "light" ? 0 : 1]
    } else color[k] = value
  }
  if (def.color.ring == null) color.ring = color.text
  if (def.color.selection == null) color.selection = color.overlayPressed
  let base = def.text?.base ?? 14
  let ratio = def.text?.ratio ?? 1.26
  let role = (name: TextVariant): TextStyle => {
    let d = ROLE_DEFAULTS[name]
    return {
      size: Math.round(base * ratio ** d.step),
      lineHeight: d.lineHeight,
      weight: d.weight,
      ...def.text?.roles?.[name],
    }
  }
  return {
    text: {
      fontFamily: def.text?.fontFamily ?? "sans",
      monoFamily: def.text?.monoFamily ?? "mono",
      caption: role("caption"),
      label: role("label"),
      body: role("body"),
      title: role("title"),
      heading: role("heading"),
    },
    color,
    spacing:
      typeof def.spacing === "number"
        ? deriveSpacing(def.spacing)
        : { ...deriveSpacing(SPACING_BASE), ...def.spacing },
    radius:
      typeof def.radius === "number" ? deriveRadius(def.radius) : { ...deriveRadius(RADIUS_BASE), ...def.radius },
    borderWidth: { ...BORDER_WIDTH, ...def.borderWidth },
    motion: { ...MOTION, ...def.motion },
    size: { ...SIZE, ...def.size },
    icons: def.icons ?? {},
    components: def.components ?? {},
  }
}

// -- The built-in presets: one definition, resolved twice ---------------------

const DEFAULT: ThemeDefinition = {
  color: {
    background: ["#ffffff", "#0b0f17"],
    surface: ["#f6f8fa", "#161b22"],
    surfaceAlt: ["#eaeef2", "#21262d"],
    // Dark body text sits well under white (about 10:1 on the background,
    // tuned by eye on a low-DPI display): full brightness glares on a dark
    // ground, and the low-DPI weight compensation thickens it further.
    text: ["#1f2328", "#b1bac4"],
    // Muted is an opaque tone between text and background, mixed in oklab
    // (like Material 3's tonal colors, not an alpha overlay): alpha text
    // renders thin on low-DPI and its contrast depends on what sits behind
    // it. Precomputed - core's mixColors delegates to flux:rendertree, and a
    // preset is data that must not need the render engine at import time
    // (the website token build imports this module headless). If text or
    // background changes, recompute: mixColors(text, background, 0.4).
    // The dark tone sits a step above the 4.5:1 AA floor (about 5.4:1)
    // instead of at the mix: the body text already sits low, and the
    // strict mix falls under it.
    textMuted: ["#707376", "#828993"],
    border: ["rgba(0,0,0,0.15)", "rgba(255,255,255,0.14)"],
    // Accent tuned to the puzzle mark's mid blue.
    primary: "#547ebf",
    onPrimary: "#ffffff",
    // The darker shade of the same puzzle segment.
    secondary: "#2b5696",
    onSecondary: "#ffffff",
    danger: ["#cf222e", "#f85149"],
    scrim: ["rgba(0,0,0,0.4)", "rgba(0,0,0,0.6)"],
    // Feedback darkens on a light scheme and lightens on a dark one.
    overlayHover: ["rgba(0,0,0,0.08)", "rgba(255,255,255,0.08)"],
    overlayPressed: ["rgba(0,0,0,0.14)", "rgba(255,255,255,0.14)"],
    // Primary (#547ebf) at a translucent strength: pale enough to read
    // through on light, a touch stronger against the dark ground.
    selection: ["rgba(84,126,191,0.30)", "rgba(84,126,191,0.40)"],
  },
  // base 14 and ratio 1.26 derive title 18 and heading 22; caption sits
  // one step under body but the ratio lands on 11, too small to read on a
  // low-DPI display or at TV distance, so it is pinned.
  text: { roles: { caption: { size: 12 } } },
}

export let darkTheme: Theme = defineTheme(DEFAULT, "dark")
export let lightTheme: Theme = defineTheme(DEFAULT, "light")

let [themeStore, setThemeStore] = createStore<Theme>({ ...darkTheme })

// The shared theme, backed by a Solid store so reads are tracked: calling
// setTheme at runtime recolors the live UI without remounting. Components
// read theme.* through thunks/JSX expressions, so they pick this up with no
// call-site changes.
export let theme: Theme = themeStore

type ThemePartial = { [K in keyof Theme]?: Partial<Theme[K]> }

// Switch themes with a full preset (setTheme(lightTheme)), a resolved
// definition (setTheme(defineTheme({...}))), or apply a targeted override
// (setTheme({ color: { primary: "#f00" } })). Merges one level deep per
// category (for components, that level is the component name).
export function setTheme(partial: ThemePartial) {
  setThemeStore((s) => {
    for (let key in partial) {
      let k = key as keyof Theme
      Object.assign(s[k], (partial as any)[k])
    }
  })
}
