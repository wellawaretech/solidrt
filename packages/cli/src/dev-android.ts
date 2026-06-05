import { print, requireAdb } from "./util"
import { resolveApk } from "./artifacts"
import { values } from "./args"

// Launch component of the "go" dev-client flavor (see lattice/Makefile.x-android).
let PACKAGE_ACTIVITY = "com.solidrt.go/com.solidrt.app.MainActivity"

// Prefix adb args with `-s <serial>` only when the user pinned a device; with a
// single connected device adb selects it on its own.
function adbArgs(extra: string[]) {
  return values.device ? ["-s", values.device, ...extra] : extra
}

// Install + launch the Android client on a connected device over adb, then let
// the device discover the running dev server over LAN UDP (the same path as a
// manually launched client). Fire-and-forget: the client's lifecycle is tracked
// via WS connect/disconnect in dev-server.ts, not as a child process here.
export async function spawnAndroidClient() {
  let adb = requireAdb()

  let apk = resolveApk()
  if (!apk) {
    console.error("Could not find the Android client APK.")
    console.error("Add it with: bun add -d @solidrt/android-arm64-v8a")
    process.exit(1)
  }

  // Resolve the target device: 0 -> error, 1 -> use it, many -> require --device.
  let target = values.device
  if (!target) {
    let listed = Bun.spawnSync([adb, "devices"], { stdout: "pipe", stderr: "pipe" })
    let devices = listed.stdout
      .toString()
      .split("\n")
      .slice(1)
      .map((l) => l.trim())
      .filter(Boolean)
      .filter((l) => l.endsWith("\tdevice"))
      .map((l) => l.split("\t")[0])

    if (devices.length === 0) {
      console.error("No authorized Android device found. Enable USB debugging and check `adb devices`.")
      process.exit(1)
    }
    if (devices.length > 1) {
      console.error(`Multiple devices connected (${devices.join(", ")}); pick one with --device <serial>.`)
      process.exit(1)
    }
    target = devices[0]
  }

  print(`[cli] Installing Android client on ${target}`)
  let install = Bun.spawn([adb, ...adbArgs(["install", "-r", apk])], { stdout: "pipe", stderr: "pipe" })
  if ((await install.exited) !== 0) {
    console.error("adb install failed:\n" + (await new Response(install.stderr).text()))
    process.exit(1)
  }

  let start = Bun.spawn([adb, ...adbArgs(["shell", "am", "start", "-n", PACKAGE_ACTIVITY])], {
    stdout: "pipe",
    stderr: "pipe",
  })
  if ((await start.exited) !== 0) {
    console.error("adb start failed:\n" + (await new Response(start.stderr).text()))
    process.exit(1)
  }

  print(`[cli] Launched Android client on ${target}; waiting for it to discover the dev server...`)
}