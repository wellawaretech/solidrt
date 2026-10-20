// The GPU side of shared geometry (src/geometry-gpu.ts): geometries over
// one vertex array upload it once, and geometries over one MorphTargets
// upload one morph texture - what makes the glTF reader's and the
// container's sharing reach the GPU. Buffers and textures, so an app test.

import { test, expect } from "@solidrt/test"
import { box, packMorphTargets } from "../src/index.ts"
import type { Geometry } from "../src/index.ts"
import { acquireGeometryBuffers, releaseGeometryBuffers } from "../src/geometry-gpu.ts"

test("geometries over one vertex array and one morph pack share the buffer and the texture", async app => {
  await app.mount(() => <view width={8} height={8} />)
  let cube = box({ width: 1, height: 1, depth: 1 })
  let count = cube.indices.length === 0 ? 0 : Math.max(...Array.from(cube.indices)) + 1
  let morphs = packMorphTargets(count, [{ name: "puff", position: new Float32Array(count * 3).fill(0.1), normal: null }])
  let whole: Geometry = { vertices: cube.vertices, indices: cube.indices, layout: cube.layout, morphs }
  let half: Geometry = { vertices: cube.vertices, indices: cube.indices.slice(0, 18), layout: cube.layout, morphs }

  let a = acquireGeometryBuffers(whole)
  let b = acquireGeometryBuffers(half)
  expect(b.buffers[0]).toBe(a.buffers[0])
  expect(b.index).not.toBe(a.index)
  expect(a.morphs).not.toBe(null)
  expect(b.morphs!.texture).toBe(a.morphs!.texture)
  expect(b.morphs!.width).toBe(a.morphs!.width)

  // Releasing one keeps the shared uploads for the other: a third acquire
  // of the first geometry after its release finds them where they were.
  releaseGeometryBuffers(a)
  await app.frame()
  let again = acquireGeometryBuffers(whole)
  expect(again.buffers[0]).toBe(b.buffers[0])
  expect(again.morphs!.texture).toBe(b.morphs!.texture)
  releaseGeometryBuffers(again)
  releaseGeometryBuffers(b)
})
