// A Switch toggling between the two built-in themes. Every component reads
// theme.* reactively, so flipping setTheme() recolors the whole tree with no
// remount - the Switch itself included.
import { render, createSignal, createEffect, env } from "@solidrt/core"
import { Window, View, Text, Switch, Card, theme, setTheme, darkTheme, lightTheme } from "@solidrt/components"

function App() {
  // Start from the OS light/dark preference; env.systemTheme is
  // "dark" | "light" | "unknown" (unknown falls back to dark, the default).
  let [dark, setDark] = createSignal(() => env.systemTheme !== "light")

  createEffect(
    () => dark(),
    (on) => setTheme(on ? darkTheme : lightTheme),
  )

  return (
    <Window
      title="Theme toggle"
      layout={{ flexDirection: "column", alignItems: "center", justifyContent: "center" }}
      style={{ backgroundColor: theme.color.background }}
    >
      <Card title={dark() ? "Dark theme" : "Light theme"} layout={{ width: 280 }}>
        <View
          layout={{ flexDirection: "row", alignItems: "center", justifyContent: "space-between" }}
        >
          <Text layout={{ fontSize: 14 }} style={{ color: theme.color.text }}>
            Dark mode
          </Text>
          <Switch value={dark()} onChange={setDark} />
        </View>
      </Card>
    </Window>
  )
}

render(() => <App />)
