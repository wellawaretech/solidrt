import { chmodSync, mkdirSync, writeFileSync } from "node:fs"
import { dirname, join } from "node:path"
import { inflateRawSync } from "node:zlib"
import { devDir } from "../lib/dev-dir"
import { parseZip } from "../pack/android/zip"

// Google publishes platform-tools (adb, fastboot) as a standalone ~15 MB zip
// at a stable versionless URL per OS, precisely so adb does not require the
// 3 GB SDK. Extracted into ~/.solidrt/ (the zip's top-level folder is
// platform-tools/), beside servers/ and clients/; removing the folder
// uninstalls it. No sudo, nothing touches the system.

// Zip compression method for entries that need inflating (0 = stored).
const METHOD_DEFLATED = 8

function zipUrl(): string {
  let os = process.platform === "win32" ? "windows" : process.platform === "darwin" ? "darwin" : "linux"
  return `https://dl.google.com/android/repository/platform-tools-latest-${os}.zip`
}

// Whether Google's zip runs on this machine: the mac build is universal and
// Windows arm64 emulates x64, but the Linux build is x86-64 only, so on
// linux-arm64 (Raspberry Pi, ARM Chromebook Crostini) the distro package is
// the only working route and the download must not be offered.
export function platformToolsAvailable(): boolean {
  return process.platform !== "linux" || process.arch === "x64"
}

// Download and extract platform-tools under the dev dir. Returns false (with
// the error printed) when the download or the archive is bad; the caller
// re-resolves adb on success.
export async function installPlatformTools(): Promise<boolean> {
  let url = zipUrl()
  console.log(`[cli] Downloading ${url}`)
  let resp: Response
  try {
    resp = await fetch(url)
  } catch (e) {
    console.error(`Download failed: ${e instanceof Error ? e.message : e}`)
    return false
  }
  if (!resp.ok) {
    console.error(`Download failed: ${resp.status} ${resp.statusText}`)
    return false
  }
  let zip = Buffer.from(await resp.arrayBuffer())
  let root = devDir()
  try {
    for (let entry of parseZip(zip)) {
      let name = entry.name.toString()
      if (name.endsWith("/")) continue
      // Never write outside the dev dir, whatever the archive says.
      if (name.startsWith("/") || name.split("/").includes("..")) continue
      let dest = join(root, name)
      mkdirSync(dirname(dest), { recursive: true })
      writeFileSync(dest, entry.method === METHOD_DEFLATED ? inflateRawSync(entry.data) : entry.data)
      // The zip's external attributes carry the unix mode in the high 16
      // bits; applying it keeps adb executable (absent on Windows-made
      // entries, where the mode does not matter).
      let mode = (entry.extAttr >>> 16) & 0o777
      if (mode) chmodSync(dest, mode)
    }
  } catch (e) {
    console.error(`Could not extract platform-tools: ${e instanceof Error ? e.message : e}`)
    return false
  }
  console.log(`[cli] Installed platform-tools into ${join(root, "platform-tools")}`)
  return true
}