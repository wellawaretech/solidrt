import { cpSync, existsSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs"
import { basename, dirname, join, resolve } from "node:path"
import {
  assetPathFor,
  collectAssets,
  loadAppIdentity,
  projectDirFor,
  RUNTIME_VERSION,
  type ManifestFont,
} from "./project"
import { resolvePackFonts } from "./fonts"

// The canonical flat pack folder (okf/plans/client-storage-updates.md, Pack
// output): runner + manifest.json + bundle.bin + assets/. The manifest
// enumerates exactly the files belonging to the version - the runner is
// deliberately unlisted - and, unlike dev manifests, carries the full app
// identity (org, displayName) and the complete font set with the default
// fonts materialized under assets/fonts/.

function fail(message: string): never {
  console.error(message)
  process.exit(1)
}

function hashHex(bytes: Uint8Array): string {
  return new Bun.CryptoHasher("sha256").update(bytes).digest("hex")
}

export type PackFolder = {
  /** The canonical manifest JSON string (serialized once, written verbatim). */
  manifest: string
  /** Files to place in the folder: absolute source -> folder-relative path. */
  copies: Array<{ from: string; to: string }>
}

export function buildPackFolder(entry: string, bytecode: Buffer): PackFolder {
  let identity = loadAppIdentity(entry)
  let projectDir = projectDirFor(resolve(entry))
  let { assets } = collectAssets(entry)
  let copies = assets.map((a) => ({ from: join(projectDir, a.path), to: a.path }))

  // The full resolved font set: custom fonts are already collected assets;
  // defaults materialize under assets/fonts/ (a user file already at that
  // path must be the same bytes, otherwise the layout is ambiguous).
  let fonts: ManifestFont[] = []
  for (let font of resolvePackFonts(entry)) {
    if (font.isDefault) {
      let path = "assets/fonts/" + basename(font.path)
      let bytes = readFileSync(font.path)
      let existing = assets.find((a) => a.path === path)
      if (existing) {
        if (existing.sha256 !== hashHex(bytes)) {
          fail(`${path} collides with the packed default font; rename it or bind it via "solidrt.fonts"`)
        }
      } else {
        assets.push({ path, sha256: hashHex(bytes), size: bytes.length })
        copies.push({ from: font.path, to: path })
      }
      fonts.push({ path, alias: font.alias })
    } else {
      let path = assetPathFor(projectDir, font.path)
      if (!path) fail(`"solidrt.fonts": "${font.alias}": ${font.path} must live under assets/`)
      fonts.push({ path, alias: font.alias })
    }
  }
  assets.sort((a, b) => (a.path < b.path ? -1 : 1))

  let manifest = JSON.stringify({
    appId: identity.appId,
    org: identity.org,
    displayName: identity.displayName,
    runtimeVersion: RUNTIME_VERSION,
    bundle: { path: "bundle.bin", sha256: hashHex(bytecode), size: bytecode.length },
    ...(assets.length ? { assets } : {}),
    ...(fonts.length ? { fonts } : {}),
  })
  return { manifest, copies }
}

/**
 * Write the folder. An existing output dir is only reused when it is empty or
 * already a pack folder (has a manifest.json) - then the files this pack owns
 * (runner, manifest, bundle, assets/) are replaced; anything else in it is
 * left alone but never a reason to touch an unrelated directory.
 */
export function writePackFolder(outDir: string, runnerPath: string, bytecode: Buffer, folder: PackFolder) {
  let existing = existsSync(outDir) ? readdirSync(outDir) : null
  if (existing && existing.length > 0 && !existing.includes("manifest.json")) {
    fail(`${resolve(outDir)} exists and is not a pack folder; choose another --output`)
  }

  let runnerName = "solidrt" + (process.platform === "win32" ? ".exe" : "")
  mkdirSync(outDir, { recursive: true })
  rmSync(join(outDir, "assets"), { recursive: true, force: true })
  for (let name of ["manifest.json", "bundle.bin", runnerName]) {
    rmSync(join(outDir, name), { force: true })
  }

  // Dereference: the runner path may itself be a symlink (contributor dist
  // layouts); the folder must carry the real binary.
  cpSync(runnerPath, join(outDir, runnerName), { dereference: true })
  if (process.platform !== "win32") {
    Bun.spawnSync(["chmod", "+x", join(outDir, runnerName)])
  }
  writeFileSync(join(outDir, "bundle.bin"), bytecode)
  writeFileSync(join(outDir, "manifest.json"), folder.manifest)
  for (let { from, to } of folder.copies) {
    let dest = join(outDir, to)
    mkdirSync(dirname(dest), { recursive: true })
    cpSync(from, dest)
  }
}
