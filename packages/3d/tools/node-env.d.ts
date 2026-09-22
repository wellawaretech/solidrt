// The node surface the tools use, declared here so `srt check
// packages/3d/tools/<name>.ts` typechecks them on the package's flux-typed
// program (the nearest tsconfig is the repo root's). The tools run under
// bun, but bun's own types cannot join this program: their globals
// (Response, fetch, process) collide with the flux standards the rest of
// the chain is typed against - the same split packages/cli keeps. Each
// tool references this file with a triple-slash directive; nothing else
// includes it, and a scaffolded app never roots a program at a tool.

declare module "node:fs" {
  export function readFileSync(path: string): Uint8Array
  export function writeFileSync(path: string, data: Uint8Array | string): void
}

declare module "node:path" {
  export function basename(path: string, ext?: string): string
  export function dirname(path: string): string
  export function extname(path: string): string
  export function join(...parts: string[]): string
}

declare const process: {
  argv: string[]
  exit(code?: number): never
}
