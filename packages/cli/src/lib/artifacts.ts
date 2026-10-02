import { createRequire } from "node:module"
import { existsSync, readFileSync } from "node:fs"
import { resolve, dirname, join } from "node:path"
import process from "node:process"

let require = createRequire(import.meta.url)

let TRIPLE_MAP: Record<string, string> = {
  "linux-x64": "linux-x64-gnu",
  "linux-arm64": "linux-arm64-gnu",
  "darwin-arm64": "darwin-arm64",
  "win32-x64": "win32-x64-msvc",
}

let PKG_MAP: Record<string, string> = {
  "linux-x64": "@solidrt/linux-x64-gnu",
  "linux-arm64": "@solidrt/linux-arm64-gnu",
  "darwin-arm64": "@solidrt/darwin-arm64",
  "win32-x64": "@solidrt/win32-x64-msvc",
}

export function resolveBinary(name: string) {
  let key = `${process.platform}-${process.arch}`
  let ext = process.platform === "win32" ? ".exe" : ""

  // 1. SOLIDRT_HOME: contributors pointing at their local solidrt checkout
  let solRoot = process.env.SOLIDRT_HOME
  if (solRoot) {
    let triple = TRIPLE_MAP[key]
    if (triple) {
      let bin = resolve(solRoot, "dist", triple, name + ext)
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

// The GL libraries the runner needs next to it (or, single-file, embedded as
// kind-3 trailer sections it extracts at boot): ANGLE's libraries on Windows
// and macOS, nothing on platforms with a system GL. Order matters and the
// runner preloads in section order: libGLESv2 must load before libEGL so
// libEGL's import of it resolves against the already-loaded module instead
// of a directory search.
const GL_LIB_NAMES: Partial<Record<NodeJS.Platform, string[]>> = {
  win32: ["libGLESv2.dll", "libEGL.dll"],
  darwin: ["libGLESv2.dylib", "libEGL.dylib"],
}

// The GL libraries shipped next to the runner binary, resolved to their paths.
// Missing files are fatal: a pack without them cannot create a window.
export function runnerGlLibs(runnerPath: string): Array<{ name: string; path: string }> {
  let names = GL_LIB_NAMES[process.platform] ?? []
  let dir = dirname(runnerPath)
  return names.map((name) => {
    let path = join(dir, name)
    if (!existsSync(path)) {
      console.error(`Could not find ${name} next to the runner (${dir}); the packed app needs it to create a GL context.`)
      process.exit(1)
    }
    return { name, path }
  })
}

// The Android APKs are host-independent (they bundle native .so for the
// device, not the host), so they live under dist/android*/<abi>/ rather than
// the host triple map. Each ABI ships a published npm package carrying its
// Player (solidrt-go.apk) and its runner (solidrt.apk), both built for that
// ABI alone; other ABIs (e.g. x86) only resolve via the SOLIDRT_HOME contributor
// path.
export let DEFAULT_ANDROID_ABI = "arm64-v8a"
export let ANDROID_PKG_MAP: Record<string, string> = {
  "arm64-v8a": "@solidrt/android-arm64-v8a",
  "armeabi-v7a": "@solidrt/android-armeabi-v7a",
  x86_64: "@solidrt/android-x86_64",
}

// The installed folder of an ABI's package: the first node_modules/<pkg>
// walking up from the project (the cwd), then from the CLI itself. A plain
// filesystem walk, not require.resolve: Bun caches a failed resolution for
// the life of the process, so a package added mid-run (android-targets.ts)
// would stay unresolvable.
function androidPackageDir(abi: string): string | null {
  let pkg = ANDROID_PKG_MAP[abi]
  if (!pkg) return null
  for (let start of [process.cwd(), import.meta.dir]) {
    for (let dir = start; ; dir = dirname(dir)) {
      let candidate = join(dir, "node_modules", pkg)
      if (existsSync(join(candidate, "package.json"))) return candidate
      if (dirname(dir) === dir) break
    }
  }
  return null
}

// The client version the project expects on an `abi` device: the version of
// its @solidrt/android-<abi> dev dependency (the release action pins it to the
// runtime version). Null when the package is not installed.
export function androidPackageVersion(abi: string): string | null {
  let dir = androidPackageDir(abi)
  if (!dir) return null
  try {
    let version = JSON.parse(readFileSync(join(dir, "package.json"), "utf8")).version
    return typeof version === "string" ? version : null
  } catch {
    return null
  }
}

// The production runner APK `sol pack --apk` patches: staged per ABI by
// `make android-runtime` in a checkout, or shipped inside the
// @solidrt/android-<abi> platform package next to solidrt-go.apk. Runners
// are per-ABI by decision - a shipped app carries one ABI, never a fat APK
// (okf/backlog/standalone-android-apk.md).
export function resolveRunnerApk(abi: string = DEFAULT_ANDROID_ABI): string | null {
  let solRoot = process.env.SOLIDRT_HOME
  if (solRoot) {
    let apk = resolve(solRoot, "dist/android-runtime", abi, "solidrt.apk")
    if (existsSync(apk)) return apk
  }
  let pkgDir = androidPackageDir(abi)
  if (pkgDir) {
    let apk = resolve(pkgDir, "solidrt.apk")
    if (existsSync(apk)) return apk
  }
  return null
}

export function resolveApk(abi: string = DEFAULT_ANDROID_ABI) {
  // 1. SOLIDRT_HOME: contributor checkout, where `make android-dist` stages the APK
  //    under dist/android/<abi>/.
  let solRoot = process.env.SOLIDRT_HOME
  if (solRoot) {
    let apk = resolve(solRoot, "dist/android", abi, "solidrt-go.apk")
    if (existsSync(apk)) return apk
  }

  // 2. Platform npm package
  let pkgDir = androidPackageDir(abi)
  if (pkgDir) {
    let apk = resolve(pkgDir, "solidrt-go.apk")
    if (existsSync(apk)) return apk
  }

  return null
}