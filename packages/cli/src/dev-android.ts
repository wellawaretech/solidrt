import { networkInterfaces } from "node:os"
import { print, requireAdb } from "./util"
import { resolveApk, ANDROID_PKG_MAP } from "./artifacts"
import { values } from "./args"
import { DEV_PORT } from "./dev-server"

// Launch component of the "go" dev-client flavor (see lattice/Makefile.android).
let PACKAGE_ACTIVITY = "com.solidrt.go/com.solidrt.app.MainActivity"

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
function devServerAddress(adb: string, target: string): string | null {
  if (target.startsWith("emulator-")) return `10.0.2.2:${DEV_PORT}`
  let dip = deviceIp(adb, target)
  if (!dip) return null
  let host = hostIpFor(dip)
  return host ? `${host}:${DEV_PORT}` : null
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
// build would target, then a dev-dependency hint for each ABI that isn't
// resolvable locally yet. Returns the device->abi map so callers don't have
// to re-query it.
function printDeviceStatus(adb: string, devices: string[]): Map<string, string> {
  let abiByDevice = new Map<string, string>()
  let missingAbis = new Set<string>()
  for (let d of devices) {
    let abi = deviceAbi(adb, d)
    abiByDevice.set(d, abi)
    print(`${d} - ${abi}`)
    if (!resolveApk(abi)) missingAbis.add(abi)
  }
  let missingPkgs = [...missingAbis].map((abi) => ANDROID_PKG_MAP[abi]).filter((pkg): pkg is string => Boolean(pkg))
  if (missingPkgs.length > 0) print(`Add dev dependencies with: bun add -d ${missingPkgs.join(" ")}`)
  return abiByDevice
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

// Install + launch the Android client on a connected device over adb, passing it
// the dev-server address to dial as a launch-intent extra (see devServerAddress).
// Fire-and-forget: the client's lifecycle is tracked via WS connect/disconnect in
// dev-server.ts, not as a child process here.
export async function spawnAndroidClient() {
  let adb = requireAdb()

  let { target, abi } = resolveTarget(adb)

  let apk = resolveApk(abi)
  if (!apk) {
    console.error(`Could not find a SolidRT-Go APK for ABI "${abi}".`)
    let pkg = ANDROID_PKG_MAP[abi]
    if (pkg) console.error(`Add it with: bun add -d ${pkg}`)
    process.exit(1)
  }

  print(`[cli] Installing SolidRT-Go on ${target}`)
  let install = Bun.spawn([adb, "-s", target, "install", "-r", apk], { stdout: "pipe", stderr: "pipe" })
  if ((await install.exited) !== 0) {
    console.error("adb install failed:\n" + (await new Response(install.stderr).text()))
    process.exit(1)
  }

  // Hand the client the dev-server address to dial, as a launch-intent extra that
  // MainActivity forwards to native argv (--dev-server); the client auto-connects
  // to it. Replaces adb reverse, which never worked over wireless adb.
  let devServer = devServerAddress(adb, target)
  let launchArgs = [adb, "-s", target, "shell", "am", "start", "-n", PACKAGE_ACTIVITY]
  if (devServer) {
    print(`[cli] Client will dial dev server at ${devServer}`)
    launchArgs.push("--es", "srt_dev_server", devServer)
  } else {
    print("[cli] Could not resolve a host address for the device; client will need a manual/QR connect")
  }

  let start = Bun.spawn(launchArgs, { stdout: "pipe", stderr: "pipe" })
  if ((await start.exited) !== 0) {
    console.error("adb start failed:\n" + (await new Response(start.stderr).text()))
    process.exit(1)
  }

  print(`[cli] Launched SolidRT-Go on ${target}; waiting for it to connect to the dev server...`)
}