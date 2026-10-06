// Patch a runner APK into an app's APK, with no Android SDK involved: rewrite
// the application id, versionCode and versionName in the compiled manifest
// (the id's pool string is also the VIEW intent filter's scheme, so the
// rewrite renames the scheme the app answers to with it; see the runner's
// AndroidManifest.xml), declare the app's permissions there and, when
// the app opts in, turn backup on for its data/ folder; rewrite the launcher
// label in the resource table (strings.ts), swap the adaptive-icon slot PNGs
// (icon.ts), add the .solapp payload as a stored asset, then re-align and
// re-sign the zip (zip.ts, sign.ts). The dex, the native libs and every
// other resource are carried byte-for-byte; the activity class name is
// stored fully qualified in the manifest, so changing the id never touches
// the dex (okf/done/standalone-android-apk.md,
// okf/done/packed-apk-data-on-device.md).

import { createHash } from "node:crypto"
import { inflateRawSync, deflateRawSync } from "node:zlib"
import { parseZip, writeZip, crc32, type ZipEntry } from "./zip"
import {
  manifestInfo,
  applicationFlagOffset,
  addUsesPermissions,
  replacePoolStrings,
  poolStrings,
  BOOL_TRUE,
  XML_POOL_OFFSET,
  TABLE_POOL_OFFSET,
} from "./strings"
import { backgroundPixel } from "./icon"
import { signApk } from "./sign"

// Where the payload lands in the APK: under assets/ so the runtime can reach
// it through AAssetManager (open_file_descriptor needs an asset, not just any
// zip entry), stored so it is read in place with no extraction.
const PAYLOAD_ENTRY = "assets/app.solapp"

// The adaptive-icon slots the runner bakes (ic_launcher_runner.xml): the
// foreground PNG sits behind a safe-zone inset, the background is a 1x1
// stretched full-bleed.
const ICON_FG_ENTRY = "res/drawable/app_icon_fg.png"
const ICON_BG_ENTRY = "res/drawable/app_icon_bg.png"

// The backup rule files the runner carries (runner res/xml/), one pair per
// rule format: the "none" file the manifest references, and the data-only
// file whose bytes replace it when the app opts in. Swapping bytes keeps the
// manifest's resource references untouched.
const BACKUP_RULES: [none: string, data: string][] = [
  ["res/xml/backup_none.xml", "res/xml/backup_data.xml"],
  ["res/xml/extraction_none.xml", "res/xml/extraction_data.xml"],
]

// The launcher label the runner APK's resources.arsc carries (runner flavor
// strings.xml), located by value: resolving the label through the resource
// table proper would take a full table parse for a string that is fixed per
// runner build.
const BASE_LABEL = "Runner"

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
  // Fully qualified permission names to declare (the capabilities mapped,
  // plus the project's extras; duplicates are skipped).
  permissions: string[]
  // Back up the app's data/ folder; off keeps everything on the device.
  backup: boolean
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
// the result back into the entry. versionCode and allowBackup are typed
// values edited in place, which must happen before the permission splice and
// the pool rewrite move bytes and recompute the file.
function patchManifest(entry: ZipEntry, patch: ApkPatch, versionName: string) {
  let axml: Buffer = inflateRawSync(entry.data)
  let info = manifestInfo(axml)
  axml.writeUInt32LE(patch.versionCode, info.versionCodeOffset)
  if (patch.backup) axml.writeUInt32LE(BOOL_TRUE, applicationFlagOffset(axml, "allowBackup"))
  axml = addUsesPermissions(axml, patch.permissions)
  let replacements = new Map([
    [info.packageIndex, patch.appId],
    [info.versionNameIndex, versionName],
  ])
  replaceData(entry, replacePoolStrings(axml, XML_POOL_OFFSET, replacements))
}

// Opt the app into backup: the rule files the manifest references take the
// bytes of their data-only counterparts (allowBackup itself is flipped in
// patchManifest).
function patchBackup(entries: ZipEntry[]) {
  for (let [none, data] of BACKUP_RULES) {
    let source = entryNamed(entries, data)
    replaceData(entryNamed(entries, none), source.method === 0 ? source.data : inflateRawSync(source.data))
  }
}

function patchLabel(entry: ZipEntry, label: string) {
  let matches = poolStrings(entry.data, TABLE_POOL_OFFSET).flatMap((s, i) => (s === BASE_LABEL ? [i] : []))
  if (matches.length > 1) throw new Error(`Base APK label "${BASE_LABEL}" is ambiguous in resources.arsc`)
  let [index] = matches
  if (index === undefined) {
    throw new Error(`Base APK label "${BASE_LABEL}" not found in resources.arsc; is the base an older runner build?`)
  }
  replaceData(entry, replacePoolStrings(entry.data, TABLE_POOL_OFFSET, new Map([[index, label]])))
}

// Swap the adaptive-icon slot PNGs; without an app icon the runner's
// placeholder foreground stays.
function patchIcon(entries: ZipEntry[], icon: Buffer | undefined, background: string) {
  if (icon) replaceData(entryNamed(entries, ICON_FG_ENTRY), icon)
  replaceData(entryNamed(entries, ICON_BG_ENTRY), backgroundPixel(background))
}

export function patchApk(base: Buffer, patch: ApkPatch): Buffer {
  let entries = parseZip(base)
  patchManifest(entryNamed(entries, "AndroidManifest.xml"), patch, patch.versionName)
  if (patch.backup) patchBackup(entries)
  patchLabel(entryNamed(entries, "resources.arsc"), patch.label)
  patchIcon(entries, patch.icon, patch.iconBackground)

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
  return signApk(local, cd, entries.length)
}

// The inflated compiled manifest of an APK's entries.
function manifestBytes(entries: ZipEntry[]): { entry: ZipEntry; axml: Buffer } {
  let entry = entryNamed(entries, "AndroidManifest.xml")
  return { entry, axml: entry.method === 0 ? entry.data : inflateRawSync(entry.data) }
}

// The application id an APK carries, read back from its compiled manifest.
// The install side (sol android <file.apk>) needs it to address the launcher
// activity, and the APK itself is the only place it lives.
export function apkApplicationId(apk: Buffer): string {
  return manifestInfo(manifestBytes(parseZip(apk)).axml).packageValue
}

// The versionName an APK carries: what `sol android` compares the installed
// Player against, and what a derived APK suffixes (swapLibs).
export function apkVersionName(apk: Buffer): string {
  let { axml } = manifestBytes(parseZip(apk))
  let info = manifestInfo(axml)
  return poolStrings(axml, XML_POOL_OFFSET)[info.versionNameIndex]!
}

// How many hex digits of the libs' digest name a derived APK's version.
const LIBS_HASH_CHARS = 8

// A content digest of the libs, what makes a rebuilt runtime a new version
// to `sol android` (which updates an installed Player whose versionName
// differs from the project's).
function libsHash(libs: { name: string; bytes: Buffer }[]): string {
  let hash = createHash("sha256")
  for (let lib of libs) {
    hash.update(lib.name)
    hash.update(lib.bytes)
  }
  return hash.digest("hex").slice(0, LIBS_HASH_CHARS)
}

// A project runtime's APK (lib/runtime.ts): the base APK (the stock Player
// or runner) with `lib/<abi>/<name>` replaced, or added, for each lib, each
// entry keeping the base's packing (deflated in the runner, stored and
// page-aligned in the Player), the versionName suffixed with the libs'
// digest, the zip re-aligned and re-signed with the development key. The
// shell (dex, resources, the other libs) is the base's byte for byte: a
// custom runtime differs from the stock one in its native code alone.
export function swapLibs(base: Buffer, abi: string, libs: { name: string; bytes: Buffer }[]): Buffer {
  let entries = parseZip(base)
  let prefix = `lib/${abi}/`
  let template = entries.find((e) => e.name.toString("latin1").startsWith(prefix))
  if (!template) throw new Error(`Base APK has no ${prefix} libs`)
  for (let lib of libs) {
    let name = prefix + lib.name
    let entry = entries.find((e) => e.name.toString("latin1") === name)
    if (!entry) {
      entry = { ...template, name: Buffer.from(name, "latin1") }
      entries.push(entry)
    }
    replaceData(entry, lib.bytes)
  }
  let { entry, axml } = manifestBytes(entries)
  let info = manifestInfo(axml)
  // A base that is itself derived keeps one suffix.
  let stem = poolStrings(axml, XML_POOL_OFFSET)[info.versionNameIndex]!.split("+")[0]!
  let version = `${stem}+r${libsHash(libs)}`
  replaceData(entry, replacePoolStrings(axml, XML_POOL_OFFSET, new Map([[info.versionNameIndex, version]])))
  let { local, cd } = writeZip(entries)
  return signApk(local, cd, entries.length)
}
