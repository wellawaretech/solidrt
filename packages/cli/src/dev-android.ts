import { print, requireAdb } from "./util"
import { resolveApk } from "./artifacts"
import { values } from "./args"
import { DEV_PORT } from "./dev-server"

// Launch component of the "go" dev-client flavor (see lattice/Makefile.android).
let PACKAGE_ACTIVITY = "com.solidrt.go/com.solidrt.app.MainActivity"

// Forward the device's loopback DEV_PORT to the host dev server, so the client
// reaches it at 127.0.0.1:DEV_PORT (see lattice/src/go/connection.rs). This is
// the adb-reverse path: it works for the emulator (behind NAT, cannot reach the
// host via LAN UDP discovery) and for USB-tethered devices alike, and is
// harmless on any adb connection. Devices not launched via adb fall back to the
// client's standard discovery flow.
function setupAdbReverse(adb: string, target: string) {
  print(`[cli] Forwarding 127.0.0.1:${DEV_PORT} on ${target} to host dev server`)
  let res = Bun.spawnSync([adb, "-s", target, "reverse", `tcp:${DEV_PORT}`, `tcp:${DEV_PORT}`], {
    stdout: "pipe",
    stderr: "pipe",
  })
  if (res.exitCode !== 0) {
    print(`[cli] adb reverse failed (client will fall back to discovery):\n${res.stderr.toString()}`)
  }
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

// Resolve the target device serial. With --device, treat the value as a serial
// prefix and require it to match exactly one connected device; without it, use
// the sole connected device. Exits with a clear message on any ambiguity.
function resolveTarget(adb: string): string {
  let devices = listDevices(adb)

  if (values.device) {
    let prefix = values.device
    let matches = devices.filter((d) => d.startsWith(prefix))
    if (matches.length > 1) {
      console.error(`--device "${prefix}" is ambiguous; matches: ${matches.join(", ")}`)
      process.exit(1)
    }
    let [match] = matches
    if (!match) {
      console.error(`No connected device matches --device "${prefix}". Connected: ${devices.join(", ") || "none"}`)
      process.exit(1)
    }
    return match
  }

  if (devices.length > 1) {
    console.error(`Multiple devices connected (${devices.join(", ")}); pick one with --device <serial or prefix>.`)
    process.exit(1)
  }
  let [only] = devices
  if (!only) {
    console.error("No authorized Android device found. Enable USB debugging and check `adb devices`.")
    process.exit(1)
  }
  return only
}

// Install + launch the Android client on a connected device over adb, forwarding
// its loopback to the host dev server so the client connects at 127.0.0.1 (see
// setupAdbReverse). Fire-and-forget: the client's lifecycle is tracked via WS
// connect/disconnect in dev-server.ts, not as a child process here.
export async function spawnAndroidClient() {
  let adb = requireAdb()

  let apk = resolveApk()
  if (!apk) {
    console.error("Could not find the SolidRT-Go APK.")
    console.error("Add it with: bun add -d @solidrt/android-arm64-v8a")
    process.exit(1)
  }

  let target = resolveTarget(adb)

  print(`[cli] Installing SolidRT-Go on ${target}`)
  let install = Bun.spawn([adb, "-s", target, "install", "-r", apk], { stdout: "pipe", stderr: "pipe" })
  if ((await install.exited) !== 0) {
    console.error("adb install failed:\n" + (await new Response(install.stderr).text()))
    process.exit(1)
  }

  setupAdbReverse(adb, target)

  let start = Bun.spawn([adb, "-s", target, "shell", "am", "start", "-n", PACKAGE_ACTIVITY], {
    stdout: "pipe",
    stderr: "pipe",
  })
  if ((await start.exited) !== 0) {
    console.error("adb start failed:\n" + (await new Response(start.stderr).text()))
    process.exit(1)
  }

  print(`[cli] Launched SolidRT-Go on ${target}; waiting for it to connect to the dev server...`)
}