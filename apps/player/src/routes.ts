// The player's route tree and router. Home is a layout that consumes no path
// of its own: settings, connect and an app's detail are its children, each
// taking one pane of the home split view (see HomeScreen), so they stay one
// screen deep. Scan is the only route that takes the whole window.
//
// The parts import route values from here to navigate and this module
// imports their components, a cycle ESM resolves as long as a part reads a
// route only inside a handler, never at module scope.
import { createRootRoute, createRoute, createRouter } from "@solidrt/router"
import { HomeScreen, AppDetailRoute } from "./parts/home-screen"
import { SettingsPanel } from "./parts/settings-panel"
import { ConnectPanel } from "./parts/connect-panel"
import { ScanScreen } from "./parts/scan-screen"

// The shape of an installed app's id (srt:apps validates strictly on its
// side); existence is the detail screen's to check, the store can change
// under a link.
const APP_ID = /^[A-Za-z0-9][A-Za-z0-9._-]*$/

export let settings = createRoute({ path: "/settings", component: SettingsPanel })
export let connect = createRoute({ path: "/connect", component: ConnectPanel })
export let app = createRoute({
  path: "/app/$id",
  params: {
    parse: (raw) => {
      let id = raw.id ?? ""
      if (!APP_ID.test(id)) throw new Error("not an app id")
      return { id }
    },
  },
  component: AppDetailRoute,
})
export let home = createRoute({ path: "/", component: HomeScreen, children: [settings, connect, app] })
export let scan = createRoute({ path: "/scan", component: ScanScreen })
export let root = createRootRoute({ children: [home, scan] })

export let router = createRouter({ tree: root })
