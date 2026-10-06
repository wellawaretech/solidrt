import { existsSync, readFileSync } from "node:fs"
import { networkInterfaces } from "node:os"
import { resolve } from "node:path"
import { resolveApk, ANDROID_PKG_MAP } from "../lib/artifacts"
import { ensureTargets } from "../lib/android-targets"
import { values, port, source } from "../lib/args"
import { devDir } from "../lib/dev-dir"
import { confirm, multiselect } from "../lib/prompt"
import { runQuiet } from "../lib/util"
import { apkApplicationId, apkVersionName } from "../pack/android/apk"
import { census } from "./census"
import { installPlatformTools, platformToolsAvailable } from "./platform-tools"
import { resolveByPort, resolveFromCwd } from "../lib/registry"
import type { LiveRecord } from "../types/registry"

// `sol android`: the Android client flow, decoupled from `sol client` (a
// local process) because a device is a different thing: find it over adb and
// launch the client installed there pointed at the dev server, which is
// resolved like `sol client` does (the project at the cwd, or --port). A
// device without the client gets it installed, from the project's
// @solidrt/android-<abi> target for the device's ABI (android-targets.ts,
// which asks for the targets the first time); an installed client whose
// version is not the one that package carries is updated after asking, so a
// client built and installed by hand stays unless you say so. --install
// reinstalls regardless. --apk runs `sol pack --apk` and installs the
// packed app instead.

// The published Player's application id. Every build from a checkout
// without PUBLISH=1 is the dev Player, com.solidrt.player.dev, installed
// beside it, so the Player a device gets is the one its APK names
// (playerId).
let PUBLISHED_PLAYER = "com.solidrt.player"

// The launcher activity every SolidRT APK ships, the Player's and every
// packed app's, stored fully qualified in the manifest so neither an
// application-id change nor pack's rewrite touches it.
let MAIN_ACTIVITY = "com.solidrt.app.MainActivity"

let sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms))

// adb is a system tool (Android Platform Tools), never bundled. Look on PATH
// first, then the standard SDK locations, then the managed download
// (platform-tools.ts) - so a system install always wins over ours.
function resolveAdb() {
  let exe = process.platform === "win32" ? "adb.exe" : "adb"
  let onPath = Bun.which(exe)
  if (onPath) return onPath
  for (let root of [process.env.ANDROID_HOME, process.env.ANDROID_SDK_ROOT]) {
    if (!root) continue
    let candidate = resolve(root, "platform-tools", exe)
    if (existsSync(candidate)) return candidate
  }
  let managed = devDir("platform-tools", exe)
  if (existsSync(managed)) return managed
  return null
}

// Resolve adb, offering to download platform-tools when it is missing (on a
// terminal; the default is yes because typing `sol android` already states
// the intent). Non-interactive runs keep the print-and-exit behavior: CI
// should install adb itself, and gets told how.
async function requireAdb(): Promise<string> {
  let path = resolveAdb()
  if (path) return path
  console.error("Could not find adb (Android Platform Tools).")
  if (process.stdin.isTTY && platformToolsAvailable() && (await confirm("Download platform-tools (~15 MB) into ~/.solidrt?"))) {
    if (await installPlatformTools()) {
      path = resolveAdb()
      if (path) return path
      console.error("The downloaded platform-tools carry no adb; remove ~/.solidrt/platform-tools and retry.")
    }
    process.exit(1)
  }
  let install =
    process.platform === "win32"
      ? "winget install Google.PlatformTools"
      : process.platform === "darwin"
        ? "brew install android-platform-tools"
        : "install your distro's android-tools / adb package"
  console.error(`Install it: ${install}`)
  process.exit(1)
}

let ipToInt = (ip: string) => ip.split(".").reduce((acc, o) => (acc << 8) + (parseInt(o, 10) & 255), 0) >>> 0

// This host's IPv4 on the same subnet as `deviceIp`: the interface whose
// (address & netmask) matches the device's. Pure computation over what the OS
// reports (cross-platform, no routing socket, no hardcoded ranges).
function hostIpFor(deviceIp: string): string | null {
  let d = ipToInt(deviceIp)
  for (let addrs of Object.values(networkInterfaces())) {
    for (let a of addrs ?? []) {
      if (a.family === "IPv4" && !a.internal && (ipToInt(a.address) & ipToInt(a.netmask)) === (d & ipToInt(a.netmask))) {
        return a.address
      }
    }
  }
  return null
}

// The device's own IPv4 on its primary network: from the serial for wireless adb
// (ip:port), else queried over adb. `ip route get` reports the source IP of the
// route to a public address, i.e. the device's main interface IP.
function deviceIp(adb: string, target: string): string | null {
  let m = target.match(/^(\d+\.\d+\.\d+\.\d+):\d+$/)
  if (m) return m[1] ?? null
  let res = Bun.spawnSync([adb, "-s", target, "shell", "ip", "route", "get", "1.1.1.1"], {
    stdout: "pipe",
    stderr: "pipe",
  })
  return res.stdout.toString().match(/src (\d+\.\d+\.\d+\.\d+)/)?.[1] ?? null
}

// The host:port the client on `target` should dial to reach this machine's dev
// server, by adb transport: the emulator reaches the host through its NAT alias
// 10.0.2.2; every other transport shares a LAN with the host, so match the
// device's IP to the host interface on its subnet. Replaces the old adb-reverse
// loopback tunnel, which never worked over wireless adb. Returns null when the
// address cannot be resolved (the client then falls back to QR/recents).
function devServerAddress(adb: string, target: string, server: LiveRecord): string | null {
  // The emulator's host alias reaches the host's loopback, so a loopback-only
  // server is fine there; a real device needs a server started with --lan.
  if (target.startsWith("emulator-")) return `10.0.2.2:${server.port}`
  if (server.address === "127.0.0.1") {
    console.log("[cli] The dev server is loopback-only; restart it with --lan so the device can reach it")
    return null
  }
  let dip = deviceIp(adb, target)
  if (!dip) return null
  let host = hostIpFor(dip)
  return host ? `${host}:${server.port}` : null
}

// The dev server the device should dial: --port picks a local server by
// port (and must exist), otherwise the project (or file) in the current
// directory, or none: the client then starts on its own, into the player.
async function resolveServer(): Promise<LiveRecord | null> {
  if (port !== undefined) {
    let resolved = await resolveByPort(port)
    if (!resolved.ok) {
      console.error(resolved.message)
      process.exit(1)
    }
    return resolved.record
  }
  let resolved = await resolveFromCwd(process.cwd())
  return resolved.ok ? resolved.record : null
}

type AdbDeviceRow = { serial: string; state: string }

// Every row `adb devices` reports, with its state: "device" (usable),
// "unauthorized" (RSA dialog not accepted), "no permissions ..." (Linux udev
// rules missing), "offline". Callers filter; the states drive the no-device
// triage below.
function listDevices(adb: string): AdbDeviceRow[] {
  let listed = Bun.spawnSync([adb, "devices"], { stdout: "pipe", stderr: "pipe" })
  return listed.stdout
    .toString()
    .split("\n")
    .slice(1)
    .map((l) => l.trim())
    .filter(Boolean)
    .map((l) => {
      let [serial, ...state] = l.split("\t")
      return { serial: serial ?? "", state: state.join("\t") }
    })
    .filter((r) => r.serial)
}

// The "no usable device" cases look different in `adb devices` and have
// different fixes; name the one that applies instead of a catch-all line.
function reportNoDevice(rows: AdbDeviceRow[]) {
  let unauthorized = rows.find((r) => r.state === "unauthorized")
  if (unauthorized) {
    console.error(`Device ${unauthorized.serial} is unauthorized: unlock it and accept the USB debugging dialog.`)
    return
  }
  let noPerms = rows.find((r) => r.state.startsWith("no permissions"))
  if (noPerms) {
    console.error(
      `Device ${noPerms.serial} is visible but not accessible: install your distro's adb udev rules ` +
        `(android-udev on Arch, android-sdk-platform-tools-common on Debian/Ubuntu), then replug it.`,
    )
    return
  }
  console.error("No Android device found. Enable USB debugging (Developer options) and check the cable (charge-only cables exist).")
  if (process.platform === "win32") {
    console.error("On Windows a missing USB driver also hides the device; install the vendor's (or Google's) USB driver.")
  }
}

// Primary ABI of the connected device (e.g. "arm64-v8a", "armeabi-v7a"), used
// to pick the matching APK: every Player and runner is built for one ABI, so
// only that ABI's APK installs.
function deviceAbi(adb: string, target: string): string {
  let res = Bun.spawnSync([adb, "-s", target, "shell", "getprop", "ro.product.cpu.abi"], {
    stdout: "pipe",
    stderr: "pipe",
  })
  return res.stdout.toString().trim()
}

// One line per connected device with the ABI its APK build would target;
// the picker shows the same lines, so this is for the cases without one.
function printDeviceStatus(devices: string[], abiByDevice: Map<string, string>) {
  // Under --json the census summary owns stdout, so the status goes beside it.
  let print = values.json ? console.error : console.log
  for (let d of devices) print(`${d} - ${abiByDevice.get(d)}`)
}

// The version placeholder an unversioned build carries, never a client's
// real version, so there is nothing to compare against.
let UNRELEASED_VERSION = "0.0.0"

// The versionName of the client installed on `target`, null when none is.
function installedVersion(adb: string, target: string, player: string): string | null {
  let res = Bun.spawnSync([adb, "-s", target, "shell", "dumpsys", "package", player], { stdout: "pipe", stderr: "pipe" })
  return res.stdout.toString().match(/versionName=(\S+)/)?.[1] ?? null
}

type Client = { id: number; platform: string; version: string }

// The clients the dev server currently lists (empty when it cannot be asked).
async function connectedClients(server: LiveRecord): Promise<Client[]> {
  try {
    let resp = await fetch(`http://127.0.0.1:${server.port}/__control__/clients`, { signal: AbortSignal.timeout(1000) })
    return ((await resp.json()) as { clients?: Client[] }).clients ?? []
  } catch {
    return []
  }
}

// The Android clients that appear beyond `before`: all of them once `count`
// have, else whatever showed up within ~10 s.
async function waitForClients(server: LiveRecord, before: Set<number>, count: number): Promise<Client[]> {
  let fresh: Client[] = []
  for (let attempt = 0; attempt < 20 && fresh.length < count; attempt++) {
    await sleep(500)
    fresh = (await connectedClients(server)).filter((c) => c.platform === "android" && !before.has(c.id))
  }
  return fresh
}

type Device = { target: string; abi: string }

// The install failures only an uninstall clears, by the reason each names:
// the installed app is signed with another key (the published Player over
// a local build or the other way round, or an older release's key) or is a
// newer version (a project pinned to an older release).
const BLOCKED_INSTALLS: Record<string, string> = {
  INSTALL_FAILED_UPDATE_INCOMPATIBLE: "is signed with another key",
  INSTALL_FAILED_VERSION_DOWNGRADE: "is a newer version",
}

// Install `file` on `target`, exiting on failure. When the installed
// `packageId` blocks it, offer to uninstall that first: it clears the app's
// data, so it is asked, and a non-TTY only says how.
async function adbInstall(adb: string, target: string, file: string, packageId: string) {
  let install = await runQuiet([adb, "-s", target, "install", "-r", file], process.cwd())
  if (install.code === 0) return
  let blocked = Object.keys(BLOCKED_INSTALLS).find((code) => install.output.includes(code))
  if (blocked) {
    let reason = `${packageId} on ${target} ${BLOCKED_INSTALLS[blocked]}`
    if (!(await confirm(`${reason}. Uninstall it first? This clears its data.`, false))) {
      console.error(`${reason}; uninstall it first with adb -s ${target} uninstall ${packageId} (this clears its data)`)
      process.exit(1)
    }
    let uninstall = await runQuiet([adb, "-s", target, "uninstall", packageId], process.cwd())
    if (uninstall.code !== 0) {
      console.error("adb uninstall failed:\n" + uninstall.output)
      process.exit(1)
    }
    install = await runQuiet([adb, "-s", target, "install", file], process.cwd())
    if (install.code === 0) return
  }
  console.error("adb install failed:\n" + install.output)
  process.exit(1)
}

// Resolve the target devices (serial and ABI). With --device, treat the
// value as a serial prefix and require it to match exactly one connected
// device; without it, use the sole connected device, or pick any number of
// several on a terminal (all preselected: enter means every device). Exits
// with a clear message on any ambiguity.
async function resolveTargets(adb: string): Promise<Device[]> {
  let rows = listDevices(adb)
  let devices = rows.filter((r) => r.state === "device").map((r) => r.serial)
  let abiByDevice = new Map(devices.map((d) => [d, deviceAbi(adb, d)]))
  let device = (target: string): Device => ({ target, abi: abiByDevice.get(target)! })

  if (values.device) {
    printDeviceStatus(devices, abiByDevice)
    let prefix = values.device
    let matches = devices.filter((d) => d.startsWith(prefix))
    if (matches.length > 1) {
      console.error(`--device "${prefix}" is ambiguous; matches: ${matches.join(", ")}`)
      process.exit(1)
    }
    let [match] = matches
    if (!match) {
      console.error(`No connected device matches --device "${prefix}".`)
      process.exit(1)
    }
    return [device(match)]
  }

  if (devices.length > 1) {
    // The prompt's non-TTY default is no way to pick devices, so a script has
    // to say which.
    if (!process.stdin.isTTY) {
      printDeviceStatus(devices, abiByDevice)
      console.error("Pick one with --device <serial or prefix>.")
      process.exit(1)
    }
    let picked = await multiselect(
      "Pick devices",
      devices.map((d) => ({ label: `${d} - ${abiByDevice.get(d)}`, value: d, checked: true })),
    )
    if (picked.length === 0) {
      console.error("No device picked.")
      process.exit(1)
    }
    return picked.map(device)
  }
  printDeviceStatus(devices, abiByDevice)
  let [only] = devices
  if (!only) {
    reportNoDevice(rows)
    process.exit(1)
  }
  return [device(only)]
}

// The Player for an `abi` device: the application id its Player APK (the
// project's package, or a checkout's staged build) names in its manifest,
// else the published Player.
function playerId(abi: string): string {
  let apk = resolveApk(abi)
  return apk ? apkAppId(apk) : PUBLISHED_PLAYER
}

// Put the Player on `target` when it needs one: none installed yet, or
// --install. An installed Player whose version is not the one the project's
// APK carries (its package's, or its own runtime's, which a rebuild
// re-versions) is updated after asking. Returns the Player's id, the one
// to launch.
async function prepare(adb: string, { target, abi }: Device): Promise<string> {
  let apk = resolveApk(abi)
  if (!apk) {
    console.error(`Could not find a Player APK for ${target} (ABI "${abi}").`)
    process.exit(1)
  }
  let player = apkAppId(apk)
  let installed = installedVersion(adb, target, player)
  let expected = apkVersion(apk)
  let update = values.install || installed === null
  if (!update && expected !== UNRELEASED_VERSION && expected !== installed) {
    update = await confirm(`${player} on ${target} is ${installed}; the project's Player APK is ${expected}. Update it?`)
  }
  if (!update) return player
  console.log(`[cli] Installing ${player} on ${target}`)
  await adbInstall(adb, target, apk, player)
  return player
}

// Launch `player` on `target`, handing it the dev-server address to dial as
// a launch-intent extra that MainActivity forwards to native argv
// (--dev-server); the client auto-connects to it. Replaces adb reverse, which
// never worked over wireless adb. -S stops a running instance first: a
// delivered intent does not reach one, so without it the client would keep
// whatever server it had. With no server there is no extra to pass.
async function launch(adb: string, { target }: Device, player: string, server: LiveRecord | null) {
  let devServer = server ? devServerAddress(adb, target, server) : null
  let launchArgs = [adb, "-s", target, "shell", "am", "start", "-S", "-n", `${player}/${MAIN_ACTIVITY}`]
  if (devServer) {
    console.log(`[cli] Client on ${target} will dial dev server at ${devServer}`)
    launchArgs.push("--es", "sol_dev_server", devServer)
  } else if (server) {
    console.log(`[cli] Could not resolve a host address for ${target}; client will need a manual/QR connect`)
  }
  let start = Bun.spawn(launchArgs, { stdout: "pipe", stderr: "pipe" })
  if ((await start.exited) !== 0) {
    console.error("adb start failed:\n" + (await new Response(start.stderr).text()))
    process.exit(1)
  }
  console.log(`[cli] Launched ${player} on ${target}`)
}

// The application id an APK names in its own manifest (for a packed app,
// where pack wrote it).
function apkAppId(path: string): string {
  try {
    return apkApplicationId(readFileSync(path))
  } catch (e) {
    console.error(`Could not read ${path} as an APK: ${e instanceof Error ? e.message : e}`)
    process.exit(1)
  }
}

// The versionName of the APK at `path`, exiting when it is not an APK.
function apkVersion(path: string): string {
  try {
    return apkVersionName(readFileSync(path))
  } catch (e) {
    console.error(`Could not read ${path} as an APK: ${e instanceof Error ? e.message : e}`)
    process.exit(1)
  }
}

// Install a packed APK (sol pack --apk) on `target` and launch it. Nothing
// dev-flavored applies: a packed app carries its payload and never dials the
// dev server.
async function installPacked(adb: string, target: string, file: string, appId: string) {
  console.log(`[cli] Installing ${appId} on ${target}`)
  await adbInstall(adb, target, file, appId)
  let start = Bun.spawn([adb, "-s", target, "shell", "am", "start", "-S", "-n", `${appId}/${MAIN_ACTIVITY}`], {
    stdout: "pipe",
    stderr: "pipe",
  })
  if ((await start.exited) !== 0) {
    console.error("adb start failed:\n" + (await new Response(start.stderr).text()))
    process.exit(1)
  }
  console.log(`[cli] Launched ${appId} on ${target}`)
}

// `sol android <file.apk>`: install and launch that APK on the connected
// devices.
async function installApkFile(path: string) {
  let file = resolve(path)
  if (!existsSync(file)) {
    console.error(`No such file: ${path}`)
    process.exit(1)
  }
  let appId = apkAppId(file)
  let adb = await requireAdb()
  for (let { target } of await resolveTargets(adb)) await installPacked(adb, target, file, appId)
}

// `sol android --apk`: pack the app for every installed target (sol pack
// --apk), then install and launch on each device the APK built for its ABI.
// The devices are resolved first, so their ABIs are among the targets. Pack
// loads the bundler, so it is imported only on this path.
async function packAndInstall() {
  let adb = await requireAdb()
  let devices = await resolveTargets(adb)
  await ensureTargets(devices.map((d) => d.abi), process.cwd())
  let { packApks } = await import("../pack/main")
  let apks = await packApks()
  for (let { target, abi } of devices) {
    let file = apks.get(abi)
    if (!file) {
      console.error(`No APK for ${target}: no ${abi} runner (add ${ANDROID_PKG_MAP[abi] ?? "a build for it"}, or in a checkout run make android-runtime ANDROID_ABI=${abi})`)
      process.exit(1)
    }
    await installPacked(adb, target, file, apkAppId(file))
  }
}

// Launch the Android client on the connected devices over adb (installing it
// first where it is missing), then wait briefly for them to show up on the
// dev server. The clients are not child processes here: their lifecycle is
// the WS connect/disconnect the server sees. With an APK argument or --apk,
// install and launch a packed app instead.
export async function main() {
  if (source) return installApkFile(source)
  if (values.apk) return packAndInstall()
  if (values.census) {
    let adb = await requireAdb()
    for (let { target, abi } of await resolveTargets(adb)) await census(adb, target, playerId(abi))
    return
  }
  let server = await resolveServer()
  let adb = await requireAdb()

  // Only a device that gets the Player installed needs a target for its ABI.
  let devices = await resolveTargets(adb)
  let installing = devices.filter((d) => values.install || installedVersion(adb, d.target, playerId(d.abi)) === null)
  if (installing.length) await ensureTargets(installing.map((d) => d.abi), process.cwd())
  let players = new Map<string, string>()
  for (let device of devices) players.set(device.target, await prepare(adb, device))

  let before = new Set(server ? (await connectedClients(server)).map((c) => c.id) : [])
  for (let device of devices) await launch(adb, device, players.get(device.target)!, server)
  if (!server) return

  console.log("[cli] Waiting for the client(s) to connect to the dev server...")
  let clients = await waitForClients(server, before, devices.length)
  for (let client of clients) {
    console.log(`[cli] Client ${client.id} connected (${client.platform}, ${client.version})`)
  }
  if (clients.length < devices.length) {
    console.log(
      `[cli] ${devices.length - clients.length} of ${devices.length} not connected after 10 s. The server must run with --lan and the device must reach this machine.`,
    )
  }
}
