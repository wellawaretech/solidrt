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

// The Android client APK is host-independent (it bundles native .so for the
// device, not the host), so it lives under dist/android/<abi>/ rather than the
// host triple map. arm64-v8a (a fat APK: arm64-v8a + the x86_64 emulator) and
// armeabi-v7a (32-bit) ship published npm packages; other ABIs (e.g. x86) only
// resolve via the SRT_HOME contributor path.
let DEFAULT_ANDROID_ABI = "arm64-v8a"
export let ANDROID_PKG_MAP: Record<string, string> = {
  "arm64-v8a": "@solidrt/android-arm64-v8a",
  "armeabi-v7a": "@solidrt/android-armeabi-v7a",
}

export function resolveApk(abi: string = DEFAULT_ANDROID_ABI) {
  // 1. SRT_HOME: contributor checkout, where `make dist-android` stages the APK
  //    under dist/android/<abi>/.
  let srtRoot = process.env.SRT_HOME
  if (srtRoot) {
    let apk = resolve(srtRoot, "dist/android", abi, "solidrt-go.apk")
    if (existsSync(apk)) return apk
  }

  // 2. Platform npm package
  let pkg = ANDROID_PKG_MAP[abi]
  if (pkg) {
    try {
      let pkgDir = dirname(require.resolve(`${pkg}/package.json`))
      let apk = resolve(pkgDir, "solidrt-go.apk")
      if (existsSync(apk)) return apk
    } catch {}
  }

  return null
}