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
   * Installed apps, sorted by name: id, display name (the installed manifest's
   * displayName, defaulting to the id) and current version id (manifest hash).
   */
  export function list(): { id: string; name: string; version: string }[]
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
}

// Frame draw (lattice runner). renderFrame() synchronously renders the current
// frame: layout, the postLayout hook, paint and hover refresh, then builds and
// submits the display list. To schedule a future frame instead, use
// requestFrame() from "flux:rendertree". The tree-building surface itself is
// "flux:rendertree" (from @solidrt/flux-types).
declare module "srt:render" {
  export function renderFrame(): void
}
