// createCamera over a test camera: the image a test installs reaches the
// viewfinder texture, a QR code in it is decoded through the stream
// scanner like one in front of a real camera, a later install shows the
// new image, and closing the owner closes the camera.

import { test, expect } from "@solidrt/test"
import { createCamera } from "@solidrt/core/camera"
import type { CameraStream } from "@solidrt/core/camera"
import { readTexture } from "@solidrt/core/gpu"
import { listCameras, open, scanImage } from "flux:camera"
import { Show } from "solid-js"

// A version-1 QR code of "solidrt" (error correction M), one character per
// module, 21 by 21.
const QR = [
  "#######.#..##.#######",
  "#.....#.##....#.....#",
  "#.###.#..####.#.###.#",
  "#.###.#.###.#.#.###.#",
  "#.###.#...##..#.###.#",
  "#.....#..#.##.#.....#",
  "#######.#.#.#.#######",
  "........##.##........",
  "#.##.###.####.#..#.##",
  "###.#...##.#####.#..#",
  "#.#.####..##......###",
  "###.##..#..#..#.##..#",
  "##.#..#.###.#........",
  "........#..#..###.##.",
  "#######.#..##..##....",
  "#.....#.###....######",
  "#.###.#..#..###..####",
  "#.###.#.#.##..##..##.",
  "#.###.#.###.#.#..##..",
  "#.....#......#......#",
  "#######.#.#..#.##.#..",
]
// Pixels per module, and the quiet zone around the code in modules (the
// standard's four).
const MODULE = 6
const QUIET = 4
const SIDE = (QR.length + 2 * QUIET) * MODULE

function flat(width: number, height: number, rgba: number[]): { width: number; height: number; data: Uint8Array } {
  let data = new Uint8Array(width * height * 4)
  for (let i = 0; i < width * height; i++) data.set(rgba, i * 4)
  return { width, height, data }
}

// The QR code drawn black on white, a module `MODULE` pixels square.
function qrImage(): { width: number; height: number; data: Uint8Array } {
  let image = flat(SIDE, SIDE, [255, 255, 255, 255])
  for (let r = 0; r < QR.length; r++) {
    for (let c = 0; c < QR.length; c++) {
      if (QR[r]![c] !== "#") continue
      for (let y = 0; y < MODULE; y++) {
        for (let x = 0; x < MODULE; x++) {
          let px = (QUIET + c) * MODULE + x
          let py = (QUIET + r) * MODULE + y
          image.data.set([0, 0, 0, 255], (py * SIDE + px) * 4)
        }
      }
    }
  }
  return image
}

function pixel(stream: CameraStream, x: number, y: number): number[] {
  let { width, data } = readTexture(stream.texture()!)
  let at = (y * width + x) * 4
  return [data[at]!, data[at + 1]!, data[at + 2]!, data[at + 3]!]
}

// The center of module (r, c) in pixels.
function moduleCenter(r: number, c: number): [number, number] {
  return [(QUIET + c) * MODULE + MODULE / 2, (QUIET + r) * MODULE + MODULE / 2]
}

test("the installed image is the camera the app opens, and it shows in the texture", async app => {
  await app.camera(qrImage())
  expect(listCameras().map(cam => cam.id)).toEqual([0])
  let cam!: CameraStream
  await app.mount(() => {
    cam = createCamera()
    return (
      <Show when={cam.texture()}>
        {id => <texture src={id()} width={SIDE} height={SIDE} />}
      </Show>
    )
  })
  await app.settle()
  expect(cam.error()).toBe(undefined)
  expect([cam.width(), cam.height()]).toEqual([SIDE, SIDE])
  // The top-left finder pattern's corner module is dark, the quiet zone light.
  expect(pixel(cam, ...moduleCenter(0, 0))).toEqual([0, 0, 0, 255])
  expect(pixel(cam, ...moduleCenter(-2, -2))).toEqual([255, 255, 255, 255])
})

test("a QR code in the image is decoded and reaches the barcode signal", async app => {
  let image = qrImage()
  // The fixture itself decodes: a failure below is the stream path.
  expect(scanImage(image.data, image.width, image.height)).toEqual([{ data: "solidrt", format: "qr" }])
  await app.camera(image)
  let cam!: CameraStream
  await app.mount(() => {
    cam = createCamera({ scan: ["qr"] })
    return <view />
  })
  await app.settle()
  expect(cam.barcode()).toEqual({ data: "solidrt", format: "qr" })
})

test("a later install shows the new image to an open camera", async app => {
  await app.camera(flat(8, 8, [255, 0, 0, 255]))
  let cam!: CameraStream
  await app.mount(() => {
    cam = createCamera()
    return (
      <Show when={cam.texture()}>
        {id => <texture src={id()} width={8} height={8} />}
      </Show>
    )
  })
  await app.settle()
  expect(pixel(cam, 4, 4)).toEqual([255, 0, 0, 255])
  await app.camera(flat(8, 8, [0, 0, 255, 255]))
  await app.settle()
  expect(pixel(cam, 4, 4)).toEqual([0, 0, 255, 255])
})

test("open() of flux:camera resolves outside mount, with no frame driving it", async app => {
  await app.camera(flat(8, 8, [0, 255, 0, 255]))
  await app.mount(() => <view />)
  await app.settle()
  let cam = await open()
  expect([cam.width, cam.height]).toEqual([8, 8])
  expect(typeof cam.texture).toBe("number")
  cam.close()
})
