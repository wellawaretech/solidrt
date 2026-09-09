// The go client's player: the compiled-in home screen. Lists the apps
// installed in the version store (tap a row for its details and remove, or the
// row's play button to launch straight away) and manages the dev-server
// connection (address entry, discovery, QR scan on a full-screen camera view,
// recents - all gathered in the connect panel). Built from
// @solidrt/components; follows the OS dark/light preference and the layout
// policy: wide windows show a WhatsApp-style split (list left, selected app's
// details right), narrow ones navigate between two screens. Bundled by
// `make player-bundle` and embedded via include_str! (see lattice/src/lib.rs
// PLAYER_SOURCE).
//
// This module owns the theme, the screen routing, the back stack (every level of
// it, leaving only the last press to core), and the app selection and
// status notice (lifted so they survive a scan). Routing is two screens deep
// only: the home screen and the full-bleed camera scan. Settings and connect are
// panels of the home screen, each replacing one of its panes, and HomeScreen
// renders them (see HomePanel in parts/types). The dev-server connection is
// app-wide module state in parts/dev-connection; the screens and panels
// themselves live in parts/.
import { render, env, createSignal, createEffect, onBack } from "@solidrt/core"
import { Switch, Match } from "solid-js"
import {
  Window,
  SafeArea,
  createFocusNav,
  theme,
  setTheme,
  darkTheme,
  lightTheme,
} from "@solidrt/components"
import { HomeScreen } from "./parts/home-screen"
import { ScanScreen } from "./parts/scan-screen"
import { connect } from "./parts/dev-connection"
import { type HomePanel, type Screen, type ThemeMode } from "./parts/types"

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

  // Fullscreen is window state, so it lives here beside the Window it drives;
  // the settings panel only renders the toggle.
  let [fullscreen, setFullscreen] = createSignal(false)

  let [screen, setScreen] = createSignal<Screen>("home")
  // Settings and connect are panels of the home screen rather than screens of
  // their own: HomeScreen stays mounted and swaps one of its two panes, so the
  // other keeps its content (see HomePanel).
  let panel = (): HomePanel | null => {
    let s = screen()
    return s === "settings" || s === "connect" ? s : null
  }
  // Selection and the status-line notice are lifted here (rather than owned by
  // HomeScreen) so they survive the Switch unmounting the home screen during a
  // scan - and the notice is cross-screen (a camera scan error on the scan
  // screen surfaces in the home status line).
  let [selectedId, setSelectedId] = createSignal<string | null>(null)
  let [notice, setNotice] = createSignal<string | null>(null)
  let dial = (addr: string) => {
    setNotice(null)
    setScreen("home")
    connect(addr)
  }

  // The root of the back stack, so this handler registers first and runs last:
  // everything mounted above it (the home screen's detail selection, a dialog
  // inside it) gets the event first and takes it if it is theirs. What is left
  // over is App's own: close a panel or leave the scan, landing on home.
  //
  // Back at home is not ours and is left unprevented, so core's default action
  // runs and the client leaves. No confirmation: asking permission to go back is
  // not a platform idiom, and the stock behavior is to leave.
  onBack((e) => {
    if (screen() !== "home") {
      e.preventDefault()
      setScreen("home")
    }
  })

  // Focus navigation (TV remote, keyboard, gamepad) over the focusable
  // controls, on the standard UI bindings. Its handlers on the window only
  // see keys nothing focused consumed; modal trapping comes with it (see
  // createFocusNav).
  let nav = createFocusNav()

  return (
    <Window
      title="SolidRT"
      fullscreen={fullscreen()}
      layout={{ flexDirection: "column" }}
      style={{ backgroundColor: theme.color.background }}
      {...nav.handlers}
    >
      <SafeArea>
        <Switch>
          <Match when={screen() === "scan"}>
            <ScanScreen
              onScanned={(data) => dial(data)}
              // Cancelling lands on the connect panel whether the scan was
              // started there or from the home header: it holds the other ways
              // to connect, and back from it goes home. A camera failure goes
              // home instead: its notice shows in the dev card's status line,
              // which the connect panel covers.
              onCancel={() => setScreen("connect")}
              onError={(m) => {
                setNotice(`Camera: ${m}`)
                setScreen("home")
              }}
            />
          </Match>

          <Match when={screen() === "home" || panel() != null}>
            <HomeScreen
              selectedId={selectedId()}
              setSelectedId={setSelectedId}
              notice={notice()}
              setNotice={setNotice}
              panel={panel()}
              themeMode={themeMode()}
              onThemeMode={setThemeMode}
              fullscreen={fullscreen()}
              onFullscreen={setFullscreen}
              onScan={() => {
                setNotice(null)
                setScreen("scan")
              }}
              onConnect={() => setScreen("connect")}
              onSettings={() => setScreen("settings")}
              onPanelClose={() => setScreen("home")}
              onDial={(addr) => dial(addr)}
            />
          </Match>
        </Switch>
      </SafeArea>
    </Window>
  )
}

render(() => <App />)
