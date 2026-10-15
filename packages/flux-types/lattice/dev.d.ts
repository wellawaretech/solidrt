// Dev-server control surface (lattice). Present only in dev/go builds; in other
// builds `available` is false and the functions are no-ops.
declare module "sol:dev" {
  export const available: boolean
  export const canDiscover: boolean
  export const recents: string[]
  /**
   * The dev-server address the client was launched with (so the player can
   * auto-connect without on-device interaction), or null when launched without
   * one.
   */
  export const launchAddress: string | null
  export function connect(address: string): void
  export function discover(): void
  export function stop(): void
  /**
   * Register a named debug command, listable and callable from the dev server
   * (the list_debug / call_debug MCP tools). `args` arrives JSON-parsed; the
   * return value must be JSON-serializable and synchronous (an async command's
   * Promise is not awaited - the call errors). Re-registering a name replaces
   * it; registrations reset on hot reload, so a command over app-wide state
   * is registered at module init. Returns the unregister: a command that
   * reads one screen's state is registered when the screen mounts and
   * unregistered in its onCleanup, so a caller never reads a dead screen.
   * A stale unregister (the name registered again since) removes nothing.
   * Callable in every build, but only dev clients ever invoke commands.
   */
  export function registerDebug(name: string, fn: (args?: any) => unknown): () => void
  /**
   * Register a named counter the dev server's stats query reads (the
   * `app` block of `/stats` and the get_stats MCP tool): Godot's
   * `Performance.add_custom_monitor`, Unity's ProfilerCounter. `read` is
   * called synchronously at query time and returns the current number; a
   * throw or a non-number reads as null under the name, so a broken reader
   * stays visible. The same registry contract as registerDebug: a
   * re-registered name replaces, registrations reset on hot reload, the
   * returned unregister removes the name while this registration still
   * holds it. The core's resource pools register theirs (poolHits,
   * poolMisses, poolHeld); an app registers what it streams or simulates
   * (entities alive, regions loading). Callable in every build, but only
   * dev clients ever read it.
   */
  export function registerStat(name: string, read: () => number): () => void
  /**
   * Report where the app is, for tooling: a string naming the current
   * location (a router's path; an app routing by hand may report its own),
   * called again on every change. The runtime stores the value and never
   * interprets it. The dev server reads it (`GET /__control__/link`, the
   * get_location MCP tool), and a hot reload of the same app starts with the
   * last reported location as its `env.launchLink`, so a rebuild comes back
   * to the screen it left. null withdraws it. Callable in every build, but
   * only dev clients ever read it.
   */
  export function reportLocation(location: string | null): void
}
