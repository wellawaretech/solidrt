import { existsSync, readFileSync } from "node:fs"
import { resolve } from "node:path"
import { findProjectPackage } from "./project"

// The fonts `srt pack` appends to a solidrt binary (see
// okf/plans/packaged-fonts.md). By default the three Noto role defaults;
// the project's package.json can override them via the `solidrt.fonts` map
// (alias -> font file path, or false to drop a default):
//
//   "solidrt": {
//     "fonts": {
//       "sans": "./fonts/Inter.ttf",   // replaces the sans default
//       "mono": false,                 // drops the mono default
//       "display": "./fonts/F.ttf"     // adds a font under a custom alias
//     }
//   }

export type PackFont = { alias: string; bytes: Buffer }

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
  candidates.push(resolve(import.meta.dir, "../fonts"))
  candidates.push(resolve(import.meta.dir, "../../../alloy/assets/fonts"))
  for (let dir of candidates) {
    if (existsSync(resolve(dir, "NotoSans.ttf"))) return dir
  }
  return null
}

function findProjectConfig(sourcePath: string): { dir: string; fonts: unknown } | null {
  let project = findProjectPackage(sourcePath)
  return project && { dir: project.dir, fonts: project.pkg.solidrt?.fonts }
}

function fail(message: string): never {
  console.error(message)
  process.exit(1)
}

// Resolve the font set for a pack: role defaults merged with the project's
// `solidrt.fonts` map, loaded into memory. Order is roles first (sans, serif,
// mono), then added aliases in config order.
export function loadPackFonts(sourcePath: string): PackFont[] {
  let config = findProjectConfig(sourcePath)
  let overrides = config?.fonts ?? {}
  if (typeof overrides !== "object" || overrides === null || Array.isArray(overrides)) {
    fail('The "solidrt.fonts" key in package.json must be a map of alias to font file path (or false)')
  }

  // alias -> path relative to the config dir, or null for a role default.
  let selected = new Map<string, string | null>()
  for (let role of Object.keys(DEFAULT_FONTS)) selected.set(role, null)
  for (let [alias, value] of Object.entries(overrides)) {
    if (value === false) {
      if (!(alias in DEFAULT_FONTS)) fail(`"solidrt.fonts": "${alias}": false drops a default, but "${alias}" is not one of ${Object.keys(DEFAULT_FONTS).join("/")}`)
      selected.delete(alias)
    } else if (typeof value === "string") {
      if (Buffer.byteLength(alias, "utf8") > 255) fail(`"solidrt.fonts": alias "${alias}" is too long (max 255 bytes)`)
      selected.set(alias, resolve(config!.dir, value))
    } else {
      fail(`"solidrt.fonts": "${alias}" must be a font file path or false, got ${JSON.stringify(value)}`)
    }
  }

  let defaultsDir: string | null = null
  let fonts: PackFont[] = []
  for (let [alias, path] of selected) {
    if (path === null) {
      defaultsDir ??= defaultFontsDir() ?? fail(
        "Could not find the default fonts (NotoSans.ttf and friends).\n" +
          "Point SRT_HOME at your SolidRT checkout (and run `make download-fonts` there if needed).",
      )
      path = resolve(defaultsDir, DEFAULT_FONTS[alias]!)
    }
    if (!existsSync(path)) fail(`"solidrt.fonts": "${alias}": no such file: ${path}`)
    fonts.push({ alias, bytes: readFileSync(path) })
  }
  return fonts
}
