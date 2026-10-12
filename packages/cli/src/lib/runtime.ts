// The project's own runtime (okf/plans/runtime-extension-modules.md): a
// cargo project over lattice with the project's modules, built into a
// directory in the SOLIDRT_HOME/dist layout and named by `"solidrt": {
// "runtime" }` in package.json. The app runtime resolves there first:
// solidrt, solidrt-go and the Android APKs (lib/artifacts.ts); the tooling
// binaries (flux, fluxc, fluxrt) stay sol's own. A target the project did
// not build falls back to the stock binary with a notice: an app that
// checks Flux.capabilities runs either way. One that imports a module of
// the runtime (a `runtime:*` import, `BundleOutput.projectModules`) cannot,
// so a caller that knows the bundle names the module as `needed`, and the
// missing binary is then an error naming the path, before the app fails at
// its first import as if the module were at fault.
//
// Android: the project builds its cdylib into android/<abi>/libmain.so (and
// android-runtime/<abi>/ for the runner), with any further .so its modules
// need beside it; the APK is derived from the stock one with those libs
// swapped in (pack/android/apk.ts swapLibs), written beside them and
// rebuilt when a lib or the base is newer. The project's Player is its own
// app on the device, `<appId>.player` labelled "<displayName> Player"
// (playerIdentity), installed beside the stock Player and beside other
// projects' rather than over them, and told apart from the project's packed
// app by the suffix; so a project with a runtime needs an appId that is a
// valid Android id. `sol init --runtime` sets one, and a project without is
// refused here.

import { existsSync, readdirSync, readFileSync, statSync, writeFileSync } from "node:fs"
import { join, resolve } from "node:path"
import { swapLibs } from "../pack/android/apk"
import { elfLoadAlignment, MIN_LOAD_ALIGN } from "../pack/android/elf"
import { missingRuntimeBinary, noRuntimeDeclared, stockRuntimeNotice } from "../server/runtime-messages"
import { fail } from "./fail"
import { projectEntry } from "./mode"
import { ANDROID_APP_ID, ANDROID_APP_ID_HINT, loadAppIdentity, loadProject, runtimeFontAliases, type AppIdentity } from "./project"

/** The application id and launcher label of the Player of the app `identity`, a custom runtime's. */
export function playerIdentity(identity: AppIdentity): { appId: string; label: string } {
  return { appId: `${identity.appId}.player`, label: `${identity.displayName} Player` }
}

/**
 * Each resolver takes `needed`: a `runtime:*` module the app imports, or
 * null. With it, a target the project did not build is an error naming the
 * path (the stock binary has no such module); without it, a notice and null.
 */
export type ProjectRuntime = {
  /** The runtime directory, absolute. */
  dir: string
  /** `<dir>/<triple>/<name><ext>` when the project built it, else null. */
  binary(name: string, triple: string, ext: string, needed?: string | null): string | null
  /** The project's Player APK for `abi`, derived from `base` (the stock one), or null when the project built no libs for it. */
  apk(abi: string, base: string | null, needed?: string | null): string | null
  /** The project's runner APK for `abi`, likewise. */
  runnerApk(abi: string, base: string | null, needed?: string | null): string | null
  /** Whether the project built Player or runner libs for `abi`: a probe that derives nothing and prints nothing. */
  hasLibs(abi: string): boolean
  /** The font aliases the runtime binds, from the listing its host build wrote; null when it has not been built here. */
  fonts(): string[] | null
}

const PLAYER_APK = "solidrt-go.apk"
const RUNNER_APK = "solidrt.apk"

let resolved: ProjectRuntime | null | undefined

/** The cwd project's runtime, once per process; null without one. */
export function projectRuntime(): ProjectRuntime | null {
  if (resolved === undefined) resolved = load()
  return resolved
}

function load(): ProjectRuntime | null {
  let project = loadProject(existsSync("package.json") ? process.cwd() : null)
  let declared = project?.config.runtime
  if (!project || !declared) return null
  let appId = project.config.appId
  if (appId === undefined || !ANDROID_APP_ID.test(appId)) {
    fail(
      `"solidrt": "runtime" needs "appId" to be an Android application id (the runtime's player is named after it, ${appId === undefined ? "none is set" : `"${appId}" is not one`}): use ${ANDROID_APP_ID_HINT}`,
    )
  }
  let dir = resolve(project.dir, declared)
  let player = playerIdentity(loadAppIdentity(projectEntry(project.dir), project.dir))
  let noticed = new Set<string>()
  // A target the project did not build: the error when a module of the
  // runtime is needed, else null, with a notice (once per path) when a
  // stock one stands in. Without a stock one for the target either (an
  // Android ABI with no platform package installed) there is nothing to
  // say: the target is simply not there.
  let missing = (what: string, path: string, needed: string | null | undefined, stock: boolean): null => {
    if (needed) fail(missingRuntimeBinary(what, path, needed, declared))
    if (stock && !noticed.has(path)) {
      noticed.add(path)
      console.log(stockRuntimeNotice(what, path, declared))
    }
    return null
  }
  return {
    dir,
    binary(name, triple, ext, needed) {
      let path = join(dir, triple, name + ext)
      return existsSync(path) ? path : missing(`${name} for ${triple}`, path, needed, true)
    },
    apk(abi, base, needed) {
      let libDir = join(dir, "android", abi)
      return derive("Player", libDir, PLAYER_APK, abi, base, player) ?? missing(`Player libs for ${abi}`, libDir, needed, base !== null)
    },
    runnerApk(abi, base, needed) {
      let libDir = join(dir, "android-runtime", abi)
      return derive("runner", libDir, RUNNER_APK, abi, base, null) ?? missing(`runner libs for ${abi}`, libDir, needed, base !== null)
    },
    hasLibs(abi) {
      return libNames(join(dir, "android", abi)).length > 0 || libNames(join(dir, "android-runtime", abi)).length > 0
    },
    fonts() {
      return runtimeFontAliases(dir)
    },
  }
}

// The .so files in `libDir`, sorted; none when the directory is not there.
function libNames(libDir: string): string[] {
  return existsSync(libDir) ? readdirSync(libDir).filter((name) => name.endsWith(".so")).sort() : []
}

/**
 * The error for an app that imports `needed`, a module only a project
 * runtime has, in a project that declares no runtime: no binary anywhere
 * carries it.
 */
export function failNoRuntimeFor(needed: string): never {
  fail(noRuntimeDeclared(needed))
}

// The derived APK at <libDir>/<out>: the base with every .so in libDir
// swapped in and, for the Player, the application id and label set to
// `identity`, rebuilt when any of them or the base is newer than it.
function derive(
  what: string,
  libDir: string,
  out: string,
  abi: string,
  base: string | null,
  identity: { appId: string; label: string } | null,
): string | null {
  let names = libNames(libDir)
  if (names.length === 0) return null
  if (!base) fail(`No stock ${what} APK for ${abi} to derive the project's from; add the @solidrt/android-${abi} dev dependency`)
  let outPath = join(libDir, out)
  let newest = Math.max(statSync(base).mtimeMs, ...names.map((name) => statSync(join(libDir, name)).mtimeMs))
  if (existsSync(outPath) && statSync(outPath).mtimeMs >= newest) return outPath
  let libs = names.map((name) => ({ name, bytes: readFileSync(join(libDir, name)) }))
  for (let lib of libs) {
    let align = elfLoadAlignment(lib.bytes)
    if (align !== null && align < MIN_LOAD_ALIGN) {
      fail(`${join(libDir, lib.name)} has a LOAD segment aligned to ${align} bytes; Android needs ${MIN_LOAD_ALIGN} (link with -z max-page-size=${MIN_LOAD_ALIGN})`)
    }
  }
  writeFileSync(outPath, swapLibs(readFileSync(base), abi, libs, identity))
  console.log(`[cli] Derived ${outPath}${identity ? ` (${identity.appId}, "${identity.label}")` : ""} from ${base} with ${names.join(", ")}`)
  return outPath
}
