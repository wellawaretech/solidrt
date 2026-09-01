import { existsSync } from "node:fs"
import { resolve } from "node:path"
import { loadProject } from "./project"
import { fail } from "./util"

// The fonts `srt pack` appends to a solidrt binary (see
// okf/plans/packaged-fonts.md). By default the three Noto role defaults;
// the project's package.json can override them via the `solidrt.fonts` map
// (alias -> font file path, false to drop a default, true to keep one - the
// scaffold ships the roles as explicit trues so disabling is a one-word
// edit):
//
//   "solidrt": {
//     "fonts": {
//       "sans": "./fonts/Inter.ttf",   // replaces the sans default
//       "serif": true,                 // keeps the serif default
//       "mono": false,                 // drops the mono default
//       "display": "./fonts/F.ttf"     // adds a font under a custom alias
//     }
//   }

/** A resolved font source: the file behind an alias. */
export type ResolvedFont = { alias: string; path: string; isDefault: boolean }

let DEFAULT_FONTS: Record<string, string> = {
  sans: "NotoSans.ttf",
  serif: "NotoSerif.ttf",
  mono: "NotoSansMono.ttf",
}

// Where the default Noto files live: a contributor checkout via SRT_HOME, the
// fonts/ copy shipped inside the published CLI package (staged at release
// time), or the monorepo relative to this source file.
function defaultFontsDir(): string | null {
  let candidates: string[] = []
  if (process.env.SRT_HOME) candidates.push(resolve(process.env.SRT_HOME, "alloy/assets/fonts"))
  candidates.push(resolve(import.meta.dir, "../../fonts"))
  candidates.push(resolve(import.meta.dir, "../../../../alloy/assets/fonts"))
  for (let dir of candidates) {
    if (existsSync(resolve(dir, "NotoSans.ttf"))) return dir
  }
  return null
}

// Resolve the font set for a pack as file paths: role defaults merged with the
// project's `solidrt.fonts` map (shape-checked by loadProject; file mode has
// no project, so defaults only). Order is roles first (sans, serif, mono),
// then added aliases in config order.
export function resolvePackFonts(projectDir: string | null): ResolvedFont[] {
  let project = loadProject(projectDir)
  let overrides = project?.config.fonts ?? {}

  // alias -> path relative to the project dir, or null for a role default.
  let selected = new Map<string, string | null>()
  for (let role of Object.keys(DEFAULT_FONTS)) selected.set(role, null)
  for (let [alias, value] of Object.entries(overrides)) {
    if (typeof value === "boolean") {
      if (!(alias in DEFAULT_FONTS)) {
        fail(`"solidrt.fonts": "${alias}": ${value} toggles a default, but "${alias}" is not one of ${Object.keys(DEFAULT_FONTS).join("/")}`)
      }
      // true keeps the role default already selected above; false drops it.
      if (!value) selected.delete(alias)
    } else {
      selected.set(alias, resolve(project!.dir, value))
    }
  }

  let defaultsDir: string | null = null
  let fonts: ResolvedFont[] = []
  for (let [alias, path] of selected) {
    let isDefault = path === null
    if (path === null) {
      defaultsDir ??= defaultFontsDir() ?? fail(
        "Could not find the default fonts (NotoSans.ttf and friends).\n" +
          "Point SRT_HOME at your SolidRT checkout (and run `make download-fonts` there if needed).",
      )
      path = resolve(defaultsDir, DEFAULT_FONTS[alias]!)
    }
    if (!existsSync(path)) fail(`"solidrt.fonts": "${alias}": no such file: ${path}`)
    fonts.push({ alias, path, isDefault })
  }
  return fonts
}
