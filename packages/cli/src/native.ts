import { createRequire } from "node:module"
import { existsSync } from "node:fs"
import { resolve, dirname } from "node:path"
import process from "node:process"

let require = createRequire(import.meta.url)

let TRIPLE_MAP: Record<string, string> = {
  "linux-x64": "linux-x64-gnu",
  "darwin-arm64": "darwin-arm64",
  "win32-x64": "win32-x64-msvc",
}

let PKG_MAP: Record<string, string> = {
  "linux-x64": "@solidrt/linux-x64-gnu",
  "darwin-arm64": "@solidrt/darwin-arm64",
  "win32-x64": "@solidrt/win32-x64-msvc",
}

export function resolveBinary(name: string) {
  let key = `${process.platform}-${process.arch}`
  let ext = process.platform === "win32" ? ".exe" : ""

  // 1. SRT_HOME: contributors pointing at their local solidrt checkout
  let srtRoot = process.env.SRT_HOME
  if (srtRoot) {
    let triple = TRIPLE_MAP[key]
    if (triple) {
      let bin = resolve(srtRoot, "lattice", "dist", triple, name + ext)
      if (existsSync(bin)) return bin
    }
  }

  // 2. Platform npm package (installed via optionalDependencies)
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