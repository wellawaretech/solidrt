// The go client's launcher: the compiled-in home screen. Lists the apps
// installed in the version store (tap to launch, info button for a detail
// view with remove) and manages the dev-server connection (discover, QR scan
// on a full-screen camera view, manual address entry, recents). Built from
// @solidrt/components; follows the OS dark/light preference and the layout
// policy: wide windows show a WhatsApp-style split (list left, selected app's
// details right), narrow ones navigate between two screens. Bundled by
// `make launcher-bundle` and embedded via include_str! (see lattice/src/lib.rs
// LAUNCHER_SOURCE).
//
// This module owns the theme, the screen routing, and the app selection and
// status notice (lifted so they survive sub-screen visits). The dev-server
// connection is app-wide module state in parts/dev-connection; the screens
// themselves live in parts/.
import { render, env, createSignal, createEffect, onBack } from "@solidrt/core"
import { Switch, Match } from "solid-js"
import {
  Window,
  SafeArea,
  theme,
  setTheme,
  darkTheme,
  lightTheme,
} from "@solidrt/components"
import { HomeScreen } from "./parts/home-screen"
import { SettingsScreen } from "./parts/settings-screen"
import { ScanScreen } from "./parts/scan-screen"
import { ConnectScreen } from "./parts/connect-screen"
import { connect } from "./parts/dev-connection"
import { type Screen, type ThemeMode } from "./parts/types"

function App() {
  // Theme mode: "system" follows the OS preference (settable back to, unlike a
  // one-way toggle), "light"/"dark" pin it. Effective dark is dark until the OS
  // preference resolves.
  let [themeMode, setThemeMode] = createSignal<ThemeMode>("system")
  let dark = () => {
    let mode = themeMode()
    if (mode === "system") return env.systemTheme !== "light"
    return mode === "dark"
  }
  createEffect(
    () => dark(),
    (d) => setTheme(d ? darkTheme : lightTheme),
  )

  let [screen, setScreen] = createSignal<Screen>("home")
  // Selection and the status-line notice are lifted here (rather than owned by
  // HomeScreen) so they survive the Switch unmounting the home screen during a
  // Settings/Scan/Connect visit - and the notice is cross-screen (a camera
  // scan error on the scan screen surfaces in the home status line).
  let [selectedId, setSelectedId] = createSignal<string | null>(null)
  let [notice, setNotice] = createSignal<string | null>(null)

  let dial = (addr: string) => {
    setNotice(null)
    setScreen("home")
    connect(addr)
  }

  // Back pops sub-screens toward home before it exits; the home screen's own
  // handler clears a narrow-layout detail selection. At home with nothing to
  // pop, the default action runs - exit() at the launcher root quits the client
  // (backgrounds it on Android, the stock back-at-root feel).
  onBack((e) => {
    if (screen() !== "home") {
      e.preventDefault()
      setScreen("home")
    }
  })

  return (
    <Window
      title="SolidRT"
      layout={{ flexDirection: "column" }}
      style={{ backgroundColor: theme.color.background }}
    >
      <SafeArea>
        <Switch>
          <Match when={screen() === "scan"}>
            <ScanScreen
              onScanned={(data) => dial(data)}
              onCancel={() => setScreen("home")}
              onError={(m) => {
                setNotice(`Camera: ${m}`)
                setScreen("home")
              }}
            />
          </Match>

          <Match when={screen() === "manual"}>
            <ConnectScreen onDial={(addr) => dial(addr)} onCancel={() => setScreen("home")} />
          </Match>

          <Match when={screen() === "settings"}>
            <SettingsScreen
              mode={themeMode()}
              onMode={setThemeMode}
              onBack={() => setScreen("home")}
            />
          </Match>

          <Match when={screen() === "home"}>
            <HomeScreen
              selectedId={selectedId()}
              setSelectedId={setSelectedId}
              notice={notice()}
              setNotice={setNotice}
              onScan={() => {
                setNotice(null)
                setScreen("scan")
              }}
              onManual={() => setScreen("manual")}
              onSettings={() => setScreen("settings")}
            />
          </Match>
        </Switch>
      </SafeArea>
    </Window>
  )
}

render(() => <App />)
