// The runtime-free entry (src/model-data.ts, published as
// @solidrt/3d/model) under bun test: a bake script's whole surface with
// no flux:* shim, so a runtime import creeping into the chain fails here
// rather than in an app's next bake. Builds a two-part ModelData from the
// geometry kit (a shared part with a placement, a plain one), round-trips
// it through the container and checks the app-data slots - extras on the
// root, a node and a part, named blobs - come back deep-equal and the
// blobs aligned for typed views.
import { expect, test } from "bun:test"
import * as entry from "../src/model-data.ts"
import type { ModelData } from "../src/model-data.ts"

test("the model entry loads without the runtime", async () => {
  // The published subpath resolves to the same module as the file.
  let published = await import("@solidrt/3d/model")
  expect(published.encodeModel).toBe(entry.encodeModel)
  expect(published.parseGltf).toBe(entry.parseGltf)
})

test("a bake of own geometry round-trips through the container", () => {
  let crate = entry.box({ width: 1, height: 1, depth: 1 })
  let slab = entry.box({ width: 4, height: 0.2, depth: 4 })
  let identity = { rotation: [0, 0, 0, 1] as [number, number, number, number], scale: [1, 1, 1] as [number, number, number] }
  let heights = new Float32Array([0.5, 1.5, 2.5])
  let data: ModelData = {
    nodes: [
      { name: "level", parent: null, position: [0, 0, 0], ...identity, extras: { kind: "level" } },
      { name: "crate", parent: 0, position: [1, 0, 0], ...identity },
      { name: "crate2", parent: 0, position: [3, 0, 0], ...identity },
      { name: "slab", parent: 0, position: [0, -1, 0], ...identity },
    ],
    parts: [
      { name: "crate", node: 1, placements: [2], skin: null, material: 0, geometry: crate, extras: { collider: "box" } },
      { name: "slab", node: 3, skin: null, material: 0, geometry: slab },
    ],
    skins: [],
    clips: [],
    materials: [
      {
        name: "paint",
        color: [1, 0.5, 0, 1],
        map: null,
        doubleSided: false,
        transparent: false,
        alphaMode: "OPAQUE",
        alphaCutoff: 0.5,
        normalMap: null,
        normalScale: 1,
        emissive: [0, 0, 0],
        emissiveIntensity: 1,
        emissiveMap: null,
        metalness: 0,
        roughness: 1,
        metalnessRoughnessMap: null,
        extras: { surface: "matte" },
      },
    ],
    images: [],
    bounds: new Float32Array([-2, -1.1, -2, 3.5, 0.5, 2]),
    extras: { spawn: [1, 2, 3], name: "hangar" },
    blobs: { grid: new Uint8Array([1, 2, 3, 4, 5]), heights: new Uint8Array(heights.buffer) },
  }
  let back = entry.decodeModel(entry.encodeModel(data))
  expect(back.nodes).toEqual(data.nodes)
  expect(back.materials).toEqual(data.materials)
  expect(back.parts.map((p) => ({ name: p.name, node: p.node, placements: p.placements, extras: p.extras }))).toEqual([
    { name: "crate", node: 1, placements: [2], extras: { collider: "box" } },
    { name: "slab", node: 3, placements: undefined, extras: undefined },
  ])
  expect(Array.from(back.parts[0]!.geometry.indices)).toEqual(Array.from(crate.indices))
  expect(entry.vertexBytes(back.parts[1]!.geometry.vertices).join()).toBe(entry.vertexBytes(slab.vertices).join())
  expect(Array.from(back.bounds)).toEqual(Array.from(data.bounds))
  expect(back.extras).toEqual(data.extras)
  expect(Array.from(back.blobs!.grid!)).toEqual([1, 2, 3, 4, 5])
  // Blocks are 4-aligned: a float view sits on the decoded bytes directly.
  let backHeights = back.blobs!.heights!
  expect(backHeights.byteOffset % 4).toBe(0)
  expect(Array.from(new Float32Array(backHeights.buffer, backHeights.byteOffset, 3))).toEqual([0.5, 1.5, 2.5])
})
