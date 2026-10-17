// A splat cloud on the GPU: the record block parsed at bake scale, handed
// to the engine as its key column (the centers: what the depth sort reads
// and all the engine keeps of the records) and uploaded in place as the
// record texture, then painted back to front - the probe is the frame
// itself (`app.painted`), as records.test.tsx reads it. Two opaque splats
// over the view's center: the nearer one is drawn last, so its color is
// the pixel; the camera crossing to the other side re-sorts the engine's
// key column and the other splat wins; a count of 0 draws nothing until
// setRecordCount publishes the prefix.

import { test, expect } from "@solidrt/test"
import type { Pixels, RefLocator, TestApp } from "@solidrt/test"
import { add, createScene, createSplatMesh, disposeInstances, parseSplat, setRecordCount } from "../src/index.ts"
import type { SceneHandle, SplatMeshNode } from "../src/index.ts"

const SIZE = 64
// The camera stands this far from the origin on the z axis, either side.
const DISTANCE = 10
// The near splat sits this far toward +z of the far one (at the origin).
const NEAR_Z = 2
// A channel reads at least this to count as the splat's color: the pixel
// center sits half a pixel off the splat center, so the gaussian leaves a
// sliver of the one beneath.
const SOLID = 240
const FAINT = 16
const RED = [255, 0, 0, 255]
const BLUE = [0, 0, 255, 255]

// A .splat record: position, linear scale, color bytes, quaternion
// (w, x, y, z) quantized v * 128 + 128; the identity rotation at the
// largest w the byte holds.
const IDENTITY = [127 / 128, 0, 0, 0]
function dotSplat(splats: { p: number[]; c: number[] }[]): Uint8Array {
  let out = new Uint8Array(splats.length * 32)
  let dv = new DataView(out.buffer)
  splats.forEach((g, i) => {
    for (let k = 0; k < 3; k++) {
      dv.setFloat32(i * 32 + k * 4, g.p[k]!, true)
      dv.setFloat32(i * 32 + 12 + k * 4, 1, true)
    }
    for (let k = 0; k < 4; k++) {
      out[i * 32 + 24 + k] = g.c[k]!
      out[i * 32 + 28 + k] = Math.round(IDENTITY[k]! * 128 + 128)
    }
  })
  return out
}

function scene(): SceneHandle {
  let s = createScene(SIZE, SIZE, { blendSpace: "display", clearColor: [0, 0, 0, 1], label: "splat-mesh" })
  s.setCamera({ position: [0, 0, DISTANCE], target: [0, 0, 0] })
  return s
}

async function mounted<T extends { scene: SceneHandle }>(app: TestApp, build: () => T): Promise<T & { leaf: RefLocator }> {
  let leaf = app.ref()
  let w!: T
  await app.mount(() => {
    w = build()
    return <texture ref={leaf} src={w.scene.texture} width={SIZE} height={SIZE} />
  })
  return { ...w, leaf }
}

function center(pixels: Pixels): number[] {
  let i = ((SIZE / 2) * SIZE + SIZE / 2) * 4
  return [pixels.data[i]!, pixels.data[i + 1]!, pixels.data[i + 2]!, pixels.data[i + 3]!]
}

function expectColor(pixel: number[], color: number[]) {
  for (let k = 0; k < 3; k++) {
    if (color[k] === 255) expect(pixel[k]!).toBeGreaterThanOrEqual(SOLID)
    else expect(pixel[k]!).toBeLessThan(FAINT)
  }
  expect(pixel[3]).toBe(255)
}

test("a splat cloud paints back to front from the engine's key column, re-sorted as the camera crosses, after the count admits it", async app => {
  let { scene: s, mesh, leaf } = await mounted(app, () => {
    let s = scene()
    let data = parseSplat(dotSplat([{ p: [0, 0, 0], c: RED }, { p: [0, 0, NEAR_Z], c: BLUE }]), { keepOrientation: true })
    let mesh: SplatMeshNode = createSplatMesh(data, { count: 0, label: "splat-mesh" })
    add(s.root, mesh)
    return { scene: s, mesh }
  })
  // Nothing published at count 0.
  expect(center(await app.painted(leaf))).toEqual([0, 0, 0, 255])
  setRecordCount(mesh, 2)
  // The near (blue) splat is drawn last from +z.
  expectColor(center(await app.painted(leaf)), BLUE)
  // From -z the far (red) one is nearer: the engine re-sorts its column.
  s.setCamera({ position: [0, 0, -DISTANCE], target: [0, 0, 0] })
  expectColor(center(await app.painted(leaf)), RED)
  disposeInstances(mesh)
})
