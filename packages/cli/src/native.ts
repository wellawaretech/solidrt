import { createRequire } from "node:module"
import { existsSync } from "node:fs"
import { resolve, dirname } from "node:path"
import { fileURLToPath } from "node:url"
import process from "node:process"

let require = createRequire(import.meta.url)

let DIST_MAP: Record<string, string> = {
  "linux-x64": "linux-x64-gnu",
  "darwin-arm64": "darwin-arm64",
  "darwin-x64": "darwin-x64",
  "win32-x64": "win32-x64-msvc",
}

let PKG_MAP: Record<string, string> = {
  "linux-x64": "@solidrt/linux-x64-gnu",
  "win32-x64": "@solidrt/win32-x64-msvc",
}

let __dirname = dirname(fileURLToPath(import.meta.url))
let repoRoot = resolve(__dirname, "../..")

export function resolveBinary(name: string) {
  let key = `${process.platform}-${process.arch}`
  let ext = process.platform === "win32" ? ".exe" : ""

  // 1. Check lattice/dist/<host-triple>/ (staged by Makefile)
  let triple = DIST_MAP[key]
  if (triple) {
    let dist = resolve(repoRoot, `lattice/dist/${triple}/${name}${ext}`)
    if (existsSync(dist)) return dist
  }

  // 2. Fall back to platform npm package
  let pkg = PKG_MAP[key]
  if (pkg) {
    try {
      let pkgDir = dirname(require.resolve(`${pkg}/package.json`))
      let bin = resolve(pkgDir, name + ext)
      if (existsSync(bin)) return bin
    } catch {}
  }

  return null
}