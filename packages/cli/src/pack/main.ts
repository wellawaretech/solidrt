import { values, source } from "../lib/args"
import { bundleFlux, bundleSolid, compileToBytecode, findFluxIsolates } from "../bundle/bundler"
import { resolvePackFonts } from "../lib/fonts"
import { loadAppIdentity, loadProject, resolveCapabilities, type Capability, type Project } from "../lib/project"
import { resolveMode, type Mode } from "../lib/mode"
import { packApp, packFlux, packSolid } from "./trailer"
import { buildPackFolder, writePackFolder } from "./layout"
import { patchApk } from "./android/apk"
import { isPng } from "./android/icon"
import { requireBinary } from "../lib/util"
import { resolveRunnerApk, ANDROID_PKG_MAP, DEFAULT_ANDROID_ABI } from "../lib/artifacts"
import { ensureTargets } from "../lib/android-targets"
import { existsSync, readFileSync } from "node:fs"
import { basename, dirname, join, resolve } from "node:path"

// Android package names are stricter than a general appId: at least two
// dot-separated segments, each starting with a letter.
const ANDROID_APP_ID = /^[a-zA-Z][a-zA-Z0-9_]*(\.[a-zA-Z][a-zA-Z0-9_]*)+$/
// Where an unconfigured APK starts on Android's update ordering.
const DEFAULT_VERSION_CODE = 1
// Display version when package.json declares none (matches the runner base).
const DEFAULT_VERSION_NAME = "1.0"
// Adaptive-icon background when the project sets no iconBackground: the
// near-black ground of the unbranded grayscale scaffold icon.
const DEFAULT_ICON_BACKGROUND = "#1a1a1a"
// The Android permission each capability declares. The runner declares none
// itself; SDL requests the runtime grant when a feature opens, which only
// works for a declared permission.
const ANDROID_CAPABILITY_PERMISSIONS: Record<Capability, string> = {
  camera: "android.permission.CAMERA",
  microphone: "android.permission.RECORD_AUDIO",
  vibration: "android.permission.VIBRATE",
  internet: "android.permission.INTERNET",
}

// The app's Android launcher icon as PNG bytes: the `icon` config key when it
// names a .png, else the assets/icon.png convention. An SVG icon (the desktop
// and player-tile source) cannot be rasterized at pack time, so with no PNG
// beside it the runner's placeholder stays and a note says so.
function resolveLauncherIcon(project: Project | null): Buffer | undefined {
  if (!project) return undefined
  let declared = project.config.icon
  let path = declared?.endsWith(".png") ? resolve(project.dir, declared) : join(project.dir, "assets", "icon.png")
  if (!existsSync(path)) {
    if (declared?.endsWith(".png")) {
      console.error(`"solidrt": "icon" names ${declared}, which does not exist`)
      process.exit(1)
    }
    // Declared SVG or the conventional assets/icon.svg: the app has icon
    // artwork, just not in a form the patcher can use.
    if (declared || existsSync(join(project.dir, "assets", "icon.svg"))) {
      console.log(">> note: an SVG icon cannot be rasterized at pack time; add assets/icon.png for the launcher icon")
    }
    return undefined
  }
  let bytes = readFileSync(path)
  if (!isPng(bytes)) {
    console.error(`${path} is not a PNG file`)
    process.exit(1)
  }
  return bytes
}

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

// A per-target APK path: the ABI goes in before the extension, so the
// targets land side by side (dist/<name>-arm64-v8a.apk).
function apkPath(path: string, abi: string): string {
  return path.endsWith(".apk") ? `${path.slice(0, -".apk".length)}-${abi}.apk` : `${path}-${abi}.apk`
}

// The app being packed and where its outputs go, reported first by every
// solidrt pack.
//
// One output rule: every deliverable defaults into the gitignored dist/
// build root (okf/backlog/build-output-dirs.md) - files in the root named
// by the appId's last segment, flow dirs (pack/, render/, bundle/) below
// it - never next to the sources. --output overrides.
function packTarget() {
  let mode = resolveMode()
  let identity = loadAppIdentity(mode.entry, mode.projectDir)
  console.log(`>> app: ${identity.appId} (${identity.org} / ${identity.displayName})`)
  if (identity.defaulted) {
    console.warn('>> warning: no "solidrt.appId" in package.json; set a stable reverse-DNS id before distributing')
  }
  let distRoot = join(mode.projectDir ?? dirname(mode.entry), "dist")
  let baseName = identity.appId.split(".").pop()!
  return { mode, identity, distRoot, baseName }
}

// All solidrt outputs are the same canonical pack: manifest + bundle.fluxbc +
// assets (fonts included). --folder writes it as a flat folder next to a
// bare runner; --app writes it alone as one .solapp for a runner to load;
// --apk patches it into APKs; the default single-file exe carries it as
// trailer sections.
async function buildApp(mode: Mode) {
  let fonts = resolvePackFonts(mode.projectDir)
  console.log(`>> fonts: ${fonts.length ? fonts.map((f) => f.alias).join(", ") : "none"}`)
  let bundled = await bundleSolid(mode)
  let bytecode = await compileToBytecode(bundled.code)
  let isolates = []
  for (let i of bundled.isolates) isolates.push({ id: i.id, bytecode: await compileToBytecode(i.code, i.id) })
  if (isolates.length) console.log(`>> isolates: ${isolates.map((i) => i.id).join(", ")}`)
  return { bytecode, folder: buildPackFolder(mode, bytecode, isolates) }
}

// --apk patches the app into one installable Android APK per installed
// target (android-targets.ts; the picker runs when there is none):
// application id and label rewritten, permissions declared, the .solapp
// payload added as a stored asset, re-aligned and re-signed - pure
// TypeScript, no Android SDK (okf/done/standalone-android-apk.md). A
// target's base is its runner APK, which boots the payload. Every
// @solidrt/android-<abi> package carries one; a checkout has one where `make
// android-runtime` staged it, and a target without one is skipped with a
// note. Returns the APK written per ABI.
export async function packApks(): Promise<Map<string, string>> {
  let { mode, identity, distRoot, baseName } = packTarget()
  if (!ANDROID_APP_ID.test(identity.appId)) {
    console.error(
      `"solidrt": "appId" ("${identity.appId}") is not a valid Android application id: use reverse-DNS with at least two dot-separated segments, each starting with a letter (e.g. "com.example.app")`,
    )
    process.exit(1)
  }
  let targets: string[] = []
  for (let abi of await ensureTargets([], mode.projectDir)) {
    if (resolveRunnerApk(abi)) targets.push(abi)
    else console.log(`>> note: no ${abi} runner APK; skipped (in a checkout: make android-runtime ANDROID_ABI=${abi})`)
  }
  if (targets.length === 0) {
    console.error(`Could not find a runner APK; add the ${ANDROID_PKG_MAP[DEFAULT_ANDROID_ABI]} dev dependency, or in a checkout run make android-runtime`)
    process.exit(1)
  }

  let { bytecode, folder } = await buildApp(mode)
  let project = loadProject(mode.projectDir)
  let config = project?.config ?? {}
  let android = config.android ?? {}
  let icon = resolveLauncherIcon(project)
  let permissions = [
    ...resolveCapabilities(config).map((name) => ANDROID_CAPABILITY_PERMISSIONS[name]),
    ...(android.permissions ?? []),
  ]
  let backup = config.backup ?? false
  let payload = packApp(folder, bytecode)
  console.log(`>> icon: ${icon ? "from project" : "placeholder"}`)
  console.log(`>> permissions: ${permissions.length ? permissions.join(", ") : "none"}`)
  console.log(`>> backup: ${backup ? "data/ folder" : "off"}`)
  console.log(">> signed with the shared development key (fine for sideloading; distribution signing pending)")

  let written = new Map<string, string>()
  for (let abi of targets) {
    let base = resolveRunnerApk(abi)!
    console.log(`>> base (${abi}): ${base}`)
    let apk: Buffer
    try {
      apk = patchApk(readFileSync(base), {
        appId: identity.appId,
        label: identity.displayName,
        payload,
        versionCode: android.versionCode ?? DEFAULT_VERSION_CODE,
        versionName: project?.version ?? DEFAULT_VERSION_NAME,
        icon,
        iconBackground: config.iconBackground ?? DEFAULT_ICON_BACKGROUND,
        permissions,
        backup,
      })
    } catch (e) {
      console.error(`Could not patch ${base}: ${e instanceof Error ? e.message : e}`)
      process.exit(1)
    }
    let outfile = apkPath(values.output ?? join(distRoot, baseName + ".apk"), abi)
    await Bun.write(outfile, apk)
    console.log(`>> wrote ${apk.length} bytes to ${outfile}`)
    written.set(abi, outfile)
  }
  return written
}

export async function main() {
  if (values.flux) {
    if (values.folder || values.app || values.apk) {
      console.error("--folder, --app and --apk are for app packs; flux scripts have no folder, .solapp or APK output")
      process.exit(1)
    }
    let outfile = exeName(values.output ?? join(dirname(resolve(source!)), "dist", basename(source!).replace(/\.[jt]s$/, "")))
    // The entry's isolate modules ride along as isolates/<id>.fluxbc sections
    // (module name = id, for stack attribution).
    let isolates = []
    for (let module of findFluxIsolates(dirname(resolve(source!)))) {
      isolates.push({ id: module.id, bytecode: await compileToBytecode(await bundleFlux(module.path), module.id) })
    }
    if (isolates.length) console.log(`>> isolates: ${isolates.map((i) => i.id).join(", ")}`)
    await writeExecutable(packFlux(await compileToBytecode(await bundleFlux(source!)), isolates), outfile)
    process.exit()
  }

  if (values.apk) {
    await packApks()
    process.exit()
  }

  let { mode, distRoot, baseName } = packTarget()
  let { bytecode, folder } = await buildApp(mode)

  if (values.folder) {
    let outDir = values.output ?? join(distRoot, "pack")
    writePackFolder(outDir, requireBinary("solidrt"), bytecode, folder)
    console.log(`>> wrote pack folder to ${resolve(outDir)}`)
    process.exit()
  }

  if (values.app) {
    let outfile = values.output ?? join(distRoot, baseName + ".solapp")
    let packed = packApp(folder, bytecode)
    await Bun.write(outfile, packed)
    console.log(`>> wrote ${packed.length} bytes to ${outfile}`)
    process.exit()
  }

  let outfile = exeName(values.output ?? join(distRoot, baseName))
  await writeExecutable(packSolid(folder, bytecode), outfile)
  process.exit()
}
