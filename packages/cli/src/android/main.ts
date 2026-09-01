import { existsSync, readFileSync } from "node:fs"
import { networkInterfaces } from "node:os"
import { resolve } from "node:path"
import { androidPackageVersion, resolveApk, ANDROID_PKG_MAP } from "../lib/artifacts"
import { values, port, source } from "../lib/args"
import { devDir } from "../lib/dev-dir"
import { CLI_VERSION } from "../lib/project"
import { confirm, multiselect } from "../lib/prompt"
import { apkApplicationId } from "../pack/android/apk"
import { installPlatformTools, platformToolsAvailable } from "./platform-tools"
import { resolveByPort, resolveFromCwd } from "../lib/registry"
import type { LiveRecord } from "../types/registry"

// `srt android`: the Android client flow, decoupled from `srt client` (a
// local process) because a device is a different thing: find it over adb and
// launch the client installed there pointed at the dev server, which is
// resolved like `srt client` does (the project at the cwd, or --port). The
// APK is only touched on --install (from the @solidrt/android-<abi> package
// matching the device's ABI), so a client built and installed by hand stays;
// without --install the command just notes when the installed version is not
// the one the project's package carries.

// Launch component of the "go" dev-client flavor (see lattice/Makefile.android).
let PACKAGE_ACTIVITY = "com.solidrt.go/com.solidrt.app.MainActivity"
let PACKAGE = "com.solidrt.go"

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
// terminal; the default is yes because typing `srt android` already states
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
// directory, or none: the client then starts on its own, into the launcher.
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
// to pick the matching APK -- a fat (multi-ABI) APK reports its 64-bit ABI
// here and still installs fine, so this only matters for single-ABI builds.
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
  for (let d of devices) console.log(`${d} - ${abiByDevice.get(d)}`)
}

// A published CLI version (x.y.z): the release action publishes the CLI and
// the android packages at one version, so that is the one to pin. A checkout
// reports a git describe (or the 0.0.0 placeholder), which npm does not have.
let RELEASE_VERSION = /^\d+\.\d+\.\d+$/

// The APK for `abi`, adding the project's @solidrt/android-<abi> dev
// dependency in the cwd first when it is not installed. ABIs without a
// published package (e.g. x86) only resolve through SRT_HOME.
function ensureApk(abi: string): string {
  let apk = resolveApk(abi)
  if (apk) return apk
  let pkg = ANDROID_PKG_MAP[abi]
  if (!pkg) {
    console.error(`Could not find a SolidRT-Go APK for ABI "${abi}".`)
    process.exit(1)
  }
  let spec = RELEASE_VERSION.test(CLI_VERSION) && CLI_VERSION !== "0.0.0" ? `${pkg}@${CLI_VERSION}` : pkg
  console.log(`[cli] Adding dev dependency ${spec}`)
  let add = Bun.spawnSync(["bun", "add", "-d", spec], { cwd: process.cwd(), stdout: "inherit", stderr: "inherit" })
  if (add.exitCode !== 0) {
    console.error(`Could not add ${spec}; retry with bun add -d ${spec}`)
    process.exit(1)
  }
  apk = resolveApk(abi)
  if (apk) return apk
  console.error(`${pkg} is installed but carries no solidrt-go.apk for ABI "${abi}".`)
  process.exit(1)
}

// The versionName of the client installed on `target`, null when none is.
function installedVersion(adb: string, target: string): string | null {
  let res = Bun.spawnSync([adb, "-s", target, "shell", "dumpsys", "package", PACKAGE], { stdout: "pipe", stderr: "pipe" })
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

// Install the client on `target` (on --install), else check that one is there
// and note when its version is not the one the project's package carries.
async function prepare(adb: string, { target, abi }: Device) {
  if (values.install) {
    let apk = ensureApk(abi)
    console.log(`[cli] Installing SolidRT-Go on ${target}`)
    let install = Bun.spawn([adb, "-s", target, "install", "-r", apk], { stdout: "pipe", stderr: "pipe" })
    if ((await install.exited) !== 0) {
      console.error("adb install failed:\n" + (await new Response(install.stderr).text()))
      process.exit(1)
    }
    return
  }
  let installed = installedVersion(adb, target)
  if (installed === null) {
    console.error(`No SolidRT-Go client on ${target}; install one with srt android --install`)
    process.exit(1)
  }
  let expected = androidPackageVersion(abi)
  if (expected !== null && expected !== installed) {
    console.log(
      `[cli] Installed client is ${installed}; the project's ${ANDROID_PKG_MAP[abi]} is ${expected} (srt android --install updates it)`,
    )
  }
}

// Launch the client on `target`, handing it the dev-server address to dial as
// a launch-intent extra that MainActivity forwards to native argv
// (--dev-server); the client auto-connects to it. Replaces adb reverse, which
// never worked over wireless adb. -S stops a running instance first: a
// delivered intent does not reach one, so without it the client would keep
// whatever server it had. With no server there is no extra to pass.
async function launch(adb: string, { target }: Device, server: LiveRecord | null) {
  let devServer = server ? devServerAddress(adb, target, server) : null
  let launchArgs = [adb, "-s", target, "shell", "am", "start", "-S", "-n", PACKAGE_ACTIVITY]
  if (devServer) {
    console.log(`[cli] Client on ${target} will dial dev server at ${devServer}`)
    launchArgs.push("--es", "srt_dev_server", devServer)
  } else if (server) {
    console.log(`[cli] Could not resolve a host address for ${target}; client will need a manual/QR connect`)
  }
  let start = Bun.spawn(launchArgs, { stdout: "pipe", stderr: "pipe" })
  if ((await start.exited) !== 0) {
    console.error("adb start failed:\n" + (await new Response(start.stderr).text()))
    process.exit(1)
  }
  console.log(`[cli] Launched SolidRT-Go on ${target}`)
}

// The launcher activity every SolidRT base APK ships, stored fully qualified
// in the manifest so pack's application-id rewrite never touches it.
let PACKED_ACTIVITY = "com.solidrt.app.MainActivity"

// Install a packed APK (srt pack --apk) on the connected devices and launch
// it. The application id comes out of the APK's own manifest, where pack
// wrote it. Nothing dev-flavored applies: a packed app carries its payload
// and never dials the dev server.
async function installPackedApk(path: string) {
  let file = resolve(path)
  if (!existsSync(file)) {
    console.error(`No such file: ${path}`)
    process.exit(1)
  }
  let appId: string
  try {
    appId = apkApplicationId(readFileSync(file))
  } catch (e) {
    console.error(`Could not read ${path} as an APK: ${e instanceof Error ? e.message : e}`)
    process.exit(1)
  }
  let adb = await requireAdb()
  for (let { target } of await resolveTargets(adb)) {
    console.log(`[cli] Installing ${appId} on ${target}`)
    let install = Bun.spawn([adb, "-s", target, "install", "-r", file], { stdout: "pipe", stderr: "pipe" })
    if ((await install.exited) !== 0) {
      console.error("adb install failed:\n" + (await new Response(install.stderr).text()))
      process.exit(1)
    }
    let start = Bun.spawn([adb, "-s", target, "shell", "am", "start", "-S", "-n", `${appId}/${PACKED_ACTIVITY}`], {
      stdout: "pipe",
      stderr: "pipe",
    })
    if ((await start.exited) !== 0) {
      console.error("adb start failed:\n" + (await new Response(start.stderr).text()))
      process.exit(1)
    }
    console.log(`[cli] Launched ${appId} on ${target}`)
  }
}

// Launch the Android client on the connected devices over adb (installing it
// first on --install), then wait briefly for them to show up on the dev
// server. The clients are not child processes here: their lifecycle is the
// WS connect/disconnect the server sees. With an APK argument, install and
// launch that packed app instead.
export async function main() {
  if (source) return installPackedApk(source)
  let server = await resolveServer()
  let adb = await requireAdb()

  let devices = await resolveTargets(adb)
  for (let device of devices) await prepare(adb, device)

  let before = new Set(server ? (await connectedClients(server)).map((c) => c.id) : [])
  for (let device of devices) await launch(adb, device, server)
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
