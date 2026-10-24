// What `sol init --runtime` writes: the app's own runtime, a cargo project
// over lattice with a starter module (scaffold/runtime/), and the three
// edits that wire it into the app: package.json names the runtime's dist/
// as the app runtime (lib/runtime.ts), tsconfig.json includes the module
// types, .gitignore skips the cargo target. Pure: text in, text out, so the
// tests (tests/init-runtime.test.ts) run it without a file system and
// main.ts does the reading and writing.

/** The runtime folder in the app, and what package.json names. */
export const RUNTIME_DIR = "runtime"
export const RUNTIME_DIST = "runtime/dist"
/** The module types, one `declare module "runtime:<name>"` file each. */
export const RUNTIME_TYPES = "runtime/types"
/** The cargo target dir, for .gitignore (the app's `dist/` already covers runtime/dist). */
export const RUNTIME_TARGET = "runtime/target/"

/**
 * The cargo package name of an app's runtime: the app's package name (its
 * last path segment for a scoped one) as a crate name, with `-runtime`.
 */
export function runtimeCrate(appName: string): string {
  let base = (appName.split("/").pop() ?? "")
    .toLowerCase()
    .replace(/[^a-z0-9_-]+/g, "-")
    .replace(/^-+|-+$/g, "")
  if (!/^[a-z]/.test(base)) base = base ? `app-${base}` : "app"
  return `${base}-runtime`
}

/**
 * The runtime's files from their templates (destination path -> template
 * text), with the placeholders filled: `{{crate}}` the package name,
 * `{{lib}}` the library crate it links as, `{{version}}` the SolidRT
 * release (the lattice tag, the CLI's own version).
 */
export function renderRuntime(templates: Record<string, string>, appName: string, version: string): Record<string, string> {
  let crate = runtimeCrate(appName)
  let fill = (text: string) => text.replaceAll("{{crate}}", crate).replaceAll("{{lib}}", crate.replaceAll("-", "_")).replaceAll("{{version}}", version)
  return Object.fromEntries(Object.entries(templates).map(([path, text]) => [path, fill(text)]))
}

/** package.json with `"solidrt": { "runtime": "runtime/dist" }` set. */
export function withRuntime(packageJson: string): string {
  let pkg = JSON.parse(packageJson)
  pkg.solidrt = { ...(pkg.solidrt ?? {}), runtime: RUNTIME_DIST }
  return JSON.stringify(pkg, null, 2) + "\n"
}

/**
 * tsconfig.json with the module types folder in `include`, or null when
 * the file is not plain JSON (comments, trailing commas), for the caller
 * to say what to add by hand.
 */
export function withRuntimeTypes(tsconfig: string): string | null {
  let config: any
  try {
    config = JSON.parse(tsconfig)
  } catch {
    return null
  }
  let include: string[] = Array.isArray(config.include) ? config.include : []
  if (!include.includes(RUNTIME_TYPES)) config.include = [...include, RUNTIME_TYPES]
  return JSON.stringify(config, null, 2) + "\n"
}

/** .gitignore (or none) with the cargo target dir skipped. */
export function withRuntimeIgnore(gitignore: string | null): string {
  let lines = (gitignore ?? "").split("\n")
  if (lines.includes(RUNTIME_TARGET)) return gitignore ?? ""
  let body = (gitignore ?? "").replace(/\n*$/, "")
  return (body ? body + "\n" : "") + RUNTIME_TARGET + "\n"
}
