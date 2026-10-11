// What sol says about a project's own runtime (lib/runtime.ts) when a
// target is not built: the notice when the stock binary will do, the error
// when the app imports a module only that runtime has (a `runtime:*` import,
// BundleOutput.projectModules), and the error for a project that declares no
// runtime at all. Pure, so the bun commands (lib/runtime.ts, lib/artifacts.ts)
// and the dev server (binaries.ts, a flux script) word them the same; the
// server side cannot import the bun side, the bun side imports this (as
// test/main.ts does remap.ts). `declared` is the runtime directory as
// package.json names it, whose first folder is the cargo project to build.

/** How to build the runtime at `declared`. */
export function runtimeBuild(declared: string): string {
  return `make -C ${declared.split("/")[0]}`
}

/** `what` (a binary for a host, the libs for an ABI) is not at `path`; the stock one runs. */
export function stockRuntimeNotice(what: string, path: string, declared: string): string {
  return `[cli] No ${what} at ${path}; using the stock one (build the runtime with ${runtimeBuild(declared)})`
}

/** `what` is not at `path`, and the app imports `needed`, which only this runtime has. */
export function missingRuntimeBinary(what: string, path: string, needed: string, declared: string): string {
  return `No ${what} at ${path}: the app imports ${needed}, which only the project's runtime has. Build the runtime (${runtimeBuild(declared)}) and stage it there.`
}

/** The app imports `needed` and the project declares no runtime. */
export function noRuntimeDeclared(needed: string): string {
  return `The app imports ${needed}, a module only a project runtime has, and package.json declares none ("solidrt": { "runtime": ... }, see the runtime docs, "A custom runtime").`
}
