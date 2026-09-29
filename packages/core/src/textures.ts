// @solidrt/core/textures - what a texture bake and a loader share, without
// the runtime: this entry imports nothing the runtime provides, so a bake
// script under bun and an extension's pure modules use it as it is
// (tests/textures.test.ts imports it under bun and fails the moment a
// runtime import creeps in). Any extension that bakes or loads textures
// builds on this, the first-party 3d one included.
//
// isKtx2 tells a compressed texture file from a PNG or JPEG.
//
// textureSettings reads how an app's textures are compressed at bake
// time: the `textures` group of the `solidrt` key in its package.json.
//
//   "solidrt": {
//     "textures": {
//       "compress": true,
//       "data": { "codec": "uastc", "quality": 0.9 },
//       "files": [
//         { "match": "assets/sponza/textures/lion_*.png", "quality": 0.95 },
//         { "match": ["assets/ui/**", "assets/hero.glb#lut"], "compress": false }
//       ]
//     }
//   }
//
// Three levels, the narrower one winning: the app (`compress`), a kind of
// texture (`compress`, `codec`, `quality`), and the entries of `files`,
// each naming files by glob pattern and setting the same three for them.
// Whatever is left out keeps what the level above says, and a null is a
// key left out. Anything unknown throws naming the key.
//
// A pattern is matched against a file's path from the project root,
// written with "/". A texture that is a file of its own is that file; one
// embedded in another file (an image inside a .glb) is that file, and
// "#name" after the pattern picks it by its name there. The pattern
// language is matchesGlob's (flux:path), which the caller passes in, as
// it passes the scan (glob, flux:fs): this entry stays without a runtime.

// The first twelve bytes of every KTX2 file.
const KTX2_MAGIC = [0xab, 0x4b, 0x54, 0x58, 0x20, 0x32, 0x30, 0xbb, 0x0d, 0x0a, 0x1a, 0x0a]

/** Whether `bytes` starts as a KTX2 file, the compressed texture
 * `transcodeTexture` takes: what tells one from a PNG or JPEG. */
export function isKtx2(bytes: Uint8Array): boolean {
  return bytes.length >= KTX2_MAGIC.length && KTX2_MAGIC.every((b, i) => bytes[i] === b)
}

/** The codecs a texture is encoded with: `encodeTexture`'s (flux:image),
 * named here so that this entry needs no runtime type. "etc1s" is the
 * small one, "uastc" the accurate one. */
export type TextureCodec = "etc1s" | "uastc"

/**
 * What the texels of a texture ARE, which decides how it may be
 * compressed. Not what draws it: an extension maps its own uses (a
 * material's slots, a sprite sheet, a terrain's layers) onto these.
 *
 * - `color`: color as the eye sees it, sRGB-encoded. A photo, a painted
 *   surface, a sprite, a tile set.
 * - `normal`: directions, unit vectors packed into the channels. An error
 *   tilts the lighting computed from them.
 * - `data`: any other linear values that weigh a computation and tolerate
 *   a small error: a mask, a roughness, a height for parallax.
 *
 * Values that must come back EXACT are none of these and are not block
 * compressed at all, at any setting: a lookup table, a distance field, an
 * index map. The same goes for sharp art and text. An entry of `files`
 * with `compress: false` keeps such a texture as its file.
 */
export type TextureKind = "color" | "normal" | "data"

export type TextureSetting = {
  /** Whether the texture is compressed at all. False keeps its file as it
   * is, and `codec` and `quality` say nothing then. */
  compress: boolean
  codec: TextureCodec
  /** 0..1, higher is larger and closer to the source. */
  quality: number
}

/** One entry of `files`: the files it names and what it sets for them. */
export type TextureFileRule = {
  /** The patterns, any of which names a file of this entry. */
  match: string[]
  compress?: boolean
  codec?: TextureCodec
  quality?: number
}

export type TextureSettings = { readonly [kind in TextureKind]: Readonly<TextureSetting> } & {
  /** The entries in the order written; where several name one file, a
   * later entry's fields win over an earlier one's. */
  readonly files: readonly Readonly<TextureFileRule>[]
}

/** What a texture is to the settings: the file holding it, as its path
 * from the project root, and for a texture embedded in that file its name
 * there. */
export type TextureSource = { file: string; name?: string }

/** Whether a path matches a glob pattern: `matchesGlob` of flux:path. */
export type GlobMatcher = (path: string, pattern: string) => boolean

/** The files a glob pattern matches, from the project root: `glob` of
 * flux:fs. */
export type GlobScan = (pattern: string) => Promise<string[]>

const KINDS: TextureKind[] = ["color", "normal", "data"]
const CODECS: TextureCodec[] = ["etc1s", "uastc"]

// What parts a pattern that picks an embedded texture: the file before it,
// the name after it.
const NAME_MARK = "#"

// Textures are compressed unless the app says otherwise: a quarter of the
// GPU memory and a faster load are what an app that says nothing wants.
const DEFAULT_COMPRESS = true

// The defaults, by what each kind tolerates. Color takes the small codec
// (ETC1S moves little across its scale, so it sits high). Directions show
// their errors as blotches in the lighting, and data is often several
// unrelated values packed into the channels of one image, which the small
// codec handles badly: both take the accurate one, at the quality below
// which it gives up detail for almost no bytes. This is what the Khronos
// KTX guide and glTF-Transform recommend. The accurate codec is the larger
// download; an app that prefers the smaller one says so in its settings.
const SMALL_CODEC_QUALITY = 0.75
const ACCURATE_CODEC_QUALITY = 0.9

function frozen(settings: { [kind in TextureKind]: TextureSetting }, files: TextureFileRule[]): TextureSettings {
  for (let kind of KINDS) Object.freeze(settings[kind])
  for (let rule of files) {
    Object.freeze(rule.match)
    Object.freeze(rule)
  }
  return Object.freeze({ ...settings, files: Object.freeze(files) })
}

/** The settings of an app that declares none. Frozen, like every
 * settings object this entry hands out. */
export const DEFAULT_TEXTURE_SETTINGS: TextureSettings = frozen(
  {
    color: { compress: DEFAULT_COMPRESS, codec: "etc1s", quality: SMALL_CODEC_QUALITY },
    normal: { compress: DEFAULT_COMPRESS, codec: "uastc", quality: ACCURATE_CODEC_QUALITY },
    data: { compress: DEFAULT_COMPRESS, codec: "uastc", quality: ACCURATE_CODEC_QUALITY },
  },
  [],
)

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value)
}

// The fields a kind and an entry share, read into `into`. `key` names the
// group in a message; `others` are the group's further fields, which the
// caller reads.
function readFields(group: Record<string, unknown>, key: string, into: Partial<TextureSetting>, others: string[]) {
  for (let [field, given] of Object.entries(group)) {
    if (given === null || others.includes(field)) continue
    let name = `${key}.${field}`
    if (field === "compress") {
      if (typeof given !== "boolean") throw new Error(`${name}: ${JSON.stringify(given)} is not true or false`)
      into.compress = given
    } else if (field === "codec") {
      if (!CODECS.includes(given as TextureCodec)) throw new Error(`${name}: unknown codec ${JSON.stringify(given)} (expected "etc1s" or "uastc")`)
      into.codec = given as TextureCodec
    } else if (field === "quality") {
      if (typeof given !== "number" || !(given >= 0 && given <= 1)) throw new Error(`${name}: ${JSON.stringify(given)} is not a number in 0..1`)
      into.quality = given
    } else {
      throw new Error(`${name}: unknown field (expected ${[...others, "compress", "codec", "quality"].join(", ")})`)
    }
  }
}

// A pattern as its file part and, after the mark, its name part. A mark
// inside a set ("[#]") is a character of the file's name.
function splitPattern(pattern: string): { file: string; name?: string } {
  for (let i = 0; i < pattern.length; i++) {
    let c = pattern[i]
    if (c === "[") {
      // A "]" right after the opening, or after the negation, is a member.
      let first = pattern[i + 1] === "!" ? i + 2 : i + 1
      let close = pattern.indexOf("]", first + 1)
      if (close < 0) break
      i = close
    } else if (c === NAME_MARK) {
      return { file: pattern.slice(0, i), name: pattern.slice(i + 1) }
    }
  }
  return { file: pattern }
}

function readRule(given: unknown, key: string): TextureFileRule {
  if (!isRecord(given)) throw new Error(`${key} must be an object with match and what it sets`)
  let match = typeof given.match === "string" ? [given.match] : given.match
  if (!Array.isArray(match) || match.length === 0 || !match.every((pattern) => typeof pattern === "string" && pattern !== "")) {
    throw new Error(`${key}.match must be a pattern or a list of patterns`)
  }
  for (let pattern of match as string[]) {
    let { file, name } = splitPattern(pattern)
    if (file === "" || name === "") {
      throw new Error(`${key}.match: ${JSON.stringify(pattern)} must name a file before "${NAME_MARK}" and a texture after it`)
    }
  }
  let rule: TextureFileRule = { match: match as string[] }
  readFields(given, key, rule, ["match"])
  if (rule.compress === undefined && rule.codec === undefined && rule.quality === undefined) {
    throw new Error(`${key} sets nothing (expected compress, codec or quality)`)
  }
  if (rule.compress === false && (rule.codec !== undefined || rule.quality !== undefined)) {
    throw new Error(`${key}: compress is false, which leaves codec and quality nothing to apply to`)
  }
  return rule
}

/**
 * The texture settings of a parsed package.json: its `solidrt.textures`
 * group over the defaults. A package.json without the group (or none at
 * all: pass undefined) gives the defaults.
 *
 * `run.compress` stands in for the app's own `compress` for this one run
 * (a bake's `--compress` and `--no-compress`); what a kind or an entry
 * says about itself still holds.
 *
 * Throws on an unknown key or field, a codec that does not exist, a
 * quality outside 0..1, an entry without a pattern or that sets nothing.
 * The patterns themselves are read by the matcher: `unmatchedTextureFiles`
 * runs every one of them, so a bake that starts with it meets a malformed
 * pattern before it encodes anything.
 */
export function textureSettings(packageJson: unknown, run: { compress?: boolean } = {}): TextureSettings {
  let solidrt = isRecord(packageJson) ? packageJson.solidrt : undefined
  let group = isRecord(solidrt) ? (solidrt.textures ?? undefined) : undefined
  if (group !== undefined && !isRecord(group)) {
    throw new Error("solidrt.textures must be an object (compress, files, and the texture kinds color, normal, data)")
  }
  let given: Record<string, unknown> = group ?? {}

  let app: Partial<TextureSetting> = {}
  if (given.compress !== undefined && given.compress !== null) {
    if (typeof given.compress !== "boolean") throw new Error(`solidrt.textures.compress: ${JSON.stringify(given.compress)} is not true or false`)
    app.compress = given.compress
  }
  let compress = run.compress ?? app.compress ?? DEFAULT_COMPRESS

  let kinds = {} as { [kind in TextureKind]: TextureSetting }
  for (let kind of KINDS) kinds[kind] = { ...DEFAULT_TEXTURE_SETTINGS[kind], compress }
  let files: TextureFileRule[] = []
  for (let [key, value] of Object.entries(given)) {
    if (key === "compress" || value === null) continue
    let name = `solidrt.textures.${key}`
    if (key === "files") {
      if (!Array.isArray(value)) throw new Error(`${name} must be a list of entries, each with match and what it sets`)
      files = value.map((rule, i) => readRule(rule, `${name}[${i}]`))
    } else if (KINDS.includes(key as TextureKind)) {
      if (!isRecord(value)) throw new Error(`${name} must be an object with compress, codec and quality`)
      readFields(value, name, kinds[key as TextureKind], [])
    } else {
      throw new Error(`${name}: unknown key (expected compress, files, or a texture kind: color, normal, data)`)
    }
  }
  return frozen(kinds, files)
}

// Whether a pattern names the source: its file part matches the file, and
// its name part, when it has one, the name of a texture embedded there.
function patternNames(pattern: string, source: TextureSource, matches: GlobMatcher): boolean {
  let { file, name } = splitPattern(pattern)
  if (!matches(source.file, file)) return false
  return name === undefined || (source.name !== undefined && matches(source.name, name))
}

/**
 * How one texture is compressed: what its kind says, under the fields of
 * every entry of `files` that names it, in the order written. A pattern
 * without a name part names a file and everything embedded in it; one
 * with a name part names the embedded textures of that name, never a
 * texture that is a file of its own.
 *
 * @param matches  `matchesGlob` of flux:path.
 */
export function textureSettingFor(settings: TextureSettings, kind: TextureKind, source: TextureSource, matches: GlobMatcher): TextureSetting {
  let setting: TextureSetting = { ...settings[kind] }
  for (let rule of settings.files) {
    if (!rule.match.some((pattern) => patternNames(pattern, source, matches))) continue
    if (rule.compress !== undefined) setting.compress = rule.compress
    if (rule.codec !== undefined) setting.codec = rule.codec
    if (rule.quality !== undefined) setting.quality = rule.quality
  }
  return setting
}

/**
 * The patterns of `files`, as written, whose file matches nothing in the
 * project: a misspelled pattern, which would otherwise do nothing and say
 * nothing. The settings are the app's and a bake sees one file of it, so
 * the question goes to the project's files, not to what is being baked.
 * A bake calls this first and fails on what it returns.
 *
 * @param scan  `(pattern) => glob(pattern, { cwd })` of flux:fs, with the
 *              project root as cwd. It throws on a malformed pattern.
 */
export async function unmatchedTextureFiles(settings: TextureSettings, scan: GlobScan): Promise<string[]> {
  let unmatched: string[] = []
  for (let rule of settings.files) {
    for (let pattern of rule.match) {
      let found = await scan(splitPattern(pattern).file)
      if (found.length === 0) unmatched.push(pattern)
    }
  }
  return unmatched
}

/**
 * The patterns of `files`, as written, that pick an embedded texture of
 * `file` by a name none of its textures has. What `unmatchedTextureFiles`
 * cannot see, since it needs the file opened: the bake of that file calls
 * this with the names it found and fails on what it returns.
 *
 * @param file     The file being baked, as its path from the project root.
 * @param names    The names of the textures embedded in it.
 * @param matches  `matchesGlob` of flux:path.
 */
export function unmatchedTextureNames(settings: TextureSettings, file: string, names: string[], matches: GlobMatcher): string[] {
  let unmatched: string[] = []
  for (let rule of settings.files) {
    for (let pattern of rule.match) {
      let split = splitPattern(pattern)
      if (split.name === undefined || !matches(file, split.file)) continue
      if (!names.some((name) => matches(name, split.name!))) unmatched.push(pattern)
    }
  }
  return unmatched
}
