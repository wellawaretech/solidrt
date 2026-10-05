// Lattice runner builtin modules (the "sol:*" surface). These are ambient
// module declarations, so they live in this global-script .d.ts (no top-level
// import/export) rather than in types.d.ts: an ambient `declare module` only
// becomes globally visible to consumers from a non-module declaration file.

declare module "*.svg" {
  const content: string
  export default content
}

// Text asset imports: `import src from "./effect.glsl" with { type: "text" }`.
// The bundler inlines the file's UTF-8 contents as a string literal, so a
// shader source travels in the bundle and needs no runtime read.
declare module "*.glsl" {
  const content: string
  export default content
}
declare module "*.vert" {
  const content: string
  export default content
}
declare module "*.frag" {
  const content: string
  export default content
}

// Binary asset imports: `import data from "./pic.png" with { type: "binary" }`.
// The bundler inlines the file's bytes as a Uint8Array (see packages/cli
// bundler `inlineImport`); feed it straight into createImage/decodeImage, or a
// `.wasm` into `new Module` from flux:wasm.
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
declare module "*.wav" {
  const bytes: Uint8Array
  export default bytes
}
declare module "*.ogg" {
  const bytes: Uint8Array
  export default bytes
}
declare module "*.glb" {
  const bytes: Uint8Array
  export default bytes
}
declare module "*.sol3m" {
  const bytes: Uint8Array
  export default bytes
}
declare module "*.wasm" {
  const bytes: Uint8Array
  export default bytes
}

// UI event bus (lattice), provided by the runtime as a builtin module.
// on/once return an unsubscribe function. Notable events: the routed pointer
// stream ("pointerMove"/"pointerDown"/... consumed by window.ts),
// "pointerFrame" (the move-batch terminator: fires after all of a frame's
// pointer moves have dispatched, every pointer the same age - multi-pointer
// recognizers measure there; its `timeStamp` is theirs), and "render" (the
// per-frame signal).
declare module "sol:events" {
  export function on(event: string, callback: (data: any) => void): () => void
  export function once(event: string, callback: (data: any) => void): () => void
}

// The running application's own surface (lattice), present in every build.
declare module "sol:app" {
  /**
   * End the current app now, without running the quit hooks: back to the
   * player in a dev client, quit when standalone or at the player root (on
   * Android the activity finishes). Prefer the @solidrt/core `exit`, which
   * runs `onQuit` first.
   */
  export function exit(): void
  /**
   * Leave the current app without ending it: back to the player in a dev
   * client; otherwise to the OS background (Android moveTaskToBack, a
   * minimized window on desktop). Core's default action of an unprevented
   * `back` on Android; exposed there as `background`.
   */
  export function background(): void
  /**
   * Make this install the handler for the app's own scheme (its appId, as
   * in `com.example.app://settings`), so links of it open the app: on Linux
   * a desktop entry plus the mimeapps.list default, on Windows the
   * HKCU\Software\Classes protocol key, both for the current user. Throws
   * when the registration cannot be written. Android and macOS packages
   * declare the scheme themselves, so the call has nothing to do there; a
   * dev client registers nothing and logs a warning. Prefer the
   * @solidrt/core `registerProtocolHandler`, the same call documented with
   * the rest of the link surface.
   */
  export function registerProtocolHandler(): void
}

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
   * it; registrations reset on hot
   * reload, so register at module init. Callable in every build, but only dev
   * clients ever invoke commands.
   */
  export function registerDebug(name: string, fn: (args?: any) => unknown): void
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

// Frame draw (lattice runner). renderFrame() synchronously renders the current
// frame: layout, the postLayout hook, paint and hover refresh, then builds and
// submits the display list. To schedule a future frame instead, use
// requestFrame() from "flux:rendertree". The tree-building surface itself is
// "flux:rendertree" (from @solidrt/flux-types).
declare module "sol:render" {
  export function renderFrame(): void
}

// The engine verbs of an app test (lattice, dev client only), which
// @solidrt/test builds its surface on; tests import that, not this.
// The verbs work only in an engine a test host built (`sol test`) and throw
// anywhere else. App time there is frame / frameRate and nothing else: no
// frame runs unless one is asked for.
declare module "sol:test" {
  /**
   * Runs one frame: timers due by its time fire, then frame callbacks and
   * the flush, then the draw if anything demanded one. Fulfills once the
   * frame has run. One frame at a time: throws while the previous one is
   * still pending.
   */
  export function frame(): Promise<void>
  /** The frames per second this engine steps at (60 unless set). */
  export function frameRate(): number
  /** Another frame rate, a positive integer. Throws once a frame has run. */
  export function setFrameRate(fps: number): void
  /** App time as of the last frame, in milliseconds; 0 before the first. */
  export function time(): number
  /**
   * Fulfills once the window's size has reached the engine, at once when
   * it already has. No frame runs for it and no time passes: a mounted
   * window builds its first frame on that size.
   */
  export function windowReady(): Promise<void>
  /**
   * Expands `events` (JSON text, the control API's `/input` event shape)
   * into the steps that are sent and returns what passes before each:
   * `[ms, frames]` per step, at least that much time and that many frames.
   * Throws on an invalid event, before anything is sent. The steps are then
   * sent with `inputStep`, in order.
   */
  export function inputPlan(events: string): [ms: number, frames: number][]
  /**
   * Sends step `index` of the current plan into the input pipeline: a
   * down, an up, a key or a wheel is dispatched on arrival, ahead of the
   * next frame; a move with that frame.
   */
  export function inputStep(index: number): void
  /**
   * Runs frames until the app is at rest: nothing in flight, no timer due,
   * no frame demanded. Work in flight is waited for with no app time
   * passing. Rejects once `maxMs` of app time have passed without rest,
   * with what is left in the message.
   */
  export function settle(maxMs: number): Promise<void>
  /**
   * Delivers a link the way an OS-routed one arrives (the raw string, on
   * the `link` event, ahead of the next frame). Returns whether anything
   * listens for links.
   */
  export function link(link: string): boolean
  /**
   * Calls a debug command the app registered with `registerDebug`. `args`
   * and the result are JSON text (null for no args). Throws on an unknown
   * name, a throwing command and a result JSON cannot carry.
   */
  export function debug(name: string, args: string | null): string
  /**
   * The pixels a node's subtree paints: RGBA8, premultiplied, rows top to
   * bottom, at the display scale. Drawn at once from the tree as it is; no
   * frame runs and no app time passes.
   */
  export function capture(node: number): { width: number; height: number; data: Uint8Array }
  /**
   * Run one frame with a capture of `node` queued ahead of it, so the
   * frame's own paint services it: fulfills, once the frame has run, with
   * what that frame drew of the node (the shape of `capture`). Written
   * and painted in one frame, where `capture` paints the tree afresh
   * after the fact. One frame at a time, as `frame`.
   */
  export function painted(node: number): Promise<{ width: number; height: number; data: Uint8Array }>
}
