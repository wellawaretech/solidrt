import { ANDROID_PKG_MAP, DEFAULT_ANDROID_ABI, resolveApk, resolveRunnerApk } from "./artifacts"
import { CLI_VERSION } from "./project"
import { abort, confirm, multiselect, spinner } from "./prompt"
import { runQuiet } from "./util"

// Android targets: the ABIs a project runs and ships on. Each is an
// @solidrt/android-<abi> dev dependency carrying that ABI's Player (what
// `srt android` installs) and runner (the base `srt pack --apk` patches), so
// the project's dependencies are the record of what it targets; a
// checkout's staged builds (SRT_HOME) count too.

// What each target means in devices, for the picker.
const TARGET_HINTS: Record<string, string> = {
  "arm64-v8a": "64-bit ARM: current phones and tablets",
  x86_64: "emulators, Chromebooks",
  "armeabi-v7a": "32-bit ARM: older phones, many Android TVs",
}

// A published CLI version (x.y.z): the release action publishes the CLI and
// the android packages at one version, so that is the one to pin. A checkout
// reports a git describe (or the 0.0.0 placeholder), which npm does not have.
let RELEASE_VERSION = /^\d+\.\d+\.\d+$/

function packageSpec(abi: string): string {
  let pkg = ANDROID_PKG_MAP[abi]!
  return RELEASE_VERSION.test(CLI_VERSION) && CLI_VERSION !== "0.0.0" ? `${pkg}@${CLI_VERSION}` : pkg
}

// The installed targets: the ABIs whose runner or Player resolves.
export function installedTargets(): string[] {
  return Object.keys(ANDROID_PKG_MAP).filter((abi) => resolveRunnerApk(abi) || resolveApk(abi))
}

// Add the targets' packages as dev dependencies of the project in `dir`.
async function addTargets(abis: string[], dir: string) {
  let specs = abis.map(packageSpec)
  let progress = spinner()
  progress.start(`Adding ${specs.join(", ")}`)
  let add = await runQuiet(["bun", "add", "-d", ...specs], dir)
  if (add.code !== 0) {
    progress.error(`Could not add ${specs.join(", ")}`)
    process.stderr.write(add.output)
    abort(`Retry with bun add -d ${specs.join(" ")}`)
  }
  progress.stop(`Added ${specs.join(", ")}`)
}

// Make sure the project targets every ABI in `needed` (the connected
// devices'), and return the installed targets. A project with none gets the
// picker, arm64-v8a and `needed` preselected; one lacking some of `needed`
// is asked once to add them. A non-TTY takes the defaults. The packages are
// added to the project in `dir`; without one (a file packed on its own)
// nothing can be.
export async function ensureTargets(needed: string[], dir: string | null): Promise<string[]> {
  let installed = installedTargets()
  let missing = [...new Set(needed)].filter((abi) => ANDROID_PKG_MAP[abi] && !installed.includes(abi))
  if (installed.length > 0 && missing.length === 0) return installed
  if (!dir) abort("No Android target installed; add one to a project with bun add -d @solidrt/android-<abi>")

  let add: string[] = missing
  if (installed.length === 0) {
    let preset = new Set([DEFAULT_ANDROID_ABI, ...missing])
    add = await multiselect(
      "Which Android devices do you target?",
      Object.keys(ANDROID_PKG_MAP).map((abi) => ({ label: abi, value: abi, hint: TARGET_HINTS[abi], checked: preset.has(abi) })),
    )
    if (add.length === 0) abort("No Android target picked")
  } else {
    let s = missing.length > 1 ? "s" : ""
    let pkgs = missing.map((abi) => ANDROID_PKG_MAP[abi]).join(", ")
    if (!(await confirm(`Add ${pkgs} for the connected ${missing.join(", ")} device${s}?`))) return installed
  }
  await addTargets(add, dir)
  return installedTargets()
}
