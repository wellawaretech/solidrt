import { values, source } from "../lib/args"
import { bundleFlux, bundleSolid, compileToBytecode, findFluxIsolates } from "../bundle/bundler"
import { resolvePackFonts } from "../lib/fonts"
import { loadAppIdentity } from "../lib/project"
import { resolveMode } from "../lib/mode"
import { packApp, packFlux, packSolid } from "./trailer"
import { buildPackFolder, writePackFolder } from "./layout"
import { patchApk } from "./android/apk"
import { requireBinary } from "../lib/util"
import { resolveApk, resolveRunnerApk, ANDROID_PKG_MAP } from "../lib/artifacts"
import { readFileSync } from "node:fs"
import { dirname, join, resolve } from "node:path"

// Android package names are stricter than a general appId: at least two
// dot-separated segments, each starting with a letter.
const ANDROID_APP_ID = /^[a-zA-Z][a-zA-Z0-9_]*(\.[a-zA-Z][a-zA-Z0-9_]*)+$/

// Windows executables need the suffix; a user-given --output may already
// carry it.
function exeName(outfile: string): string {
  return process.platform === "win32" && !outfile.toLowerCase().endsWith(".exe") ? outfile + ".exe" : outfile
}

// Write the packed executable, mark it runnable, and report its size.
async function writeExecutable(packed: Buffer, outfile: string) {
  await Bun.write(outfile, packed)
  if (process.platform !== "win32") {
    Bun.spawnSync(["chmod", "+x", outfile])
  }
  console.log(`>> wrote ${packed.length} bytes to ${outfile}`)
}

export async function main() {
  if (values.flux) {
    if (values.folder || values.app || values.apk) {
      console.error("--folder, --app and --apk are for app packs; flux scripts have no folder, .srtapp or APK output")
      process.exit(1)
    }
    let outfile = exeName(values.output ?? source!.replace(/\.[jt]s$/, ""))
    // The entry's isolate modules ride along as isolates/<id>.bin sections
    // (module name = id, for stack attribution).
    let isolates = []
    for (let module of findFluxIsolates(dirname(resolve(source!)))) {
      isolates.push({ id: module.id, bytecode: await compileToBytecode(await bundleFlux(module.path), module.id) })
    }
    if (isolates.length) console.log(`>> isolates: ${isolates.map((i) => i.id).join(", ")}`)
    await writeExecutable(packFlux(await compileToBytecode(await bundleFlux(source!)), isolates), outfile)
    process.exit()
  }

  // All solidrt outputs are the same canonical pack: manifest + bundle.bin +
  // assets (fonts included). --folder writes it as a flat folder next to a
  // bare runner; --app writes it alone as one .srtapp for a runner to load;
  // the default single-file exe carries it as trailer sections.
  let mode = resolveMode()
  let identity = loadAppIdentity(mode.entry, mode.projectDir)
  console.log(`>> app: ${identity.appId} (${identity.org} / ${identity.displayName})`)
  if (identity.defaulted) {
    console.warn('>> warning: no "solidrt.appId" in package.json; set a stable reverse-DNS id before distributing')
  }
  let fonts = resolvePackFonts(mode.projectDir)
  console.log(`>> fonts: ${fonts.length ? fonts.map((f) => f.alias).join(", ") : "none"}`)

  let bundled = await bundleSolid(mode)
  let bytecode = await compileToBytecode(bundled.code)
  let isolates = []
  for (let i of bundled.isolates) isolates.push({ id: i.id, bytecode: await compileToBytecode(i.code, i.id) })
  if (isolates.length) console.log(`>> isolates: ${isolates.map((i) => i.id).join(", ")}`)
  let folder = buildPackFolder(mode, bytecode, isolates)

  // --apk patches the app into an installable Android APK: application id and
  // label rewritten, the .srtapp payload added as a stored asset, re-aligned
  // and re-signed - pure TypeScript, no Android SDK
  // (okf/backlog/standalone-android-apk.md). The base is the production
  // runner APK (`make runtime-android`), which boots the payload; while none
  // is staged, the solidrt-go dev client stands in - that APK installs and
  // launches, but boots the launcher instead of the payload.
  if (values.apk) {
    if (!ANDROID_APP_ID.test(identity.appId)) {
      console.error(
        `"solidrt": "appId" ("${identity.appId}") is not a valid Android application id: use reverse-DNS with at least two dot-separated segments, each starting with a letter (e.g. "com.example.app")`,
      )
      process.exit(1)
    }
    let base = resolveRunnerApk()
    if (!base) {
      base = resolveApk()
      if (base) {
        console.log(">> note: no runner APK staged; using the go dev client as the base - the payload rides along unloaded")
      }
    }
    if (!base) {
      console.error(`Could not find a base APK; run make runtime-android, or add the ${ANDROID_PKG_MAP["arm64-v8a"]} dev dependency`)
      process.exit(1)
    }
    console.log(`>> base: ${base}`)
    let patched = patchApk(readFileSync(base), identity.appId, identity.displayName, packApp(folder, bytecode))
    let outfile = values.output ?? mode.entry.replace(/\.[jt]sx?$/, ".apk")
    await Bun.write(outfile, patched)
    console.log(`>> wrote ${patched.length} bytes to ${outfile}`)
    process.exit()
  }

  if (values.folder) {
    let outDir = values.output ?? join("dist", "pack")
    writePackFolder(outDir, requireBinary("solidrt"), bytecode, folder)
    console.log(`>> wrote pack folder to ${resolve(outDir)}`)
    process.exit()
  }

  if (values.app) {
    let outfile = values.output ?? mode.entry.replace(/\.[jt]sx?$/, ".srtapp")
    let packed = packApp(folder, bytecode)
    await Bun.write(outfile, packed)
    console.log(`>> wrote ${packed.length} bytes to ${outfile}`)
    process.exit()
  }

  let outfile = values.output ?? mode.entry.replace(/\.[jt]sx?$/, "")
  // On Windows the packed image is a PE executable; it needs a .exe name to run.
  if (process.platform === "win32" && !outfile.toLowerCase().endsWith(".exe")) {
    outfile += ".exe"
  }
  await writeExecutable(packSolid(folder, bytecode), outfile)
  process.exit()
}
