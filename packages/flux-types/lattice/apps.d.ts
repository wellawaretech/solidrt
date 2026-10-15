// Installed-app management (lattice), the player's surface over the client's
// version store. Present only in go/dev client builds; elsewhere `available`
// is false, `list` returns [], and launch/remove are no-ops.
declare module "sol:apps" {
  export const available: boolean
  /**
   * An installed app: id, display name (the installed manifest's displayName,
   * defaulting to the id) and current version id (manifest hash). `updated` is
   * when that version became current, in milliseconds since the epoch (0 when
   * the store's timestamp is unreadable); a repush of an identical manifest
   * installs nothing and leaves it alone. `size` is the version's
   * manifest-declared size (bundle plus assets) - claimed rather than walked,
   * so that listing stays cheap; `info()` reports what is actually on disk.
   * `icon` is the manifest-declared icon's SVG source, ready for `parseSvg`;
   * absent when the app declares none (or the file is unreadable).
   */
  export type InstalledApp = {
    id: string
    name: string
    icon?: string
    version: string
    updated: number
    size: number
  }
  /** Installed apps, most recently updated first. */
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
   * player). Throws when the app is not installed. Custom fonts of the
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
   * Build identity of this runtime, for the player's settings screen. Not
   * app-specific, but surfaced here since the player already imports this
   * module. `version` is the release version (git describe; "0.0.0-dev" in a
   * plain build), `profile` is "debug" or "release", `platform` is the OS
   * (std::env::consts::OS, e.g. "linux", "android", "windows", "macos").
   */
  export const version: string
  export const profile: string
  export const platform: string
}
