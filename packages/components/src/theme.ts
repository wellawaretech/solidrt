import { createStore } from "@solidrt/core"

export type TextStyle = {
  size: number
  lineHeight: number
}

export type Theme = {
  text: { body: TextStyle }
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
    // Validation / destructive.
    danger: string
    // Overlay dim behind modals.
    scrim: string
  }
  spacing: { sm: number; md: number }
  radius: { sm: number }
  borderWidth: { sm: number }
}

// Scheme-independent tokens, shared by both presets.
const TEXT = { body: { size: 14, lineHeight: 1.5 } }
const SPACING = { sm: 4, md: 8 }
const RADIUS = { sm: 4 }
const BORDER_WIDTH = { sm: 1 }

export let darkTheme: Theme = {
  text: TEXT,
  color: {
    background: "#0b0f17",
    surface: "#161b22",
    surfaceAlt: "#21262d",
    text: "#e6edf3",
    textMuted: "rgba(230,237,243,0.5)",
    border: "rgba(255,255,255,0.14)",
    primary: "#1f6feb",
    onPrimary: "#ffffff",
    danger: "#f85149",
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
    text: "#1f2328",
    textMuted: "rgba(31,35,40,0.5)",
    border: "rgba(0,0,0,0.15)",
    primary: "#1f6feb",
    onPrimary: "#ffffff",
    danger: "#cf222e",
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
