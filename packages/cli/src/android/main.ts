import { existsSync } from "node:fs"
import { networkInterfaces } from "node:os"
import { resolve } from "node:path"
import { androidPackageVersion, resolveApk, ANDROID_PKG_MAP } from "../lib/artifacts"
import { values, port } from "../lib/args"
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
// first, then the standard SDK location.
function resolveAdb() {
  let exe = process.platform === "win32" ? "adb.exe" : "adb"
  let onPath = Bun.which(exe)
  if (onPath) return onPath
  for (let root of [process.env.ANDROID_HOME, process.env.ANDROID_SDK_ROOT]) {
    if (!root) continue
    let candidate = resolve(root, "platform-tools", exe)
    if (existsSync(candidate)) return candidate
  }
  return null
}

function requireAdb() {
  let path = resolveAdb()
  if (path) return path
  console.error("Could not find adb (Android Platform Tools).")
  console.error("Install it:")
  console.error("  Windows: winget install Google.PlatformTools")
  console.error("  macOS:   brew install android-platform-tools")
  console.error("  Linux:   install your distro's android-tools / adb package")
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
// port, otherwise the project (or file) in the current directory.
async function resolveServer(): Promise<LiveRecord> {
  let resolved = port !== undefined ? await resolveByPort(port) : await resolveFromCwd(process.cwd())
  if (!resolved.ok) {
    console.error(resolved.message)
    process.exit(1)
  }
  return resolved.record
}

// Serials of connected, authorized devices (excludes offline/unauthorized).
function listDevices(adb: string): string[] {
  let listed = Bun.spawnSync([adb, "devices"], { stdout: "pipe", stderr: "pipe" })
  return listed.stdout
    .toString()
    .split("\n")
    .slice(1)
    .map((l) => l.trim())
    .filter(Boolean)
    .filter((l) => l.endsWith("\tdevice"))
    .map((l) => l.split("\t")[0])
    .filter((s): s is string => Boolean(s))
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

// Always shown up front, regardless of how many devices are connected or
// whether --device narrows it down: one line per device with the ABI its APK
// build would target, then (with --install) a dev-dependency hint for each
// ABI that isn't resolvable locally yet. Returns the device->abi map so
// callers don't have to re-query it.
function printDeviceStatus(adb: string, devices: string[]): Map<string, string> {
  let abiByDevice = new Map<string, string>()
  let missingAbis = new Set<string>()
  for (let d of devices) {
    let abi = deviceAbi(adb, d)
    abiByDevice.set(d, abi)
    console.log(`${d} - ${abi}`)
    if (!resolveApk(abi)) missingAbis.add(abi)
  }
  let missingPkgs = [...missingAbis].map((abi) => ANDROID_PKG_MAP[abi]).filter((pkg): pkg is string => Boolean(pkg))
  if (values.install && missingPkgs.length > 0) {
    console.log(`Add dev dependencies with: bun add -d ${missingPkgs.join(" ")}`)
  }
  return abiByDevice
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

// The first Android client that appears beyond `before`, or null after ~10 s.
async function waitForClient(server: LiveRecord, before: Set<number>): Promise<Client | null> {
  for (let attempt = 0; attempt < 20; attempt++) {
    await sleep(500)
    let fresh = (await connectedClients(server)).find((c) => c.platform === "android" && !before.has(c.id))
    if (fresh) return fresh
  }
  return null
}

// Resolve the target device serial (and its ABI). With --device, treat the
// value as a serial prefix and require it to match exactly one connected
// device; without it, use the sole connected device. Exits with a clear
// message on any ambiguity.
function resolveTarget(adb: string): { target: string; abi: string } {
  let devices = listDevices(adb)
  let abiByDevice = printDeviceStatus(adb, devices)

  if (values.device) {
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
    return { target: match, abi: abiByDevice.get(match)! }
  }

  if (devices.length > 1) {
    console.error("Pick one with --device <serial or prefix>.")
    process.exit(1)
  }
  let [only] = devices
  if (!only) {
    console.error("No authorized Android device found. Enable USB debugging and check `adb devices`.")
    process.exit(1)
  }
  return { target: only, abi: abiByDevice.get(only)! }
}

// Launch the Android client on a connected device over adb (installing it
// first on --install), passing it the dev-server address to dial as a
// launch-intent extra (see devServerAddress), then wait briefly for it to show
// up on the dev server. The client is not a child process here: its lifecycle
// is the WS connect/disconnect the server sees.
export async function main() {
  let server = await resolveServer()
  let adb = requireAdb()

  let { target, abi } = resolveTarget(adb)

  if (values.install) {
    let apk = resolveApk(abi)
    if (!apk) {
      console.error(`Could not find a SolidRT-Go APK for ABI "${abi}".`)
      let pkg = ANDROID_PKG_MAP[abi]
      if (pkg) console.error(`Add it with: bun add -d ${pkg}`)
      process.exit(1)
    }
    console.log(`[cli] Installing SolidRT-Go on ${target}`)
    let install = Bun.spawn([adb, "-s", target, "install", "-r", apk], { stdout: "pipe", stderr: "pipe" })
    if ((await install.exited) !== 0) {
      console.error("adb install failed:\n" + (await new Response(install.stderr).text()))
      process.exit(1)
    }
  } else {
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

  // Hand the client the dev-server address to dial, as a launch-intent extra that
  // MainActivity forwards to native argv (--dev-server); the client auto-connects
  // to it. Replaces adb reverse, which never worked over wireless adb. -S stops
  // a running instance first: a delivered intent does not reach one, so without
  // it the client would keep whatever server it had.
  let before = new Set((await connectedClients(server)).map((c) => c.id))
  let devServer = devServerAddress(adb, target, server)
  let launchArgs = [adb, "-s", target, "shell", "am", "start", "-S", "-n", PACKAGE_ACTIVITY]
  if (devServer) {
    console.log(`[cli] Client will dial dev server at ${devServer}`)
    launchArgs.push("--es", "srt_dev_server", devServer)
  } else {
    console.log("[cli] Could not resolve a host address for the device; client will need a manual/QR connect")
  }

  let start = Bun.spawn(launchArgs, { stdout: "pipe", stderr: "pipe" })
  if ((await start.exited) !== 0) {
    console.error("adb start failed:\n" + (await new Response(start.stderr).text()))
    process.exit(1)
  }

  console.log(`[cli] Launched SolidRT-Go on ${target}; waiting for it to connect to the dev server...`)
  let client = await waitForClient(server, before)
  if (client) {
    console.log(`[cli] Client ${client.id} connected (${client.platform}, ${client.version})`)
  } else {
    console.log(`[cli] No connection after 10 s. The server must run with --lan and the device must reach this machine.`)
  }
}