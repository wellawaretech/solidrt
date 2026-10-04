#!/usr/bin/env bun

// The notices a distributed SolidRT binary owes: the license texts of
// everything compiled or bundled into it that is not ours. Written next to
// the binaries in a platform package (THIRD-PARTY-NOTICES.txt), generated
// by the dist goals so it describes the build it ships with.
//
//   bun scripts/build-third-party-notices.ts --target <rust triple> --out <file>
//
// Two sources:
//
// - Rust crates: the crates the runtime LINKS, which is what `cargo tree`
//   lists over normal dependency edges for the target, from lattice and
//   flux at the dist features (build scripts and test helpers ship
//   nothing, and `cargo metadata` alone lists optional dependencies no
//   feature turns on), each with the license files its package carries. A crate without one is listed under
//   its license expression, with the standard text.
// - Native libraries and assets, from the table below: what those crates
//   and our own build compile in or bundle, which no crate metadata
//   describes. Adding a vendored library, a bundled font or a prebuilt
//   means adding a row here; the script fails on a row whose file is gone.
//
// - JavaScript libraries, from the table below: what the apps built into
//   the runtime (the player, the error screen) bundle. An app a developer
//   packs bundles its own dependencies on top; those are that app's to
//   credit.
//
// Identical texts are printed once, with everything they cover above them.

import { createHash } from "node:crypto"
import { existsSync, readdirSync, readFileSync, writeFileSync } from "node:fs"
import { dirname, join, resolve } from "node:path"
import { fileURLToPath } from "node:url"

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "..")

// The cargo features a dist build links, per root package: what
// lattice/Makefile and flux/Makefile pass for the `dist` goals (speech is
// off in every dist build).
const ROOTS: { manifest: string; features: string }[] = [
  { manifest: "lattice/Cargo.toml", features: "go,video,ktx2,flux/compile" },
  { manifest: "flux/Cargo.toml", features: "compile,ktx2" },
]

// Files in a package that carry license or notice text.
const LICENSE_FILE = /^(licen[cs]e|copying|notice|unlicense|copyright|patents)/i

// The width of the rules between entries.
const RULE_WIDTH = 78

type Native = {
  name: string
  /** What it is and how it gets into the binary. */
  what: string
  /** License files, relative to the repo root or (`crate`) to a crate's
   * source folder. */
  files: string[]
  /** The crate whose source folder `files` are in: the one that carries
   * the library's sources, often a build dependency of the one that
   * links it. */
  crate?: string
  /** The crate that links it: the row counts only where that crate is
   * linked. Always, when absent. */
  linkedBy?: string
  /** The targets it ships on; every target when absent. */
  only?: (target: string) => boolean
}

const desktop = (target: string) => !target.includes("android")
const angle = (target: string) => target.includes("windows") || target.includes("apple")

const NATIVE: Native[] = [
  { name: "SDL", what: "windowing, input and audio devices; built from source", crate: "sdl3-src", linkedBy: "sdl3-sys", files: ["SDL/LICENSE.txt"] },
  { name: "SDL_mixer", what: "audio mixing and decoding; built from source", crate: "sdl3-mixer-src", linkedBy: "sdl3-mixer-sys", files: ["SDL_mixer/LICENSE.txt"] },
  { name: "SDL_image", what: "image loading for SDL; built from source", crate: "sdl3-image-src", linkedBy: "sdl3-image-sys", files: ["SDL_image/LICENSE.txt"] },
  { name: "SDL_ttf", what: "font rendering for SDL; built from source", crate: "sdl3-ttf-src", linkedBy: "sdl3-ttf-sys", files: ["SDL_ttf/LICENSE.txt"] },
  {
    name: "Impeller (Flutter engine)",
    what: "the 2D renderer; a prebuilt library of the Flutter SDK, whose license file covers the engine and everything it bundles",
    files: ["third-party-licenses/impeller/LICENSE"],
  },
  { name: "ANGLE", what: "OpenGL ES over the platform's graphics API; prebuilt libEGL and libGLESv2 shipped beside the binary", files: ["third-party-licenses/angle/LICENSE"], only: angle },
  { name: "QuickJS", what: "the JavaScript engine; built from source", crate: "rquickjs-sys", linkedBy: "rquickjs-sys", files: ["quickjs/LICENSE"] },
  { name: "Basis Universal", what: "compressed texture transcoder and encoder; built from source", files: ["forge/vendor/basis_universal/LICENSE", "forge/vendor/basis_universal/NOTICE"] },
  { name: "Zstandard", what: "decompression inside Basis Universal; built from source", files: ["forge/vendor/basis_universal/zstd/LICENSE"] },
  { name: "libvpx", what: "VP9 video decoder; built from source", files: ["forge/vendor/libvpx/LICENSE", "forge/vendor/libvpx/PATENTS"], only: desktop },
  { name: "Opus", what: "audio decoder; built from source", files: ["forge/vendor/opus/COPYING"] },
  {
    name: "libc++",
    what: "the C++ runtime of the Android NDK; libc++_shared.so shipped in the APK",
    files: ["third-party-licenses/libcxx/LICENSE.TXT"],
    only: (target) => target.includes("android"),
  },
  { name: "Noto fonts", what: "Noto Sans, Noto Serif and Noto Sans Mono; embedded", files: ["alloy/assets/fonts/OFL.txt"] },
]

// The npm packages bundled into the built-in apps, and the folders they
// may be installed in (the workspace root, or the package that depends on
// them).
const BUNDLED_JS = ["solid-js", "@solidjs/signals", "qrcode-generator"]
const JS_ROOTS = ["node_modules", "packages/components/node_modules", "packages/core/node_modules", "apps/player/node_modules"]

// Compiled in, with no license to reproduce: their authors placed them in
// the public domain.
const PUBLIC_DOMAIN = [
  "SQLite (https://www.sqlite.org/copyright.html), built from source",
  "stb_vorbis by Sean Barrett (https://github.com/nothings/stb), the Ogg Vorbis decoder inside SDL_mixer",
]

type Package = {
  id: string
  name: string
  version: string
  license: string | null
  license_file: string | null
  repository: string | null
  manifest_path: string
  source: string | null
}

type Metadata = { packages: Package[] }

function fail(message: string): never {
  console.error("build-third-party-notices: " + message)
  process.exit(1)
}

function argument(name: string): string {
  let at = process.argv.indexOf("--" + name)
  let value = at === -1 ? undefined : process.argv[at + 1]
  if (value === undefined) fail(`Missing --${name}. Usage: --target <rust triple> --out <file>`)
  return value
}

function metadata(manifest: string, features: string, target: string): Metadata {
  let proc = Bun.spawnSync(
    ["cargo", "metadata", "--format-version", "1", "--locked", "--filter-platform", target, "--manifest-path", join(ROOT, manifest), "--features", features],
    { stdout: "pipe", stderr: "pipe" },
  )
  if (proc.exitCode !== 0) fail(`cargo metadata failed for ${manifest}:\n${proc.stderr.toString()}`)
  return JSON.parse(proc.stdout.toString())
}

// The packages the root at `manifest` links for the target, itself
// included, as `name version`.
function linked(manifest: string, features: string, target: string): Set<string> {
  let proc = Bun.spawnSync(
    ["cargo", "tree", "--locked", "--edges", "normal", "--prefix", "none", "--format", "{p}", "--target", target, "--manifest-path", join(ROOT, manifest), "--features", features],
    { stdout: "pipe", stderr: "pipe" },
  )
  if (proc.exitCode !== 0) fail(`cargo tree failed for ${manifest}:\n${proc.stderr.toString()}`)
  let names = new Set<string>()
  for (let line of proc.stdout.toString().split("\n")) {
    // "name v1.2.3", then the source in parentheses and a (*) on a repeat.
    let match = /^(\S+) v(\S+)/.exec(line)
    if (match !== null) names.add(`${match[1]} ${match[2]}`)
  }
  return names
}

// One license text and everything it covers. Texts are keyed by their
// content with whitespace runs collapsed, so a file that differs only in
// line endings or trailing blanks is the same text.
type Entry = { text: string; covers: string[] }

function collect(entries: Map<string, Entry>, text: string, covered: string) {
  let key = createHash("sha256").update(text.replace(/\s+/g, " ").trim()).digest("hex")
  let entry = entries.get(key)
  if (entry === undefined) entries.set(key, { text: text.replace(/\r\n/g, "\n").trimEnd(), covers: [covered] })
  else if (!entry.covers.includes(covered)) entry.covers.push(covered)
}

function rule(char: string): string {
  return char.repeat(RULE_WIDTH)
}

function section(title: string): string {
  return `${rule("=")}\n${title}\n${rule("=")}\n`
}

function render(entries: Map<string, Entry>): string {
  let sorted = [...entries.values()].sort((a, b) => a.covers[0]!.localeCompare(b.covers[0]!))
  return sorted.map((entry) => `${entry.covers.sort().join("\n")}\n\n${entry.text}\n`).join(`\n${rule("-")}\n\n`)
}

let target = argument("target")
let out = argument("out")

let crates = new Map<string, Package>()
// Every package of the build by name, build dependencies included: where
// the sources of a library are found.
let folders = new Map<string, string>()
for (let root of ROOTS) {
  let names = linked(root.manifest, root.features, target)
  for (let pkg of metadata(root.manifest, root.features, target).packages) {
    folders.set(pkg.name, dirname(pkg.manifest_path))
    // Our own crates are covered by the package's own LICENSE.
    if (pkg.source !== null && names.has(`${pkg.name} ${pkg.version}`)) crates.set(pkg.id, pkg)
  }
}
let linkedNames = new Set([...crates.values()].map((p) => p.name))

// --- native libraries and assets
let native = new Map<string, Entry>()
for (let lib of NATIVE) {
  if (lib.only !== undefined && !lib.only(target)) continue
  if (lib.linkedBy !== undefined && !linkedNames.has(lib.linkedBy)) continue
  let base = ROOT
  if (lib.crate !== undefined) {
    let folder = folders.get(lib.crate)
    if (folder === undefined) fail(`${lib.name}: ${lib.linkedBy} is linked for ${target} but the crate ${lib.crate} with its sources is not in the build; fix its row in NATIVE`)
    base = folder
  }
  let text = lib.files
    .map((file) => {
      let path = join(base, file)
      if (!existsSync(path)) fail(`${lib.name}: ${path} does not exist (a submodule not fetched, or the row in NATIVE is stale)`)
      return readFileSync(path, "utf8").trimEnd()
    })
    .join("\n\n")
  collect(native, text, `${lib.name} - ${lib.what}`)
}

// --- Rust crates
let withText = new Map<string, Entry>()
let withoutText = new Map<string, string[]>()
for (let pkg of [...crates.values()].sort((a, b) => a.name.localeCompare(b.name))) {
  let label = `${pkg.name} ${pkg.version}` + (pkg.repository ? ` (${pkg.repository})` : "")
  let folder = dirname(pkg.manifest_path)
  let files = readdirSync(folder).filter((f) => LICENSE_FILE.test(f)).sort()
  if (pkg.license === null && files.length === 0) fail(`${label} declares no license and ships no license file`)
  if (files.length === 0) {
    let list = withoutText.get(pkg.license!) ?? []
    list.push(label)
    withoutText.set(pkg.license!, list)
    continue
  }
  for (let file of files) collect(withText, readFileSync(join(folder, file), "utf8"), `${label}, ${pkg.license ?? "see text"}, ${file}`)
}

// --- JavaScript libraries
let scripts = new Map<string, Entry>()
let scriptsWithoutText: string[] = []
for (let name of BUNDLED_JS) {
  let folder = JS_ROOTS.map((root) => join(ROOT, root, name)).find((candidate) => existsSync(join(candidate, "package.json")))
  if (folder === undefined) fail(`${name} is not installed (looked in ${JS_ROOTS.join(", ")}); run bun install, or fix BUNDLED_JS`)
  let manifest = JSON.parse(readFileSync(join(folder, "package.json"), "utf8"))
  let label = `${name} ${manifest.version}, ${manifest.license}`
  let files = readdirSync(folder).filter((f) => LICENSE_FILE.test(f)).sort()
  if (files.length === 0) scriptsWithoutText.push(label)
  for (let file of files) collect(scripts, readFileSync(join(folder, file), "utf8"), `${label}, ${file}`)
}

let parts = [
  "THIRD-PARTY NOTICES\n",
  `SolidRT for ${target} contains the software listed below, each under its own\n` +
    "license. SolidRT itself is under the license in the LICENSE file next to this\n" +
    "one. Generated by scripts/build-third-party-notices.ts.\n",
  section("Native libraries and assets"),
  render(native),
  section("In the public domain"),
  PUBLIC_DOMAIN.join("\n") + "\n",
  section("JavaScript libraries in the built-in apps"),
  render(scripts),
  ...(scriptsWithoutText.length > 0
    ? ["Without a license file in the package, under the license named:\n\n" + scriptsWithoutText.map((l) => "  " + l).join("\n") + "\n"]
    : []),
  section(`Rust crates (${crates.size})`),
  render(withText),
]
if (withoutText.size > 0) {
  parts.push(section("Rust crates whose package carries no license file"))
  parts.push(
    "Each is under the license expression it is listed under; the texts of those\n" +
      "licenses are printed above, with the crates that do carry them.\n",
  )
  for (let [license, list] of [...withoutText].sort()) parts.push(`${license}\n\n${list.map((l) => "  " + l).join("\n")}\n`)
}
writeFileSync(out, parts.join("\n"))

let bytes = readFileSync(out).byteLength
console.log(
  `Wrote ${out}: ${native.size} native libraries, ${BUNDLED_JS.length} JavaScript libraries, ${crates.size} crates in ${withText.size} license texts, ` +
    `${[...withoutText.values()].reduce((n, l) => n + l.length, 0)} crates without a file, ${(bytes / 1024).toFixed(0)} KiB`,
)
