// uploadTexture's two forms against a running GPU: a whole-frame upload
// (an offset selecting the frame in a multi-frame buffer) and a rect
// upload writing one region of the texture in place (GL's texSubImage2D
// with an offset: what a runtime atlas writes a cell with), read back
// through readTexture; a rect outside the texture or of the wrong size
// throws and changes nothing.

import { test, expect } from "@solidrt/test"
import { createMutableTexture, readTexture, uploadTexture } from "@solidrt/core/gpu"

const SIDE = 4
// The rect written into the texture: a 2x2 at (1, 1).
const RECT = { x: 1, y: 1, width: 2, height: 2 }

function solid(width: number, height: number, rgba: number[]): Uint8Array {
  let data = new Uint8Array(width * height * 4)
  for (let i = 0; i < width * height; i++) data.set(rgba, i * 4)
  return data
}

function pixel(texture: number, x: number, y: number): number[] {
  let { width, data } = readTexture(texture)
  let at = (y * width + x) * 4
  return [data[at]!, data[at + 1]!, data[at + 2]!, data[at + 3]!]
}

test("a rect upload writes its region in place and leaves the rest", async app => {
  let texture = 0
  await app.mount(() => {
    texture = createMutableTexture(solid(SIDE, SIDE, [0, 0, 255, 255]), SIDE, SIDE, { label: "upload-rect" })
    return <texture src={texture} width={SIDE} height={SIDE} />
  })
  uploadTexture(texture, solid(RECT.width, RECT.height, [255, 0, 0, 255]), RECT)
  await app.frame()
  expect(pixel(texture, 0, 0)).toEqual([0, 0, 255, 255])
  expect(pixel(texture, RECT.x, RECT.y)).toEqual([255, 0, 0, 255])
  expect(pixel(texture, RECT.x + 1, RECT.y + 1)).toEqual([255, 0, 0, 255])
  expect(pixel(texture, RECT.x + 2, RECT.y)).toEqual([0, 0, 255, 255])
  // The whole-frame form with an offset picks the second frame of two.
  let two = new Uint8Array(SIDE * SIDE * 4 * 2)
  two.set(solid(SIDE, SIDE, [0, 255, 0, 255]), 0)
  two.set(solid(SIDE, SIDE, [255, 255, 0, 255]), SIDE * SIDE * 4)
  uploadTexture(texture, two, SIDE * SIDE * 4)
  await app.frame()
  expect(pixel(texture, 0, 0)).toEqual([255, 255, 0, 255])
  expect(pixel(texture, RECT.x, RECT.y)).toEqual([255, 255, 0, 255])
})

test("a rect outside the texture or of the wrong size throws and changes nothing", async app => {
  let texture = 0
  await app.mount(() => {
    texture = createMutableTexture(solid(SIDE, SIDE, [0, 0, 255, 255]), SIDE, SIDE, { label: "upload-rect-bad" })
    return <texture src={texture} width={SIDE} height={SIDE} />
  })
  expect(() => uploadTexture(texture, solid(2, 2, [255, 0, 0, 255]), { x: 3, y: 3, width: 2, height: 2 })).toThrow()
  expect(() => uploadTexture(texture, solid(2, 2, [255, 0, 0, 255]), { x: 0, y: 0, width: 3, height: 2 })).toThrow()
  expect(() => uploadTexture(texture, solid(2, 2, [255, 0, 0, 255]), { x: -1, y: 0, width: 2, height: 2 })).toThrow()
  await app.frame()
  expect(pixel(texture, 3, 3)).toEqual([0, 0, 255, 255])
})
