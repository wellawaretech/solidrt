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
      let bin = resolve(srtRoot, "dist", triple, name + ext)
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

// The Android client APK is per-ABI (it bundles native .so for one architecture)
// and host-independent, so it lives under dist/android/<abi>/ rather than the
// host triple map. Only arm64-v8a is supported for now.
let ANDROID_ABI = "arm64-v8a"
let ANDROID_PKG = "@solidrt/android-arm64-v8a"

export function resolveApk() {
  // 1. SRT_HOME: contributor checkout, where `make dist-android` stages the APK
  //    under dist/android/<abi>/.
  let srtRoot = process.env.SRT_HOME
  if (srtRoot) {
    let apk = resolve(srtRoot, "dist/android", ANDROID_ABI, "solidrt-go.apk")
    if (existsSync(apk)) return apk
  }

  // 2. Platform npm package
  try {
    let pkgDir = dirname(require.resolve(`${ANDROID_PKG}/package.json`))
    let apk = resolve(pkgDir, "solidrt-go.apk")
    if (existsSync(apk)) return apk
  } catch {}

  return null
}