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
}

// Frame draw (lattice runner). renderFrame() synchronously renders the current
// frame: layout, the postLayout hook, paint and hover refresh, then builds and
// submits the display list. To schedule a future frame instead, use
// requestFrame() from "flux:rendertree". The tree-building surface itself is
// "flux:rendertree" (from @solidrt/flux-types).
declare module "srt:render" {
  export function renderFrame(): void
}
