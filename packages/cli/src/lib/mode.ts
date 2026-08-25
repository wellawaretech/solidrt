// What a command works on (run, server, bundle, pack, render), decided by
// the cwd and the argument and never by searching upward
// (okf/backlog/cli-flux-migration.md):
//
//   cwd has package.json   argument   mode
//   yes                    none       project (entry = solidrt.entry, default src/index.tsx)
//   no                     none       error
//   no                     file       file
//   yes                    file       error, unless --project (project at cwd,
//                                     entry overridden) or --file (ignore the project)
//
// Project mode hangs assets, fonts, identity, isolates and build outputs off
// the project root; file mode has none of those, the file stands alone. The
// key (the canonical project root, or the canonical file path) is what the
// dev server registry and every control response name.

import { existsSync, realpathSync } from "node:fs"
import { dirname, resolve } from "node:path"
import { values, source, isSource, isPrebuilt } from "./args"
import { loadProject } from "./project"
import { fail } from "./util"

export type Mode =
  | { mode: "project"; key: string; projectDir: string; entry: string }
  | { mode: "file"; key: string; projectDir: null; entry: string }

const DEFAULT_ENTRY = "src/index.tsx"

function canonical(path: string): string {
  try {
    return realpathSync.native(path)
  } catch {
    return resolve(path)
  }
}

export function resolveMode(): Mode {
  let cwd = process.cwd()
  let pkgPath = resolve(cwd, "package.json")
  let hasPkg = existsSync(pkgPath)

  if (source !== undefined && !isSource && !(isPrebuilt && source.endsWith(".srt.js"))) {
    fail(`Not an app entry: ${source} (expected .tsx, .jsx, .ts, .js or .srt.js)`)
  }
  if (source !== undefined && !existsSync(source)) fail(`Entry not found: ${source}`)
  if (values.file && values.project) fail("--file and --project exclude each other")

  if (source === undefined) {
    if (values.file || values.project) fail("--file and --project need an entry file")
    if (!hasPkg) fail(`No package.json in ${cwd}. Run from the project root, or pass a file to use on its own.`)
    let declared = loadProject(cwd)!.config.entry
    let entry = resolve(cwd, declared ?? DEFAULT_ENTRY)
    if (!existsSync(entry)) {
      fail(`Entry not found: ${entry}${declared ? "" : ' (set "solidrt": { "entry": ... } in package.json)'}`)
    }
    let key = canonical(cwd)
    return { mode: "project", key, projectDir: key, entry }
  }

  let entry = canonical(source)
  if (hasPkg && !values.file && !values.project) {
    fail(
      `${cwd} is a project (it has a package.json) and ${source} is a file: pass --project to use the project with this entry, or --file to use the file on its own.`,
    )
  }
  if (hasPkg && values.project) {
    let key = canonical(cwd)
    return { mode: "project", key, projectDir: key, entry }
  }
  if (!hasPkg && values.project) fail(`--project needs a package.json in ${cwd}`)
  return { mode: "file", key: entry, projectDir: null, entry }
}

/** The directory the file routes serve: the entry's directory. */
export function sourceDirOf(mode: Mode): string {
  return dirname(mode.entry)
}
