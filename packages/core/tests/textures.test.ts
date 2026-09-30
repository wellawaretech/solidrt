// @solidrt/core/textures under srt test: the entry imports no gui or srt:
// module (this import is the proof: the bare flux binary has neither),
// isKtx2 sniffs the file, and the texture settings an app
// declares in package.json (solidrt.textures) read as defaults where
// nothing is said and a named error where something is wrong. Which
// setting a texture gets is decided with the matcher passed in; here that
// is the runtime's own.
import { expect, test } from "flux:test"
import { matchesGlob } from "flux:path"
import { DEFAULT_TEXTURE_SETTINGS, isKtx2, textureSettingFor, textureSettings, unmatchedTextureFiles, unmatchedTextureNames } from "../src/textures.ts"

let read = (textures: unknown, run?: { compress?: boolean }) => textureSettings({ solidrt: { textures } }, run)

test("isKtx2 goes by the first twelve bytes", () => {
  let ktx2 = new Uint8Array([0xab, 0x4b, 0x54, 0x58, 0x20, 0x32, 0x30, 0xbb, 0x0d, 0x0a, 0x1a, 0x0a, 1, 2])
  let png = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 0, 0, 0])
  expect(isKtx2(ktx2)).toBe(true)
  expect(isKtx2(png)).toBe(false)
  expect(isKtx2(ktx2.subarray(0, 11))).toBe(false)
})

test("no package.json, no solidrt key and no textures group give the defaults", () => {
  expect(textureSettings(undefined)).toEqual(DEFAULT_TEXTURE_SETTINGS)
  expect(textureSettings({ name: "app" })).toEqual(DEFAULT_TEXTURE_SETTINGS)
  expect(textureSettings({ solidrt: { appId: "com.example.app" } })).toEqual(DEFAULT_TEXTURE_SETTINGS)
  expect(read(null)).toEqual(DEFAULT_TEXTURE_SETTINGS)
})

test("an app that says nothing compresses its textures", () => {
  let settings = textureSettings(undefined)
  expect(settings.color).toEqual({ compress: true, codec: "etc1s", quality: 0.75 })
  expect(settings.normal).toEqual({ compress: true, codec: "uastc", quality: 0.9 })
  expect(settings.data).toEqual({ compress: true, codec: "uastc", quality: 0.9 })
  expect(settings.files).toEqual([])
})

test("a kind overrides its fields and leaves the rest at the defaults", () => {
  let settings = read({ data: { codec: "etc1s", quality: 0.6 }, color: { quality: 0.5 } })
  expect(settings.data).toEqual({ compress: true, codec: "etc1s", quality: 0.6 })
  expect(settings.color).toEqual({ compress: true, codec: DEFAULT_TEXTURE_SETTINGS.color.codec, quality: 0.5 })
  expect(settings.normal).toEqual(DEFAULT_TEXTURE_SETTINGS.normal)
})

test("compress is the app's, and a kind's own word wins over it", () => {
  let off = read({ compress: false, normal: { compress: true } })
  expect([off.color.compress, off.normal.compress, off.data.compress]).toEqual([false, true, false])
  let on = read({ data: { compress: false } })
  expect([on.color.compress, on.normal.compress, on.data.compress]).toEqual([true, true, false])
})

test("a run's compress stands in for the app's, not for a kind's", () => {
  let forced = read({ compress: false, data: { compress: false } }, { compress: true })
  expect([forced.color.compress, forced.normal.compress, forced.data.compress]).toEqual([true, true, false])
  let skipped = read({ normal: { compress: true } }, { compress: false })
  expect([skipped.color.compress, skipped.normal.compress, skipped.data.compress]).toEqual([false, true, false])
})

test("a null is a key left out", () => {
  let settings = read({ compress: null, color: null, data: { codec: null, quality: 0.5 }, files: null })
  expect(settings.color).toEqual(DEFAULT_TEXTURE_SETTINGS.color)
  expect(settings.data).toEqual({ compress: true, codec: "uastc", quality: 0.5 })
  expect(settings.files).toEqual([])
})

test("an entry of files takes one pattern or several", () => {
  let settings = read({
    files: [
      { match: "assets/lion_*.png", codec: "uastc", quality: 0.95 },
      { match: ["assets/ui/**", "assets/hero.glb#lut"], compress: false },
    ],
  })
  expect(settings.files).toEqual([
    { match: ["assets/lion_*.png"], codec: "uastc", quality: 0.95 },
    { match: ["assets/ui/**", "assets/hero.glb#lut"], compress: false },
  ])
})

test("settings cannot be written to, the defaults included", () => {
  let settings = read({ normal: { quality: 0.1 }, files: [{ match: "a.png", quality: 1 }] })
  expect(DEFAULT_TEXTURE_SETTINGS.normal.quality).toBe(0.9)
  let write = (target: object, key: string, value: unknown) => () => {
    ;(target as Record<string, unknown>)[key] = value
  }
  expect(write(DEFAULT_TEXTURE_SETTINGS.normal, "quality", 0.1)).toThrow(TypeError)
  expect(write(DEFAULT_TEXTURE_SETTINGS, "color", {})).toThrow(TypeError)
  expect(write(settings.normal, "quality", 0.5)).toThrow(TypeError)
  expect(write(settings.files[0]!, "quality", 0.5)).toThrow(TypeError)
  expect(() => (settings.files as unknown[]).push({})).toThrow(TypeError)
  expect(() => (settings.files[0]!.match as unknown[]).push("b.png")).toThrow(TypeError)
})

test("what is wrong throws naming the key", () => {
  let wrong = (textures: unknown) => () => read(textures)
  expect(wrong([])).toThrow("solidrt.textures must be an object")
  expect(wrong({ roughness: {} })).toThrow("solidrt.textures.roughness: unknown key")
  expect(wrong({ compress: "yes" })).toThrow('solidrt.textures.compress: "yes" is not true or false')
  expect(wrong({ data: "uastc" })).toThrow("solidrt.textures.data must be an object")
  expect(wrong({ data: { codec: "astc" } })).toThrow('solidrt.textures.data.codec: unknown codec "astc"')
  expect(wrong({ data: { codec: "none" } })).toThrow('solidrt.textures.data.codec: unknown codec "none"')
  expect(wrong({ data: { quality: 1.5 } })).toThrow("solidrt.textures.data.quality: 1.5 is not a number in 0..1")
  expect(wrong({ data: { quality: "high" } })).toThrow('solidrt.textures.data.quality: "high" is not a number in 0..1')
  expect(wrong({ data: { compress: 1 } })).toThrow("solidrt.textures.data.compress: 1 is not true or false")
  expect(wrong({ data: { srgb: true } })).toThrow("solidrt.textures.data.srgb: unknown field (expected compress, codec, quality)")
  expect(wrong({ data: { match: "a.png" } })).toThrow("solidrt.textures.data.match: unknown field")
})

test("what is wrong in an entry throws naming the entry", () => {
  let wrong = (...files: unknown[]) => () => read({ files })
  expect(() => read({ files: { "a.png": { quality: 1 } } })).toThrow("solidrt.textures.files must be a list of entries")
  expect(wrong("a.png")).toThrow("solidrt.textures.files[0] must be an object")
  expect(wrong({ match: "a.png", quality: 1 }, { quality: 1 })).toThrow("solidrt.textures.files[1].match must be a pattern or a list of patterns")
  expect(wrong({ match: [], quality: 1 })).toThrow("solidrt.textures.files[0].match must be a pattern")
  expect(wrong({ match: ["a.png", ""], quality: 1 })).toThrow("solidrt.textures.files[0].match must be a pattern")
  expect(wrong({ match: ["a.png", 7], quality: 1 })).toThrow("solidrt.textures.files[0].match must be a pattern")
  expect(wrong({ match: "a.png" })).toThrow("solidrt.textures.files[0] sets nothing")
  expect(wrong({ match: "a.png", codec: null })).toThrow("solidrt.textures.files[0] sets nothing")
  expect(wrong({ match: "a.png", files: "b.png", quality: 1 })).toThrow("solidrt.textures.files[0].files: unknown field (expected match, compress, codec, quality)")
  expect(wrong({ match: "a.png", quality: 2 })).toThrow("solidrt.textures.files[0].quality: 2 is not a number in 0..1")
  expect(wrong({ match: "a.png", compress: false, codec: "uastc" })).toThrow("solidrt.textures.files[0]: compress is false")
  expect(wrong({ match: "a.png", compress: false, quality: 1 })).toThrow("solidrt.textures.files[0]: compress is false")
  expect(wrong({ match: "hero.glb#", quality: 1 })).toThrow('solidrt.textures.files[0].match: "hero.glb#" must name a file')
  expect(wrong({ match: "#lut", quality: 1 })).toThrow('solidrt.textures.files[0].match: "#lut" must name a file')
})

test("a texture gets what its kind says, under the entries that name it", () => {
  let settings = read({
    color: { quality: 0.6 },
    files: [
      { match: "assets/sponza/textures/lion_*.png", codec: "uastc", quality: 0.95 },
      { match: ["assets/ui/**", "assets/luts/*.png"], compress: false },
    ],
  })
  let get = (kind: "color" | "normal" | "data", file: string) => textureSettingFor(settings, kind, { file }, matchesGlob)
  expect(get("color", "assets/sponza/textures/curtain.png")).toEqual({ compress: true, codec: "etc1s", quality: 0.6 })
  expect(get("normal", "assets/sponza/textures/curtain_n.png")).toEqual({ compress: true, codec: "uastc", quality: 0.9 })
  expect(get("color", "assets/sponza/textures/lion_head.png")).toEqual({ compress: true, codec: "uastc", quality: 0.95 })
  expect(get("color", "assets/other/textures/lion_head.png")).toEqual({ compress: true, codec: "etc1s", quality: 0.6 })
  expect(get("color", "assets/ui/dark/icon.png").compress).toBe(false)
  expect(get("data", "assets/luts/grade.png").compress).toBe(false)
  // What comes back is the caller's own.
  get("color", "assets/ui/icon.png").quality = 0
  expect(settings.color.quality).toBe(0.6)
})

test("a later entry wins, field by field", () => {
  let settings = read({
    files: [
      { match: "assets/**", codec: "uastc", quality: 0.8 },
      { match: "assets/hero/*", quality: 1 },
      { match: "assets/hero/lut.png", compress: false },
      { match: "assets/plain/**", compress: false },
      { match: "assets/plain/keep/*", compress: true },
    ],
  })
  let get = (file: string) => textureSettingFor(settings, "color", { file }, matchesGlob)
  expect(get("assets/a.png")).toEqual({ compress: true, codec: "uastc", quality: 0.8 })
  expect(get("assets/hero/face.png")).toEqual({ compress: true, codec: "uastc", quality: 1 })
  expect(get("assets/hero/lut.png")).toEqual({ compress: false, codec: "uastc", quality: 1 })
  expect(get("assets/plain/a.png").compress).toBe(false)
  expect(get("assets/plain/keep/a.png")).toEqual({ compress: true, codec: "uastc", quality: 0.8 })
})

test("a name after the file picks a texture embedded in it", () => {
  let settings = read({
    files: [
      { match: "assets/hero.glb", quality: 0.8 },
      { match: "assets/hero.glb#lut*", compress: false },
      { match: "assets/*.glb#face", quality: 1 },
      { match: "assets/odd[#]name.png", quality: 0.5 },
    ],
  })
  let get = (file: string, name?: string) => textureSettingFor(settings, "color", { file, name }, matchesGlob)
  // The file alone names everything embedded in it.
  expect(get("assets/hero.glb", "body").quality).toBe(0.8)
  expect(get("assets/hero.glb", "lut_warm")).toEqual({ compress: false, codec: "etc1s", quality: 0.8 })
  expect(get("assets/hero.glb", "face").quality).toBe(1)
  expect(get("assets/villain.glb", "face").quality).toBe(1)
  expect(get("assets/villain.glb", "body").quality).toBe(0.75)
  // A texture that is a file of its own has no name to be picked by.
  expect(get("assets/hero.glb").quality).toBe(0.8)
  // A mark inside a set is a character of the file's name.
  expect(get("assets/odd#name.png").quality).toBe(0.5)
})

test("a pattern no file of the project matches is reported as written", async () => {
  let project = ["assets/hero.glb", "assets/textures/lion_head.png", "assets/ui/icon.png"]
  let asked: string[] = []
  let scan = async (pattern: string) => {
    asked.push(pattern)
    return project.filter((file) => matchesGlob(file, pattern))
  }
  let settings = read({
    files: [
      { match: ["assets/textures/lion_*.png", "assets/textures/tiger_*.png"], quality: 1 },
      { match: "assets/hero.glb#lut", compress: false },
      { match: "assets/villain.glb#lut", compress: false },
      { match: "assets/ui/**", compress: false },
    ],
  })
  expect(await unmatchedTextureFiles(settings, scan)).toEqual(["assets/textures/tiger_*.png", "assets/villain.glb#lut"])
  // The scan is asked for files: a name part never reaches it.
  expect(asked).toEqual(["assets/textures/lion_*.png", "assets/textures/tiger_*.png", "assets/hero.glb", "assets/villain.glb", "assets/ui/**"])
  expect(await unmatchedTextureFiles(DEFAULT_TEXTURE_SETTINGS, scan)).toEqual([])
})

test("a name no texture of the baked file has is reported as written", () => {
  let settings = read({
    files: [
      { match: ["assets/hero.glb#lut", "assets/hero.glb#fcae"], quality: 1 },
      { match: "assets/*.glb#body", quality: 1 },
      { match: "assets/villain.glb#cape", quality: 1 },
      { match: "assets/hero.glb", quality: 1 },
    ],
  })
  let unmatched = (file: string, names: string[]) => unmatchedTextureNames(settings, file, names, matchesGlob)
  expect(unmatched("assets/hero.glb", ["lut", "face", "body"])).toEqual(["assets/hero.glb#fcae"])
  // Another file's entries are not this bake's to judge.
  expect(unmatched("assets/villain.glb", ["cape", "body"])).toEqual([])
  expect(unmatched("assets/villain.glb", [])).toEqual(["assets/*.glb#body", "assets/villain.glb#cape"])
  expect(unmatched("assets/prop.gltf", [])).toEqual([])
})
