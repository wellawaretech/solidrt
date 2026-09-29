// @solidrt/core/textures under bun: the entry is runtime-free (this import
// is the proof), isKtx2 sniffs the file, and the texture settings an app
// declares in package.json (solidrt.textures) read as defaults where
// nothing is said and a named error where something is wrong.
import { expect, test } from "bun:test"
import { DEFAULT_TEXTURE_SETTINGS, isKtx2, textureSettings } from "../src/textures.ts"

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
})

test("a kind overrides its fields and leaves the rest at the defaults", () => {
  let settings = textureSettings({ solidrt: { textures: { data: { codec: "etc1s", quality: 0.6 }, color: { quality: 0.5 } } } })
  expect(settings.data).toEqual({ codec: "etc1s", quality: 0.6 })
  expect(settings.color).toEqual({ codec: DEFAULT_TEXTURE_SETTINGS.color.codec, quality: 0.5 })
  expect(settings.normal).toEqual(DEFAULT_TEXTURE_SETTINGS.normal)
})

test("reading settings does not change the defaults", () => {
  textureSettings({ solidrt: { textures: { normal: { quality: 0.1 } } } })
  expect(DEFAULT_TEXTURE_SETTINGS.normal.quality).toBe(0.9)
})

test("what is wrong throws naming the key", () => {
  let read = (textures: unknown) => () => textureSettings({ solidrt: { textures } })
  expect(read([])).toThrow("solidrt.textures must be an object")
  expect(read({ roughness: {} })).toThrow("solidrt.textures.roughness: unknown texture kind")
  expect(read({ data: "uastc" })).toThrow("solidrt.textures.data must be an object")
  expect(read({ data: { codec: "astc" } })).toThrow('solidrt.textures.data.codec: unknown codec "astc"')
  expect(read({ data: { quality: 1.5 } })).toThrow("solidrt.textures.data.quality: 1.5 is not a number in 0..1")
  expect(read({ data: { quality: "high" } })).toThrow('solidrt.textures.data.quality: "high" is not a number in 0..1')
  expect(read({ data: { srgb: true } })).toThrow("solidrt.textures.data.srgb: unknown field")
})
