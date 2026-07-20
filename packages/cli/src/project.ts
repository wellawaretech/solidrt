import { existsSync, readFileSync, readdirSync, statSync } from "node:fs"
import { basename, dirname, isAbsolute, join, relative, resolve, sep } from "node:path"

// Project configuration lives in the `solidrt` key of the nearest package.json
// above the entry file (okf/plans/client-storage-updates.md):
//
//   "solidrt": {
//     "appId": "com.example.app",   // stable identity: storage dir, Android package id
//     "org": "Example",             // publisher, pref-path org component
//     "displayName": "Example App", // pref-path app component
//     "fonts": { ... }              // see fonts.ts
//   }
//
// Everything defaults from the package name (or the entry filename when there
// is no package.json) so a dev project needs zero config; `srt pack` warns
// when appId is defaulted, since a distributed app should pin its identity.

export function findProjectPackage(sourcePath: string): { dir: string; pkg: any } | null {
  let dir = resolve(dirname(sourcePath))
  while (true) {
    let pkgPath = resolve(dir, "package.json")
    if (existsSync(pkgPath)) {
      return { dir, pkg: JSON.parse(readFileSync(pkgPath, "utf8")) }
    }
    let parent = dirname(dir)
    if (parent === dir) return null
    dir = parent
  }
}

export type AppIdentity = { appId: string; org: string; displayName: string; defaulted: boolean }

function fail(message: string): never {
  console.error(message)
  process.exit(1)
}

// Storage directory component (matches the runtime's safe_component check).
let APP_ID_PATTERN = /^[A-Za-z0-9._-]+$/

// Derived values are sanitized into a valid appId; explicit config must
// already be valid (throw-in-dev policy: a bad value fails the command).
function sanitizeAppId(name: string): string {
  let id = name.replace(/[^A-Za-z0-9._-]+/g, "-").replace(/^[-.]+|[-.]+$/g, "")
  return id === "" || id === "." || id === ".." ? "app" : id
}

function checkField(value: string, what: string) {
  if (Buffer.byteLength(value, "utf8") > 255) fail(`"solidrt": ${what} is too long (max 255 bytes)`)
  if (/[/\\]/.test(value)) fail(`"solidrt": ${what} must not contain path separators`)
  if (value === "") fail(`"solidrt": ${what} must not be empty`)
}

// The version manifest for a bundle (okf/plans/client-storage-updates.md,
// stages 2 + 3): appId + runtimeVersion + the bundle entry + the collected
// assets/ tree + font annotations. The returned JSON string is canonical - it
// travels verbatim to clients and its sha256 is the version id, so it must
// never be re-serialized along the way. runtimeVersion is the constant 1
// until the derivation question is settled (see plan).
export function buildManifest(code: string, entry: string): string {
  let identity = loadAppIdentity(entry)
  let sha256 = new Bun.CryptoHasher("sha256").update(code).digest("hex")
  let { assets, fonts } = collectAssets(entry)
  return JSON.stringify({
    appId: identity.appId,
    runtimeVersion: 1,
    bundle: { path: "bundle.js", sha256, size: Buffer.byteLength(code, "utf8") },
    ...(assets.length ? { assets } : {}),
    ...(fonts.length ? { fonts } : {}),
  })
}

export type ManifestAsset = { path: string; sha256: string; size: number }
export type ManifestFont = { path: string; alias: string }

// The project root the assets/ convention hangs off: the nearest package.json
// dir, or the entry's own dir when there is none.
export function projectDirFor(sourcePath: string): string {
  return findProjectPackage(sourcePath)?.dir ?? resolve(dirname(sourcePath))
}

function walkAssets(assetsDir: string, dir: string, out: ManifestAsset[]) {
  for (let entry of readdirSync(dir, { withFileTypes: true })) {
    // Dotfiles (.DS_Store and friends) are tooling noise, not app assets.
    if (entry.name.startsWith(".")) continue
    let abs = join(dir, entry.name)
    if (entry.isDirectory()) {
      walkAssets(assetsDir, abs, out)
    } else if (entry.isFile()) {
      let bytes = readFileSync(abs)
      let path = "assets/" + relative(assetsDir, abs).split(sep).join("/")
      out.push({ path, sha256: new Bun.CryptoHasher("sha256").update(bytes).digest("hex"), size: bytes.length })
    }
  }
}

// The convention-first asset set: everything under the project's assets/
// folder (next to package.json), collected wholesale in sorted order so the
// manifest bytes are deterministic. Fonts are annotations pointing into that
// set: `solidrt.fonts` path entries must live under assets/ so they reach dev
// clients and the version store (`false` entries only drop pack defaults and
// have no manifest presence).
export function collectAssets(entry: string): { assets: ManifestAsset[]; fonts: ManifestFont[] } {
  let project = findProjectPackage(entry)
  let projectDir = projectDirFor(entry)
  let assetsDir = resolve(projectDir, "assets")

  let assets: ManifestAsset[] = []
  if (existsSync(assetsDir) && statSync(assetsDir).isDirectory()) {
    walkAssets(assetsDir, assetsDir, assets)
    assets.sort((a, b) => (a.path < b.path ? -1 : 1))
  }

  let fonts: ManifestFont[] = []
  let map = project?.pkg.solidrt?.fonts
  if (map && typeof map === "object" && !Array.isArray(map)) {
    for (let [alias, value] of Object.entries(map)) {
      if (typeof value !== "string") continue
      let rel = relative(assetsDir, resolve(projectDir, value))
      if (rel.startsWith("..") || isAbsolute(rel)) {
        fail(`"solidrt.fonts": "${alias}": ${value} must live under assets/ (fonts ship as version assets)`)
      }
      let path = "assets/" + rel.split(sep).join("/")
      if (!assets.some((a) => a.path === path)) {
        fail(`"solidrt.fonts": "${alias}": no such file: ${resolve(projectDir, value)}`)
      }
      fonts.push({ path, alias })
    }
  }
  return { assets, fonts }
}

// Resolve the app identity for a pack. All three fields are guaranteed
// non-empty and 255 bytes max (the trailer encoding's length prefix).
export function loadAppIdentity(sourcePath: string): AppIdentity {
  let project = findProjectPackage(sourcePath)
  let config = project?.pkg.solidrt ?? {}
  let fallbackName = project?.pkg.name ?? basename(sourcePath).replace(/\.[jt]sx?$/, "")

  for (let key of ["appId", "org", "displayName"]) {
    if (key in config && typeof config[key] !== "string") fail(`"solidrt": "${key}" must be a string`)
  }

  let appId: string
  let defaulted = typeof config.appId !== "string"
  if (defaulted) {
    appId = sanitizeAppId(fallbackName)
  } else {
    appId = config.appId
    if (!APP_ID_PATTERN.test(appId) || appId === "." || appId === "..") {
      fail(`"solidrt": "appId" must match ${APP_ID_PATTERN} (reverse-DNS recommended, e.g. "com.example.app")`)
    }
  }
  let displayName = config.displayName ?? (project?.pkg.name || fallbackName)
  let org = config.org ?? displayName
  checkField(appId, '"appId"')
  checkField(displayName, '"displayName"')
  checkField(org, '"org"')
  return { appId, org, displayName, defaulted }
}
