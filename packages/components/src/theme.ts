export type TextStyle = {
  size: number
  lineHeight: number
}

export type Theme = {
  text: { body: TextStyle }
  color: {
    text: string
    textMuted: string
    surface: string
    border: string
    primary: string
    onPrimary: string
    scrim: string
  }
  spacing: { sm: number; md: number }
  radius: { sm: number }
  borderWidth: { sm: number }
}

export let theme: Theme = {
  text: {
    body: { size: 14, lineHeight: 1.5 },
  },
  color: {
    text: "#333",
    textMuted: "rgba(0,0,0,0.4)",
    surface: "#ccc",
    border: "rgba(0,0,0,0.2)",
    primary: "#1f6feb",
    onPrimary: "#ffffff",
    scrim: "rgba(0,0,0,0.6)",
  },
  spacing: { sm: 4, md: 8 },
  radius: { sm: 4 },
  borderWidth: { sm: 1 },
}

type ThemePartial = { [K in keyof Theme]?: Partial<Theme[K]> }

export function setTheme(partial: ThemePartial) {
  for (let key in partial) {
    let k = key as keyof Theme
    Object.assign(theme[k], partial[k])
  }
}