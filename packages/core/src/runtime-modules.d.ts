// Lattice runner builtin modules (the "srt:*" surface). These are ambient
// module declarations, so they live in this global-script .d.ts (no top-level
// import/export) rather than in types.d.ts: an ambient `declare module` only
// becomes globally visible to consumers from a non-module declaration file.

declare module "*.svg" {
  const content: string
  export default content
}

// Binary asset imports: `import data from "./pic.png" with { type: "binary" }`.
// The bundler inlines the file's bytes as a Uint8Array (see packages/cli
// bundler `binaryImport`); feed it straight into createImage/decodeImage.
declare module "*.png" {
  const bytes: Uint8Array
  export default bytes
}
declare module "*.jpg" {
  const bytes: Uint8Array
  export default bytes
}
declare module "*.jpeg" {
  const bytes: Uint8Array
  export default bytes
}

// UI event bus (lattice), provided by the runtime as a builtin module.
// on/once return an unsubscribe function.
declare module "srt:events" {
  export function on(event: string, callback: (data: any) => void): () => void
  export function once(event: string, callback: (data: any) => void): () => void
}

// Dev-server control surface (lattice). Present only in dev/go builds; in other
// builds `available` is false and the functions are no-ops.
declare module "srt:dev" {
  export const available: boolean
  export const canDiscover: boolean
  export const recents: string[]
  export function connect(address: string): void
  export function discover(): void
  export function stop(): void
  /**
   * Register a named debug command, listable and callable from the dev server
   * (the list_debug / call_debug MCP tools). `args` arrives JSON-parsed; the
   * return value must be JSON-serializable and synchronous (promises are not
   * awaited). Re-registering a name replaces it; registrations reset on hot
   * reload, so register at module init. Callable in every build, but only dev
   * clients ever invoke commands.
   */
  export function registerDebug(name: string, fn: (args?: any) => unknown): void
}

// Installed-app management (lattice), the launcher's surface over the client's
// version store. Present only in go/dev client builds; elsewhere `available`
// is false, `list` returns [], and launch/remove are no-ops.
declare module "srt:apps" {
  export const available: boolean
  /**
   * An installed app: id, display name (the installed manifest's displayName,
   * defaulting to the id) and current version id (manifest hash).
   */
  export type InstalledApp = { id: string; name: string; version: string }
  /** Installed apps, sorted by name. */
  export function list(): InstalledApp[]
  /**
   * A stored version: id (manifest hash), bytes on disk, whether it is the
   * current one, and the SolidRT (CLI) release that built it per its
   * manifest ("unknown" from an in-repo CLI or when the manifest predates
   * the field).
   */
  export type AppVersion = { id: string; size: number; current: boolean; solidrtVersion: string }
  /** One file in a listing: a relative path and its size in bytes. */
  export type AppFile = { path: string; size: number }
  /**
   * One fetch-cache entry: the cached (resolved) url, the response content
   * type (lowercased, parameters stripped; absent when the response had
   * none) and the entry's size on disk.
   */
  export type AppCacheEntry = { url: string; type?: string; size: number }
  /**
   * Usage details for one installed app: total bytes of its stored versions
   * (assets shared between versions via hardlinks count in each), of its
   * data sandbox and of its fetch cache, plus the stored versions (current
   * first, then newest first) and three listings: `files` and `data` are
   * disk walks of the current version dir and the data sandbox (sorted by
   * path), `cache` is the fetch cache's entries (sorted by url).
   */
  export type AppInfo = {
    id: string
    name: string
    version: string
    installSize: number
    dataSize: number
    cacheSize: number
    versions: AppVersion[]
    files: AppFile[]
    data: AppFile[]
    cache: AppCacheEntry[]
  }
  /** Usage details for an installed app. Throws when the app is not installed. */
  export function info(id: string): AppInfo
  /**
   * Boot the app's installed current version, replacing the running app (the
   * launcher). Throws when the app is not installed. Custom fonts of the
   * launched app register at client startup only, not mid-session.
   */
  export function launch(id: string): void
  /**
   * Full uninstall: the app's versions, state and data sandbox. Throws when
   * the app is not installed.
   */
  export function remove(id: string): void
  /**
   * Delete the app's fetch cache. Clearing a missing or empty cache is a
   * no-op; the id does not need to be installed, so a removed app's
   * leftover cache is still clearable.
   */
  export function clearCache(id: string): void
  /**
   * Build identity of this runtime, for the launcher's settings screen. Not
   * app-specific, but surfaced here since the launcher already imports this
   * module. `version` is the release version (git describe; "0.0.0-dev" in a
   * plain build), `profile` is "debug" or "release", `platform` is the OS
   * (std::env::consts::OS, e.g. "linux", "android", "windows", "macos").
   */
  export const version: string
  export const profile: string
  export const platform: string
}

// Frame draw (lattice runner). renderFrame() synchronously renders the current
// frame: layout, the postLayout hook, paint and hover refresh, then builds and
// submits the display list. To schedule a future frame instead, use
// requestFrame() from "flux:rendertree". The tree-building surface itself is
// "flux:rendertree" (from @solidrt/flux-types).
declare module "srt:render" {
  export function renderFrame(): void
}
