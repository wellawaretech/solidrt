// Patch a runner APK into an app's APK, with no Android SDK involved: rewrite
// the application id in the compiled manifest and the launcher label in the
// resource table (strings.ts), add the .srtapp payload as a stored asset,
// then re-align and re-sign the zip (zip.ts, sign.ts). The dex, the native
// libs and every resource file are carried byte-for-byte; the activity class
// name is stored fully qualified in the manifest, so changing the id never
// touches the dex (okf/backlog/standalone-android-apk.md).

import { inflateRawSync, deflateRawSync } from "node:zlib"
import { parseZip, writeZip, crc32, type ZipEntry } from "./zip"
import { manifestPackageIndex, replacePoolStrings, poolStrings, XML_POOL_OFFSET, TABLE_POOL_OFFSET } from "./strings"
import { signApk } from "./sign"

// Where the payload lands in the APK: under assets/ so the runtime can reach
// it through AAssetManager (open_file_descriptor needs an asset, not just any
// zip entry), stored so it is read in place with no extraction.
const PAYLOAD_ENTRY = "assets/app.srtapp"

// The launcher label the base APK's resources.arsc carries, located by value:
// resolving the label through the resource table proper would take a full
// table parse for a string that is fixed per runner build. Two known bases:
// the runner APK (prod flavor) and, while the runner is pending on a machine,
// the go dev client.
const BASE_LABELS = ["SolidRT App", "SolidRT Go"]

function entryNamed(entries: ZipEntry[], name: string): ZipEntry {
  let entry = entries.find((e) => e.name.toString("latin1") === name)
  if (!entry) throw new Error(`Base APK has no ${name}`)
  return entry
}

// The manifest is deflated in the APK; patch the inflated bytes and deflate
// the result back into the entry, refreshing crc and sizes.
function patchManifest(entry: ZipEntry, appId: string) {
  let axml = inflateRawSync(entry.data)
  let pkg = manifestPackageIndex(axml)
  let patched = replacePoolStrings(axml, XML_POOL_OFFSET, new Map([[pkg.index, appId]]))
  entry.data = deflateRawSync(patched, { level: 9 })
  entry.crc = crc32(patched)
  entry.usize = patched.length
}

// resources.arsc is stored, so the patched bytes go in as-is.
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
  entry.data = replacePoolStrings(entry.data, TABLE_POOL_OFFSET, new Map([[index, label]]))
  entry.crc = crc32(entry.data)
  entry.usize = entry.data.length
}

export function patchApk(base: Buffer, appId: string, label: string, payload: Buffer): Buffer {
  let entries = parseZip(base)
  patchManifest(entryNamed(entries, "AndroidManifest.xml"), appId)
  patchLabel(entryNamed(entries, "resources.arsc"), label)

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
    crc: crc32(payload),
    usize: payload.length,
    intAttr: template.intAttr,
    extAttr: template.extAttr,
    data: payload,
  })

  let { local, cd } = writeZip(entries)
  return signApk(local, cd, entries.length)
}

// The application id an APK carries, read back from its compiled manifest.
// The install side (srt android <file.apk>) needs it to address the launcher
// activity, and the APK itself is the only place it lives.
export function apkApplicationId(apk: Buffer): string {
  let entry = entryNamed(parseZip(apk), "AndroidManifest.xml")
  let axml = entry.method === 0 ? entry.data : inflateRawSync(entry.data)
  return manifestPackageIndex(axml).value
}
