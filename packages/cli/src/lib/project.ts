import { existsSync, readFileSync, readdirSync, statSync } from "node:fs"
import { basename, dirname, isAbsolute, join, relative, resolve, sep } from "node:path"
import { fail } from "./util"

// Project configuration lives in the `solidrt` key of the project's
// package.json (okf/plans/client-storage-updates.md):
//
//   "solidrt": {
//     "appId": "com.example.app",   // stable identity: storage dir, Android package id
//     "org": "Example",             // optional display metadata (publisher)
//     "displayName": "Example App", // optional display metadata (launcher/window)
//     "fonts": { ... },             // see fonts.ts
//     "icon": "./assets/icon.svg",  // optional app icon (SVG, under assets/);
//                                   // an undeclared assets/icon.svg is picked
//                                   // up by convention. A .png also feeds the
//                                   // Android launcher icon (pack --apk)
//     "iconBackground": "#ffffff",  // Android adaptive-icon background color
//     "versionCode": 1              // Android update ordering (pack --apk);
//                                   // versionName comes from the package
//                                   // "version" field
//   }
//
// Everything defaults from the package name (or the entry filename when there
// is no project) so a dev project needs zero config; `srt pack` warns
// when appId is defaulted, since a distributed app should pin its identity.
//
// A null value means "unset": the scaffold ships the keys as visible nulls
// (JSON has no comments, so the file itself is the discovery surface) and a
// null behaves exactly like the absent key - defaults, and the defaulted-
// appId warning, included.
//
// The key is read in one place, loadProject, which checks every field's
// shape (throw-in-dev policy: a bad value fails the command); the readers
// (mode.ts, fonts.ts, and the identity and asset collectors below) take
// their fields from the result and only check what is specific to them.
//
// Which project an entry belongs to is the caller's decision (mode.ts: the
// cwd, never a search); the one exception is `srt check`, which verifies
// trees of entries from one cwd and walks up from each (findProject).

/** The `solidrt` key: every field optional, every present field shape-checked. */
export type ProjectConfig = {
  entry?: string
  appId?: string
  org?: string
  displayName?: string
  /**
   * alias -> font file path, false to drop a role default, or true to keep
   * one (the scaffold's explicit "on"; fonts.ts).
   */
  fonts?: Record<string, string | boolean>
  icon?: string
  /** Android adaptive-icon background color, "#rrggbb" (apk.ts). */
  iconBackground?: string
  /** Android versionCode: a positive integer that must only ever grow. */
  versionCode?: number
}

export type Project = { dir: string; name: string | undefined; version: string | undefined; config: ProjectConfig }

function parseProjectConfig(raw: unknown): ProjectConfig {
  if (raw === undefined) return {}
  if (typeof raw !== "object" || raw === null || Array.isArray(raw)) fail('"solidrt" in package.json must be an object')
  let config = raw as Record<string, unknown>
  // Null = unset (see the header note): stripped here so every reader sees
  // the same absent key it would for an omitted one.
  for (let key of ["entry", "appId", "org", "displayName", "icon", "iconBackground", "fonts", "versionCode"]) {
    if (key in config && config[key] === null) delete config[key]
  }
  for (let key of ["entry", "appId", "org", "displayName", "icon"]) {
    if (key in config && typeof config[key] !== "string") fail(`"solidrt": "${key}" must be a string`)
  }
  if ("iconBackground" in config && !(typeof config.iconBackground === "string" && /^#[0-9a-fA-F]{6}$/.test(config.iconBackground))) {
    fail('"solidrt": "iconBackground" must be a "#rrggbb" color')
  }
  // Android caps versionCode at 2100000000; enforcing it here keeps every
  // packed artifact installable.
  if ("versionCode" in config) {
    let code = config.versionCode
    if (typeof code !== "number" || !Number.isInteger(code) || code < 1 || code > 2100000000) {
      fail('"solidrt": "versionCode" must be an integer between 1 and 2100000000')
    }
  }
  if ("fonts" in config) {
    let fonts = config.fonts
    if (typeof fonts !== "object" || fonts === null || Array.isArray(fonts)) {
      fail('The "solidrt.fonts" key in package.json must be a map of alias to font file path (or true/false)')
    }
    for (let [alias, value] of Object.entries(fonts)) {
      if (typeof value !== "boolean" && typeof value !== "string") {
        fail(`"solidrt.fonts": "${alias}" must be a font file path, true or false, got ${JSON.stringify(value)}`)
      }
      if (Buffer.byteLength(alias, "utf8") > 255) fail(`"solidrt.fonts": alias "${alias}" is too long (max 255 bytes)`)
    }
  }
  return config as ProjectConfig
}

// The project at `dir`: its package name and validated `solidrt` config. A
// dir without a package.json is an empty project; null (file mode: the entry
// stands alone) stays null.
export function loadProject(dir: string | null): Project | null {
  if (dir === null) return null
  let pkgPath = resolve(dir, "package.json")
  let pkg = existsSync(pkgPath) ? JSON.parse(readFileSync(pkgPath, "utf8")) : {}
  return {
    dir,
    name: typeof pkg.name === "string" ? pkg.name : undefined,
    version: typeof pkg.version === "string" ? pkg.version : undefined,
    config: parseProjectConfig(pkg.solidrt),
  }
}

// The nearest project above a source file (see the note on searching above).
export function findProject(sourcePath: string): Project | null {
  let dir = resolve(dirname(sourcePath))
  while (true) {
    if (existsSync(resolve(dir, "package.json"))) return loadProject(dir)
    let parent = dirname(dir)
    if (parent === dir) return null
    dir = parent
  }
}

export type AppIdentity = { appId: string; org: string; displayName: string; defaulted: boolean }

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
// The manifest's runtimeVersion: a manually bumped constant until the
// derivation question is settled (see plan).
export const RUNTIME_VERSION = 1

// This CLI's version, the one value every "which srt is this" answer comes
// from (--version, the manifest stamp, the MCP server). A published CLI
// carries the real version in its package.json; in-repo that is the 0.0.0
// placeholder (see CLAUDE.md, "Versioning"), so a checkout reports the same
// git describe the runtime builds stamp themselves with (lattice/Makefile,
// flux/build.rs). Falls back to the placeholder when git cannot answer.
function cliVersion(): string {
  let pkgVersion = JSON.parse(readFileSync(join(import.meta.dir, "..", "..", "package.json"), "utf8")).version
  if (pkgVersion !== "0.0.0") return pkgVersion
  let git = Bun.spawnSync({
    cmd: ["git", "describe", "--tags", "--always", "--dirty"],
    cwd: import.meta.dir,
    stdout: "pipe",
    stderr: "ignore",
  })
  return git.success ? git.stdout.toString().trim().replace(/^v/, "") : pkgVersion
}
export const CLI_VERSION: string = cliVersion()

// The manifest's solidrtVersion: provenance, not a compat gate like
// runtimeVersion - the CLI release (or checkout) that built the version. The
// packed runner warns when it differs from its own (lattice/src/main.rs).
export const SOLIDRT_VERSION: string = CLI_VERSION

// `extra` are build outputs that ship as assets too (isolate bundles); they
// follow the assets/ tree in the list, in the order given.
export function buildManifest(code: string, entry: string, extra: ManifestAsset[], projectDir: string | null): string {
  let identity = loadAppIdentity(entry, projectDir)
  let sha256 = new Bun.CryptoHasher("sha256").update(code).digest("hex")
  let { assets, fonts, icon } = collectAssets(projectDir)
  assets.push(...extra)
  return JSON.stringify({
    appId: identity.appId,
    displayName: identity.displayName,
    runtimeVersion: RUNTIME_VERSION,
    solidrtVersion: SOLIDRT_VERSION,
    ...(icon ? { icon } : {}),
    bundle: { path: "bundle.js", sha256, size: Buffer.byteLength(code, "utf8") },
    ...(assets.length ? { assets } : {}),
    ...(fonts.length ? { fonts } : {}),
  })
}

export type ManifestAsset = { path: string; sha256: string; size: number }
export type ManifestFont = { path: string; alias: string }

/** The manifest entry for in-memory asset bytes at `path`. */
export function manifestAssetFor(path: string, bytes: Uint8Array): ManifestAsset {
  return { path, sha256: new Bun.CryptoHasher("sha256").update(bytes).digest("hex"), size: bytes.length }
}

// The manifest asset path for an absolute file inside the project's assets/
// dir, or null when it lies outside it.
export function assetPathFor(projectDir: string, abs: string): string | null {
  let rel = relative(resolve(projectDir, "assets"), abs)
  if (rel.startsWith("..") || isAbsolute(rel)) return null
  return "assets/" + rel.split(sep).join("/")
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
// manifest bytes are deterministic. Fonts and the icon are annotations
// pointing into that set: `solidrt.fonts` path entries and `solidrt.icon`
// must live under assets/ so they reach dev clients and the version store
// (`false` font entries only drop pack defaults and have no manifest
// presence). The icon is SVG-only for now: the launcher renders SVG natively,
// and the raster surfaces (window icon, OS embedding) come with later stages
// (okf/backlog/app-icons.md). An undeclared assets/icon.svg is picked up by
// convention.
export function collectAssets(dir: string | null): {
  assets: ManifestAsset[]
  fonts: ManifestFont[]
  icon: string | null
} {
  let project = loadProject(dir)
  // No project (file mode): the entry stands alone, so no assets at all.
  if (!project) return { assets: [], fonts: [], icon: null }
  let projectDir = project.dir
  let assetsDir = resolve(projectDir, "assets")

  let assets: ManifestAsset[] = []
  if (existsSync(assetsDir) && statSync(assetsDir).isDirectory()) {
    walkAssets(assetsDir, assetsDir, assets)
    assets.sort((a, b) => (a.path < b.path ? -1 : 1))
  }

  let fonts: ManifestFont[] = []
  for (let [alias, value] of Object.entries(project.config.fonts ?? {})) {
    // Booleans toggle role defaults (fonts.ts), which are not project assets.
    if (typeof value === "boolean") continue
    let path = assetPathFor(projectDir, resolve(projectDir, value))
    if (!path) {
      fail(`"solidrt.fonts": "${alias}": ${value} must live under assets/ (fonts ship as version assets)`)
    }
    if (!assets.some((a) => a.path === path)) {
      fail(`"solidrt.fonts": "${alias}": no such file: ${resolve(projectDir, value)}`)
    }
    fonts.push({ path, alias })
  }

  let icon: string | null = null
  let declared = project.config.icon
  if (declared !== undefined) {
    let path = assetPathFor(projectDir, resolve(projectDir, declared))
    if (!path) {
      fail(`"solidrt.icon": ${declared} must live under assets/ (the icon ships as a version asset)`)
    }
    if (!path.toLowerCase().endsWith(".svg")) {
      fail(`"solidrt.icon": ${declared} must be an .svg file`)
    }
    if (!assets.some((a) => a.path === path)) {
      fail(`"solidrt.icon": no such file: ${resolve(projectDir, declared)}`)
    }
    icon = path
  } else if (assets.some((a) => a.path === "assets/icon.svg")) {
    icon = "assets/icon.svg"
  }

  return { assets, fonts, icon }
}

// Resolve the app identity for a pack. All three fields are guaranteed
// non-empty and 255 bytes max (the trailer encoding's length prefix).
export function loadAppIdentity(sourcePath: string, projectDir: string | null): AppIdentity {
  let project = loadProject(projectDir)
  let config = project?.config ?? {}
  // A scoped package name (@org/name) defaults to its last segment: identity
  // fields reject path separators, and derived defaults must never fail that.
  let fallbackName = (project?.name ?? basename(sourcePath).replace(/\.[jt]sx?$/, "")).split("/").pop()!

  let defaulted = config.appId === undefined
  let appId = config.appId ?? sanitizeAppId(fallbackName)
  if (!defaulted && (!APP_ID_PATTERN.test(appId) || appId === "." || appId === "..")) {
    fail(`"solidrt": "appId" must match ${APP_ID_PATTERN} (reverse-DNS recommended, e.g. "com.example.app")`)
  }
  let displayName = config.displayName ?? fallbackName
  let org = config.org ?? displayName
  checkField(appId, '"appId"')
  checkField(displayName, '"displayName"')
  checkField(org, '"org"')
  return { appId, org, displayName, defaulted }
}
