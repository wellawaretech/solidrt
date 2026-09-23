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
// Screens are routes (routes.ts): the router owns the stack and the back
// step, so a link (`/app/<id>`, `/settings`) opens a screen directly and the
// control API reads where the player is. App-wide state the screens share is
// module state in parts/app-state and parts/dev-connection; this module
// owns the window and the theme it applies.
import { router } from "./routes"
import { render, env, createEffect } from "@solidrt/core"
import { Router } from "@solidrt/router"
import { Window, SafeArea, createFocusNav, theme, setTheme, darkTheme, lightTheme } from "@solidrt/components"
import { themeMode, fullscreen } from "./parts/app-state"

function App() {
  // Theme mode: "system" follows the OS preference (settable back to, unlike a
  // one-way toggle), "light"/"dark" pin it. Effective dark is dark until the OS
  // preference resolves.
  let dark = () => {
    let mode = themeMode()
    if (mode === "system") return env.systemTheme !== "light"
    return mode === "dark"
  }
  createEffect(
    () => dark(),
    (d) => setTheme(d ? darkTheme : lightTheme),
  )

  // Focus navigation (TV remote, keyboard, gamepad) over the focusable
  // controls, on the standard UI bindings. Its handlers on the window only
  // see keys nothing focused consumed; modal trapping comes with it (see
  // createFocusNav).
  let nav = createFocusNav()

  // Back at home is the router's to leave unprevented, so core's default
  // action runs and the client leaves. No confirmation: asking permission to
  // go back is not a platform idiom, and the stock behavior is to leave.
  return (
    <Window
      title="SolidRT"
      fullscreen={fullscreen()}
      layout={{ flexDirection: "column" }}
      style={{ backgroundColor: theme.color.background }}
      {...nav.handlers}
    >
      <SafeArea>
        <Router router={router} />
      </SafeArea>
    </Window>
  )
}

render(() => <App />)
