import { createStore, mixColors } from "@solidrt/core"

export type TextStyle = {
  size: number
  lineHeight: number
  weight: 100 | 200 | 300 | 400 | 500 | 600 | 700 | 800 | 900
}

// The type scale's role names. <Text variant> and theme.text are keyed by these.
export type TextVariant = "caption" | "label" | "body" | "title" | "heading"

export type Theme = {
  text: {
    // Passed through to the core font stack: "sans" | "mono" | a family name.
    fontFamily: string
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
    // Hover tint for surface-colored controls (non-touch interaction policies).
    surfaceHover: string
    text: string
    textMuted: string
    border: string
    primary: string
    // Hover tint for primary-colored controls.
    primaryHover: string
    onPrimary: string
    // Validation / destructive.
    danger: string
    // Hover tint for danger-colored controls.
    dangerHover: string
    // Overlay dim behind modals.
    scrim: string
  }
  spacing: { sm: number; md: number; lg: number; xl: number }
  radius: { sm: number; md: number; lg: number }
  borderWidth: { sm: number }
}

// Scheme-independent tokens, shared by both presets. The type scale: body is
// the base text style; caption and label sit under it (secondary and
// emphasized small text), title and heading above it (card and page headings).
const TEXT: Theme["text"] = {
  fontFamily: "sans",
  caption: { size: 11, lineHeight: 1.3, weight: 400 },
  label: { size: 12, lineHeight: 1.3, weight: 600 },
  body: { size: 14, lineHeight: 1.5, weight: 400 },
  title: { size: 18, lineHeight: 1.4, weight: 700 },
  heading: { size: 22, lineHeight: 1.3, weight: 700 },
}
const SPACING = { sm: 4, md: 8, lg: 16, xl: 20 }
const RADIUS = { sm: 4, md: 8, lg: 12 }
const BORDER_WIDTH = { sm: 1 }

export let darkTheme: Theme = {
  text: TEXT,
  color: {
    background: "#0b0f17",
    surface: "#161b22",
    surfaceAlt: "#21262d",
    surfaceHover: "#262c34",
    text: "#e6edf3",
    // Muted is an opaque tone between text and background, mixed in LAB (like
    // Material 3's tonal colors, not an alpha overlay): alpha text renders
    // thin on low-DPI and its contrast depends on what sits behind it.
    textMuted: mixColors("#e6edf3", "#0b0f17", 0.4),
    border: "rgba(255,255,255,0.14)",
    primary: "#1f6feb",
    primaryHover: "#388bfd",
    onPrimary: "#ffffff",
    danger: "#f85149",
    dangerHover: "#ff7b72",
    scrim: "rgba(0,0,0,0.6)",
  },
  spacing: SPACING,
  radius: RADIUS,
  borderWidth: BORDER_WIDTH,
}

export let lightTheme: Theme = {
  text: TEXT,
  color: {
    background: "#ffffff",
    surface: "#f6f8fa",
    surfaceAlt: "#eaeef2",
    surfaceHover: "#e0e5eb",
    text: "#1f2328",
    textMuted: mixColors("#1f2328", "#ffffff", 0.4),
    border: "rgba(0,0,0,0.15)",
    primary: "#1f6feb",
    primaryHover: "#1a5fd0",
    onPrimary: "#ffffff",
    danger: "#cf222e",
    dangerHover: "#a40e26",
    scrim: "rgba(0,0,0,0.4)",
  },
  spacing: SPACING,
  radius: RADIUS,
  borderWidth: BORDER_WIDTH,
}

// Backed by a Solid store so reads are tracked: calling setTheme at runtime
// recolors the live UI without remounting. Components read theme.* through
// thunks/JSX expressions, so they pick this up with no call-site changes.
let [theme, setThemeStore] = createStore<Theme>({ ...darkTheme })
export { theme }

type ThemePartial = { [K in keyof Theme]?: Partial<Theme[K]> }

// Switch themes with a full preset (setTheme(lightTheme)) or apply a targeted
// override (setTheme({ color: { primary: "#f00" } })). Merges one level deep per
// category, matching the previous Object.assign behavior.
export function setTheme(partial: ThemePartial) {
  setThemeStore((s) => {
    for (let key in partial) {
      let k = key as keyof Theme
      Object.assign(s[k], (partial as any)[k])
    }
  })
}
