// Patch a runner APK into an app's APK, with no Android SDK involved: rewrite
// the application id, versionCode and versionName in the compiled manifest
// and the launcher label in the resource table (strings.ts), swap the
// adaptive-icon slot PNGs (icon.ts), add the .srtapp payload as a stored
// asset, then re-align and re-sign the zip (zip.ts, sign.ts). The dex, the
// native libs and every other resource are carried byte-for-byte; the
// activity class name is stored fully qualified in the manifest, so changing
// the id never touches the dex (okf/backlog/standalone-android-apk.md).

import { inflateRawSync, deflateRawSync } from "node:zlib"
import { parseZip, writeZip, crc32, type ZipEntry } from "./zip"
import { manifestInfo, replacePoolStrings, poolStrings, XML_POOL_OFFSET, TABLE_POOL_OFFSET } from "./strings"
import { backgroundPixel } from "./icon"
import { signApk } from "./sign"

// Where the payload lands in the APK: under assets/ so the runtime can reach
// it through AAssetManager (open_file_descriptor needs an asset, not just any
// zip entry), stored so it is read in place with no extraction.
const PAYLOAD_ENTRY = "assets/app.srtapp"

// The adaptive-icon slots the runner bakes (ic_launcher_prod.xml): the
// foreground PNG sits behind a safe-zone inset, the background is a 1x1
// stretched full-bleed. Absent in a go-client base, where icon patching is
// silently skipped.
const ICON_FG_ENTRY = "res/drawable/app_icon_fg.png"
const ICON_BG_ENTRY = "res/drawable/app_icon_bg.png"

// The launcher label the base APK's resources.arsc carries, located by value:
// resolving the label through the resource table proper would take a full
// table parse for a string that is fixed per runner build. Two known bases:
// the runner APK (prod flavor) and, while the runner is pending on a machine,
// the go dev client.
const BASE_LABELS = ["SolidRT App", "Player"]

export type ApkPatch = {
  appId: string
  label: string
  payload: Buffer
  versionCode: number
  versionName: string
  // The app's launcher icon as a square PNG; undefined keeps the runner's
  // placeholder slot.
  icon?: Buffer
  // Adaptive-icon background, "#rrggbb".
  iconBackground: string
}

function entryNamed(entries: ZipEntry[], name: string): ZipEntry {
  let entry = entries.find((e) => e.name.toString("latin1") === name)
  if (!entry) throw new Error(`Base APK has no ${name}`)
  return entry
}

// Replace an entry's content, keeping its packing method (a deflated entry
// stays deflated, a stored one stored).
function replaceData(entry: ZipEntry, bytes: Buffer) {
  entry.data = entry.method === 0 ? bytes : deflateRawSync(bytes, { level: 9 })
  entry.crc = crc32(bytes)
  entry.usize = bytes.length
}

// The manifest is deflated in the APK; patch the inflated bytes and deflate
// the result back into the entry. versionCode is a typed integer edited in
// place, which must happen before the pool rewrite recomputes the file.
function patchManifest(entry: ZipEntry, appId: string, versionCode: number, versionName: string) {
  let axml = inflateRawSync(entry.data)
  let info = manifestInfo(axml)
  axml.writeUInt32LE(versionCode, info.versionCodeOffset)
  let replacements = new Map([
    [info.packageIndex, appId],
    [info.versionNameIndex, versionName],
  ])
  replaceData(entry, replacePoolStrings(axml, XML_POOL_OFFSET, replacements))
}

function patchLabel(entry: ZipEntry, label: string) {
  let strings = poolStrings(entry.data, TABLE_POOL_OFFSET)
  let index = -1
  for (let candidate of BASE_LABELS) {
    let matches = strings.flatMap((s, i) => (s === candidate ? [i] : []))
    if (matches.length > 1) throw new Error(`Base APK label "${candidate}" is ambiguous in resources.arsc`)
    if (matches.length === 1) {
      index = matches[0]!
      break
    }
  }
  if (index < 0) throw new Error(`Base APK label not found in resources.arsc (expected one of: ${BASE_LABELS.join(", ")})`)
  replaceData(entry, replacePoolStrings(entry.data, TABLE_POOL_OFFSET, new Map([[index, label]])))
}

// Swap the adaptive-icon slot PNGs. Returns false when the base carries no
// slots (the go client), so the caller can say the icon was not applied.
function patchIcon(entries: ZipEntry[], icon: Buffer | undefined, background: string): boolean {
  let fg = entries.find((e) => e.name.toString("latin1") === ICON_FG_ENTRY)
  let bg = entries.find((e) => e.name.toString("latin1") === ICON_BG_ENTRY)
  if (!fg || !bg) return false
  if (icon) replaceData(fg, icon)
  replaceData(bg, backgroundPixel(background))
  return true
}

export function patchApk(base: Buffer, patch: ApkPatch): { apk: Buffer; iconApplied: boolean } {
  let entries = parseZip(base)
  patchManifest(entryNamed(entries, "AndroidManifest.xml"), patch.appId, patch.versionCode, patch.versionName)
  patchLabel(entryNamed(entries, "resources.arsc"), patch.label)
  let iconApplied = patchIcon(entries, patch.icon, patch.iconBackground)

  // The payload entry borrows its bookkeeping fields (fixed timestamp,
  // version words, attributes) from another stored entry so the zip stays
  // uniform with what the build pipeline produced.
  let template = entryNamed(entries, "resources.arsc")
  entries.push({
    name: Buffer.from(PAYLOAD_ENTRY, "latin1"),
    verMade: template.verMade,
    verNeed: template.verNeed,
    flags: template.flags,
    method: 0,
    time: template.time,
    date: template.date,
    crc: crc32(patch.payload),
    usize: patch.payload.length,
    intAttr: template.intAttr,
    extAttr: template.extAttr,
    data: patch.payload,
  })

  let { local, cd } = writeZip(entries)
  return { apk: signApk(local, cd, entries.length), iconApplied }
}

// The application id an APK carries, read back from its compiled manifest.
// The install side (srt android <file.apk>) needs it to address the launcher
// activity, and the APK itself is the only place it lives.
export function apkApplicationId(apk: Buffer): string {
  let entry = entryNamed(parseZip(apk), "AndroidManifest.xml")
  let axml = entry.method === 0 ? entry.data : inflateRawSync(entry.data)
  return manifestInfo(axml).packageValue
}
