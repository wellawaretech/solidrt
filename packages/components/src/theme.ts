export type Theme = {
  fontSize: { body: number }
  color: {
    text: string
    textMuted: string
    surface: string
    border: string
  }
  spacing: { md: number }
  radius: { sm: number }
  borderWidth: { sm: number }
}

export let theme: Theme = {
  fontSize: { body: 14 },
  color: {
    text: "#333",
    textMuted: "rgba(0,0,0,0.4)",
    surface: "#ccc",
    border: "rgba(0,0,0,0.2)",
  },
  spacing: { md: 8 },
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