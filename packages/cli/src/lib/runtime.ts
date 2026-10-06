// The project's own runtime (okf/plans/runtime-extension-modules.md): a
// cargo project over lattice with the project's modules, built into a
// directory in the SOLIDRT_HOME/dist layout and named by `"solidrt": {
// "runtime" }` in package.json. The app runtime resolves there first:
// solidrt, solidrt-go and the Android APKs (lib/artifacts.ts); the tooling
// binaries (flux, fluxc, fluxrt) stay sol's own. A target the project did
// not build falls back to the stock binary with a notice: an app that
// checks Flux.capabilities runs either way, and one that imports the module
// fails at the import, which the notice explains.
//
// Android: the project builds its cdylib into android/<abi>/libmain.so (and
// android-runtime/<abi>/ for the runner), with any further .so its modules
// need beside it; the APK is derived from the stock one with those libs
// swapped in (pack/android/apk.ts swapLibs), written beside them and
// rebuilt when a lib or the base is newer.

import { existsSync, readdirSync, readFileSync, statSync, writeFileSync } from "node:fs"
import { join, resolve } from "node:path"
import { swapLibs } from "../pack/android/apk"
import { elfLoadAlignment, MIN_LOAD_ALIGN } from "../pack/android/elf"
import { fail } from "./fail"
import { loadProject } from "./project"

export type ProjectRuntime = {
  /** The runtime directory, absolute. */
  dir: string
  /** `<dir>/<triple>/<name><ext>` when the project built it, else null. */
  binary(name: string, triple: string, ext: string): string | null
  /** The project's Player APK for `abi`, derived from `base` (the stock one), or null when the project built no libs for it. */
  apk(abi: string, base: string | null): string | null
  /** The project's runner APK for `abi`, likewise. */
  runnerApk(abi: string, base: string | null): string | null
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
  let dir = resolve(project.dir, declared)
  if (!existsSync(dir)) {
    console.log(`[cli] No project runtime at ${dir} yet; using the stock binaries (build it with make -C ${declared.split("/")[0]})`)
    return null
  }
  let noticed = new Set<string>()
  return {
    dir,
    binary(name, triple, ext) {
      let path = join(dir, triple, name + ext)
      if (existsSync(path)) return path
      if (!noticed.has(path)) {
        noticed.add(path)
        console.log(`[cli] No ${name} for ${triple} in ${dir}; using the stock one`)
      }
      return null
    },
    apk: (abi, base) => derive("Player", join(dir, "android", abi), PLAYER_APK, abi, base),
    runnerApk: (abi, base) => derive("runner", join(dir, "android-runtime", abi), RUNNER_APK, abi, base),
  }
}

// The derived APK at <libDir>/<out>: the base with every .so in libDir
// swapped in, rebuilt when any of them or the base is newer than it.
function derive(what: string, libDir: string, out: string, abi: string, base: string | null): string | null {
  let names = existsSync(libDir) ? readdirSync(libDir).filter((name) => name.endsWith(".so")).sort() : []
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
  writeFileSync(outPath, swapLibs(readFileSync(base), abi, libs))
  console.log(`[cli] Derived ${outPath} from ${base} with ${names.join(", ")}`)
  return outPath
}
