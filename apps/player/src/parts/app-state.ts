// Player state that more than one screen reads or writes: the theme mode and
// fullscreen (settings writes them, App applies them), the status notice
// (a scan error surfaces in the home status line), the installed-app list
// (the home list and an app's detail), and dialing, which lands home. App-wide
// singleton state, so per Solid's guidance it lives here as module-scope
// signals rather than props (see dev-connection.ts for the same reasoning);
// route components take no props, so this is how the panels reach it.
import { createSignal } from "@solidrt/core"
import { available as appsAvailable, list, type InstalledApp } from "srt:apps"
import { connect } from "./dev-connection"
import { home, router } from "../routes"
import type { ThemeMode } from "./types"

export let [themeMode, setThemeMode] = createSignal<ThemeMode>("system")
export let [fullscreen, setFullscreen] = createSignal(false)
export let [notice, setNotice] = createSignal<string | null>(null)

let [apps, setApps] = createSignal<InstalledApp[]>(appsAvailable ? list() : [])
export let installedApps = apps
// Re-read the store after a change it does not report (a removal).
export function refreshApps() {
  setApps(appsAvailable ? list() : [])
}

// Dial an address (host:port or p2p ticket) from wherever it was entered or
// scanned: the flow is finished, so the stack starts over at home, where the
// dev card's status line reports on the attempt.
export function dial(addr: string) {
  setNotice(null)
  void router.navigate(home, { reset: true })
  connect(addr)
}
