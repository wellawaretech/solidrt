// How `sol tool` reads a source and a tool file, with no node builtins so
// the rules test under flux (tests/tool.test.ts). A tool runs under bun
// unless its file is `<name>.flux.ts`, which runs under the flux binary;
// an installed tool is named by its file without the suffix, so a name
// never ends in `.ts` and a source that does is a path to a project's
// own script.

const SCRIPT_SUFFIX = ".ts"
const FLUX_SUFFIX = ".flux.ts"

export type Host = "bun" | "flux"

/** Whether a file is a tool at all: a `.ts` that is not a declaration. */
export function isToolFile(file: string): boolean {
  return file.endsWith(SCRIPT_SUFFIX) && !file.endsWith(".d.ts")
}

/** The host a tool file runs under, by its suffix. */
export function hostOf(file: string): Host {
  return file.endsWith(FLUX_SUFFIX) ? "flux" : "bun"
}

/** An installed tool's name within its package: the file without its
 * suffix (`model.flux.ts` and `model.ts` are both `model`). */
export function nameOf(file: string): string {
  return file.slice(0, -(file.endsWith(FLUX_SUFFIX) ? FLUX_SUFFIX : SCRIPT_SUFFIX).length)
}

/** Whether a source names a file to run rather than an installed tool. */
export function isScriptPath(source: string): boolean {
  return source.endsWith(SCRIPT_SUFFIX)
}
