// flux:image: PNG and JPEG round trips, validation, and (where the runtime
// has the encoder) compressed textures.
import { expect, test } from "flux:test"
import { decodeImage, encodeImage, encodeTexture, transcodeTexture } from "flux:image"

test("a PNG round-trips premultiplied pixels", () => {
  // Both calls default to premultiplied alpha, so the pixels handed in must
  // be valid premultiplied ones (no channel above its alpha) to survive the
  // trip.
  let pixels = new Uint8Array([255, 0, 0, 255, 0, 128, 0, 128])
  let png = encodeImage({ data: pixels, width: 2, height: 1 })
  let back = decodeImage(png)
  expect([back.width, back.height]).toEqual([2, 1])
  expect(back.data).toEqual(pixels)
})

test("a PNG round-trips straight pixels", () => {
  // Straight alpha on both ends is what a PNG file stores, so it is
  // byte-exact.
  let pixels = new Uint8Array([255, 0, 0, 255, 0, 255, 0, 128])
  let png = encodeImage({ data: pixels, width: 2, height: 1 }, { alpha: "straight" })
  let back = decodeImage(png, { alpha: "straight" })
  expect([back.width, back.height]).toEqual([2, 1])
  expect(back.data).toEqual(pixels)
})

test("a JPEG decodes opaque with the same dimensions", () => {
  let pixels = new Uint8Array([255, 0, 0, 255, 0, 255, 0, 128])
  // An explicit undefined options argument is normal JS for "no options".
  let png = encodeImage({ data: pixels, width: 2, height: 1 }, undefined)
  let jpg = encodeImage(decodeImage(png), { format: "jpeg", quality: 0.8 })
  let back = decodeImage(jpg)
  expect([back.width, back.height, back.data.length, back.data[3], back.data[7]]).toEqual([2, 1, 8, 255, 255])
})

test("bad input throws", () => {
  let img = { data: new Uint8Array(8), width: 2, height: 1 }
  expect(() => decodeImage(new Uint8Array([1, 2, 3]))).toThrow(/^decodeImage:/)
  expect(() => encodeImage({ data: new Uint8Array(8), width: 3, height: 1 })).toThrow("3x1")
  // @ts-expect-error not a format; the call has to say so at runtime too
  expect(() => encodeImage(img, { format: "tiff" })).toThrow('unknown format "tiff"')
  expect(() => encodeImage(img, { quality: 1.5 })).toThrow("out of range")
  // @ts-expect-error a string is no pixel data; the call has to say so at runtime too
  expect(() => encodeImage({ data: "nope", width: 2, height: 1 })).toThrow("img.data")
})

test("the image capability is listed", () => {
  expect(Flux.capabilities.includes("image")).toBe(true)
})

// Compressed textures need the encoder, which a build may leave out
// (`Flux.capabilities` lacks "ktx2" then).
if (Flux.capabilities.includes("ktx2")) {
  test("a texture encodes and transcodes", async () => {
    // An 8x8 image encodes to a KTX2 with its four-level chain, and
    // transcodes to the block formats (7 blocks of 16 bytes) and to rgba8
    // (85 pixels) with the sRGB flag carried by the file.
    let data = new Uint8Array(8 * 8 * 4)
    for (let i = 0; i < data.length; i += 4) data.set([200, 100, 50, 255], i)
    let file = await encodeTexture({ data, width: 8, height: 8 }, { codec: "etc1s", srgb: true, mipmap: true })
    let magic = Array.from(file.slice(1, 7))
      .map((c) => String.fromCharCode(c))
      .join("")
    expect(magic).toBe("KTX 20")
    let shape = async (target: "etc2-rgba8" | "bc7-rgba8" | "rgba8") => {
      let t = await transcodeTexture(file, { target })
      return [t.format, t.width, t.height, t.mipmap, t.data.length, t.data instanceof Uint8Array]
    }
    expect(await shape("etc2-rgba8")).toEqual(["etc2-rgba8-srgb", 8, 8, true, 112, true])
    expect(await shape("bc7-rgba8")).toEqual(["bc7-rgba8-srgb", 8, 8, true, 112, true])
    expect(await shape("rgba8")).toEqual(["rgba8-srgb", 8, 8, true, 340, true])
  })

  test("the texture calls refuse bad input", async () => {
    // The argument checks throw at the call: a headless runtime has no GPU
    // to pick a target for, a target has to be a known one, the codec is
    // required. A file that is not a KTX2 and pixels short of the size
    // reject.
    let png = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a])
    expect(() => transcodeTexture(png)).toThrow("transcodeTexture: this runtime has no GPU")
    await expect(transcodeTexture(png, { target: "etc2-rgba8" })).rejects.toThrow("transcodeTexture: not a KTX2 file")
    // @ts-expect-error not a target; the call has to say so at runtime too
    expect(() => transcodeTexture(png, { target: "astc" })).toThrow("transcodeTexture: unknown target 'astc'")
    // @ts-expect-error the codec is required; the call has to say so at runtime too
    expect(() => encodeTexture({ data: new Uint8Array(4), width: 1, height: 1 }, {})).toThrow("encodeTexture: codec is required")
    await expect(encodeTexture({ data: new Uint8Array(4), width: 2, height: 2 }, { codec: "uastc" })).rejects.toThrow(
      "encodeTexture: expected 16 bytes",
    )
  })
}
