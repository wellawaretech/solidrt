// The picking contract the docs state in more than one place (AGENTS.md
// Picking and layers, scene.ts pick()/Hit/setLayers), pinned against a
// running scene so the copies cannot drift from the runtime again (a Traps
// entry once claimed box-only picking long after the narrowphase went
// triangle-accurate): an ordinary mesh picks per triangle, an instanced
// mesh by its box or per instance, pick and raycast cast the same ray,
// layer masks, the { meshes } include-list, and updateVertices moving the
// pick with the mesh. The scene is GPU state, so this is an app test. A
// query flushes the pending writes, so frames are run only around the
// vertex update, which needs the geometry on the GPU and a flush after.

import { test, expect } from "@solidrt/core/test"
import type { TestApp } from "@solidrt/core/test"
import { glsl } from "@solidrt/core/gpu"
import {
  add,
  addInstance,
  box,
  createInstancedMesh,
  createMesh,
  createRecordMesh,
  createScene,
  geometryAttribute,
  geometryVertexCount,
  mergeGeometries,
  setLayers,
  setTransform,
  shaderMaterialClass,
  transformGeometry,
  unlit,
  updateVertices,
} from "../src/index.ts"
import type { Vec3 } from "../src/index.ts"

const SIZE = 128
// Decimals a distance or normal is compared to.
const DIGITS = 3
// How far the deforming cube's vertices are carried, clear of its rest box.
const LIFT = 6

let down: Vec3 = [0, 0, -1]
let rounded = (v: readonly number[]) => v.map(x => Number(x.toFixed(DIGITS)) + 0)

function world() {
  let scene = createScene(SIZE, SIZE, { clearColor: [0.07, 0.07, 0.1, 1], label: "raycast" })
  scene.setCamera({ fov: 55, position: [0, 0, 10], target: [0, 0, 0] })
  let grey = unlit({ color: [0.5, 0.5, 0.5] })

  // Two unit cubes merged into ONE geometry with a 2-wide gap between
  // them: the merged local box spans the gap, so a ray through the middle
  // separates triangle testing from box testing.
  let pair = mergeGeometries([transformGeometry(box(), { position: [-2, 0, 0] }), transformGeometry(box(), { position: [2, 0, 0] })])
  let merged = createMesh(pair, grey)
  add(scene.root, merged)

  // Box-only tier: an instanced mesh with explicit population bounds
  // (records are opaque to picking, so the box is the whole story).
  let look = shaderMaterialClass({
    vertex: glsl`
      in vec3 aPos;
      in vec3 iPos;
      uniform mat4 uModel;
      uniform mat4 uViewProj;

      void main() {
        gl_Position = uViewProj * uModel * vec4(aPos + iPos, 1.0);
      }
    `,
    fragment: glsl`
      void main() {
        fragColor = vec4(0.8, 0.4, 0.2, 1.0);
      }
    `,
    instanceBuffers: [{ attributes: [{ name: "iPos", format: "float32x3" }] }],
    label: "raycast-instanced",
  })
  let instanced = createRecordMesh(box(), look.instance(), new Float32Array([0, 0, 0]), 1, {
    bounds: [-0.5, -0.5, -0.5, 0.5, 0.5, 0.5],
  })
  setTransform(instanced, { position: [0, 3, 0] })
  add(scene.root, instanced)

  // Per-instance tier: an instanced mesh with explicit bounds. The
  // instances are the leaves that pick; the mesh's own box (the bounds,
  // in the index for culling) is not a hit target.
  let population = createInstancedMesh(box(), unlit({ color: [0.5, 0.5, 0.5], instanced: true }), { bounds: [-3, -0.5, -0.5, 3, 0.5, 0.5] })
  setTransform(population, { position: [0, 6, 0] })
  add(scene.root, population)
  let leftInstance = addInstance(population, { position: [-2, 0, 0] })
  addInstance(population, { position: [2, 0, 0] })

  // The undrawn collision stand-in: layer 2, outside the scene mask (1).
  let collision = createMesh(box(), grey)
  setTransform(collision, { position: [0, -3, 0] })
  setLayers(collision, 2)
  add(scene.root, collision)

  // The deforming mesh: a cube whose vertices are carried up in place.
  let deforming = createMesh(box(), grey)
  setTransform(deforming, { position: [3, -3, 0] })
  add(scene.root, deforming)

  return { scene, merged, instanced, population, leftInstance, collision, deforming }
}

// The scene and its meshes own GPU resources, so they are created under
// the mounted app's root.
async function mounted(app: TestApp) {
  let w!: ReturnType<typeof world>
  await app.mount(() => {
    w = world()
    return <texture src={w.scene.texture} width={SIZE} height={SIZE} />
  })
  return w
}

test("an ordinary mesh picks per triangle: hits carry face, uv and normal, and a ray through a gap inside its box misses", async app => {
  let { scene, merged } = await mounted(app)
  let solid = scene.raycast([-2, 0, 10], down)
  expect(solid.length).toBe(1)
  let hit = solid[0]!
  expect(hit.mesh).toBe(merged)
  expect(hit.face).not.toBe(undefined)
  expect(hit.uv).not.toBe(undefined)
  expect(rounded(hit.normal)).toEqual([0, 0, 1])
  expect(rounded([hit.distance])).toEqual([9.5])
  expect(scene.raycast([0, 0, 10], down).length).toBe(0)
})

test("an instanced mesh is box-only: its hit carries the struck face's normal and neither face nor uv", async app => {
  let { scene, instanced } = await mounted(app)
  let hits = scene.raycast([0, 3, 10], down)
  expect(hits.length).toBe(1)
  let hit = hits[0]!
  expect(hit.mesh).toBe(instanced)
  expect(hit.face).toBe(undefined)
  expect(hit.uv).toBe(undefined)
  expect(rounded(hit.normal)).toEqual([0, 0, 1])
})

test("an instanced mesh with bounds picks per instance and never by its own box", async app => {
  let { scene, population, leftInstance } = await mounted(app)
  let hits = scene.raycast([-2, 6, 10], down)
  expect(hits.length).toBe(1)
  expect(hits[0]!.mesh).toBe(population)
  expect(hits[0]!.instance).toBe(leftInstance)
  expect(hits[0]!.face).not.toBe(undefined)
  // Through the population box, between the instances.
  expect(scene.raycast([0, 6, 10], down).length).toBe(0)
})

test("pick and raycast cast the same ray: the same nearest mesh under a projected point", async app => {
  let { scene, merged } = await mounted(app)
  let px = scene.project([-2, 0, 0])
  expect(px).not.toBeNull()
  let picked = scene.pick(px!.x, px!.y)[0]
  let { origin, direction } = scene.screenRay(px!.x, px!.y)
  let rayed = scene.raycast(origin, direction)[0]
  expect(picked?.mesh).toBe(merged)
  expect(rayed?.mesh).toBe(merged)
})

test("a scene-masked-out mesh is skipped like an invisible one, unless the query passes its own layers", async app => {
  let { scene, collision } = await mounted(app)
  expect(scene.raycast([0, -3, 10], down).length).toBe(0)
  let hits = scene.raycast([0, -3, 10], down, { layers: 2 })
  expect(hits.length).toBe(1)
  expect(hits[0]!.mesh).toBe(collision)
  expect(hits[0]!.face).not.toBe(undefined)
})

test("{ meshes } is an include-list: hits only from the listed meshes", async app => {
  let { scene, merged } = await mounted(app)
  expect(scene.raycast([0, 3, 10], down, { meshes: [merged] }).length).toBe(0)
})

test("updateVertices moves the pick with the mesh: gone from the rest pose, triangle-accurate where the vertices went", async app => {
  let { scene, deforming } = await mounted(app)
  expect(scene.raycast([3, -3, 10], down).length).toBe(1)
  // The geometry has to be on the GPU for an update to reach it: the
  // first frame uploads it.
  await app.frame()
  let pos = geometryAttribute(deforming.geometry, "aPos")!
  let count = geometryVertexCount(deforming.geometry, "raycast test")
  for (let i = 0; i < count; i++) pos.set(i, 1, pos.get(i, 1) + LIFT)
  updateVertices(deforming.geometry)
  // The box follows at the next flush, the triangle index at the next query.
  await app.frame()
  expect(scene.raycast([3, -3, 10], down).length).toBe(0)
  let moved = scene.raycast([3, -3 + LIFT, 10], down)
  expect(moved.length).toBe(1)
  expect(moved[0]!.mesh).toBe(deforming)
  expect(moved[0]!.face).not.toBe(undefined)
  expect(rounded([moved[0]!.distance])).toEqual([9.5])
})
