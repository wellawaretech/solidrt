// What this server serves, decided by the cwd and the entry argument and
// never by searching upward. The rule and its table live in
// packages/cli/src/lib/mode.ts (the bun copy the one-shot commands use); this is
// the same table for the server, over flux:fs - two honest copies rather
// than an fs shim (okf/done/srt-command-folders.md):
//
//   cwd has package.json   argument   mode
//   yes                    none       project (entry = solidrt.entry, default src/index.tsx)
//   no                     none       error
//   no                     file       file
//   yes                    file       error, unless --project (project at cwd,
//                                     entry overridden) or --file (ignore the project)
//
// The key (the canonical project root, or the canonical file path) is what
// the registry and every control response name.

import { file, realpath } from "flux:fs"
import { join } from "flux:path"
import { fail } from "./args"

export type Mode =
  | { mode: "project"; key: string; projectDir: string; entry: string }
  | { mode: "file"; key: string; projectDir: null; entry: string }

const DEFAULT_ENTRY = "src/index.tsx"
// A prebuilt .srt.js ends with .js, so it is admitted by the same list.
const ENTRY_EXTENSIONS = [".tsx", ".jsx", ".ts", ".js"]

export async function resolveMode(args: { entry: string | undefined; project: boolean; file: boolean }): Promise<Mode> {
  let cwd = await realpath(".")
  let hasPkg = await file(join(cwd, "package.json")).exists()
  let source = args.entry

  if (source !== undefined && !ENTRY_EXTENSIONS.some((ext) => source.endsWith(ext))) {
    fail(`Not an app entry: ${source} (expected .tsx, .jsx, .ts, .js or .srt.js)`)
  }
  if (source !== undefined && !(await file(source).exists())) fail(`Entry not found: ${source}`)
  if (args.file && args.project) fail("--file and --project exclude each other")

  if (source === undefined) {
    if (args.file || args.project) fail("--file and --project need an entry file")
    if (!hasPkg) fail(`No package.json in ${cwd}. Run from the project root, or pass a file to use on its own.`)
    let declared = await declaredEntry(cwd)
    let entry = join(cwd, declared ?? DEFAULT_ENTRY)
    if (!(await file(entry).exists())) {
      fail(`Entry not found: ${entry}${declared ? "" : ' (set "solidrt": { "entry": ... } in package.json)'}`)
    }
    return { mode: "project", key: cwd, projectDir: cwd, entry }
  }

  let entry = await realpath(source)
  if (hasPkg && !args.file && !args.project) {
    fail(
      `${cwd} is a project (it has a package.json) and ${source} is a file: pass --project to use the project with this entry, or --file to use the file on its own.`,
    )
  }
  if (hasPkg && args.project) return { mode: "project", key: cwd, projectDir: cwd, entry }
  if (!hasPkg && args.project) fail(`--project needs a package.json in ${cwd}`)
  return { mode: "file", key: entry, projectDir: null, entry }
}

// The project's declared entry (`"solidrt": { "entry" }` in package.json),
// relative to the project root. Only the one field the server needs is
// checked here; the bun commands validate the whole key (src/lib/project.ts).
async function declaredEntry(dir: string): Promise<string | undefined> {
  let pkg = await file(join(dir, "package.json"))
    .json()
    .catch(() => fail(`Unreadable package.json in ${dir}`))
  let config = pkg?.solidrt
  if (config === undefined) return undefined
  if (typeof config !== "object" || config === null || Array.isArray(config)) {
    fail('"solidrt" in package.json must be an object')
  }
  if ("entry" in config && typeof config.entry !== "string") fail('"solidrt": "entry" must be a string')
  return config.entry
}

/** The directory the file routes serve: the entry's directory. */
export function sourceDirOf(mode: Mode): string {
  return dirname(mode.entry)
}

/** The parent of a path (flux:path has no dirname); "." for a bare name. */
export function dirname(path: string): string {
  let i = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"))
  return i <= 0 ? (i === 0 ? path.slice(0, 1) : ".") : path.slice(0, i)
}

/** `path` made absolute against `base` when it is not already. */
export function absolute(path: string, base: string): string {
  return /^([A-Za-z]:)?[\\/]/.test(path) ? path : join(base, path)
}
