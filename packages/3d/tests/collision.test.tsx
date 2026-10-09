// The collision contract the docs state in more than one place (AGENTS.md
// Collision, scene.ts, collision.ts), pinned against a running scene so
// the copies cannot drift from the runtime: a sweep is exact, testing is
// in world space, the slide filter, layers and the { meshes } include-list,
// the box tier of an instanced mesh, and moveAndSlide's landing. The scene
// is GPU state, so this is an app test. A query flushes the pending
// writes, so no frame is run after the mount.

import { test, expect } from "@solidrt/test"
import type { TestApp } from "@solidrt/test"
import { glsl } from "@solidrt/core/gpu"
import { add, box, createRecordMesh, createMesh, createScene, moveAndSlide, setLayers, setTransform, shaderMaterialClass, unlit } from "../src/index.ts"
import type { Vec3 } from "../src/index.ts"

const SIZE = 128
// Decimals an analytic time, point or normal is compared to.
const DIGITS = 3
// The mover's default skin, what the landing gap is measured in.
const SKIN = 0.01


function world() {
  let scene = createScene(SIZE, SIZE, { clearColor: [0.07, 0.07, 0.1, 1], label: "collision" })
  scene.setCamera({ fov: 55, position: [0, 0, 10], target: [0, 0, 0] })
  let grey = unlit({ color: [0.5, 0.5, 0.5] })

  // A 2x2x2 cube at the origin: faces at +-1.
  let cube = createMesh(box({ width: 2, height: 2, depth: 2 }), grey)
  add(scene.root, cube)
  // A unit cube scaled to 4 wide along x, off to the side: faces at x = 10 +- 2.
  let wide = createMesh(box(), grey)
  setTransform(wide, { position: [10, 0, 0], scale: [4, 1, 1] })
  add(scene.root, wide)
  // The undrawn collision stand-in: layer 2, outside the scene mask (1).
  let collision = createMesh(box({ width: 2, height: 2, depth: 2 }), grey)
  setTransform(collision, { position: [0, -10, 0] })
  setLayers(collision, 2)
  add(scene.root, collision)
  // Box tier: an instanced mesh with explicit population bounds.
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
    label: "collision-instanced",
  })
  let instanced = createRecordMesh(box(), look.instance(), new Float32Array([0, 0, 0]), 1, {
    bounds: [-1, -1, -1, 1, 1, 1],
  })
  setTransform(instanced, { position: [0, 10, 0] })
  add(scene.root, instanced)
  return { scene, cube, wide, collision, instanced }
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

test("sweep is exact: the analytic time, the face normal and the touch point, and a cast that falls short misses", async app => {
  let { scene, cube } = await mounted(app)
  // The sphere's center stops at x = -1.5, t = 0.35.
  let hits = scene.sweep({ center: [-5, 0, 0], radius: 0.5 }, [10, 0, 0], { meshes: [cube] })
  expect(hits.length).toBe(1)
  let hit = hits[0]!
  expect(hit.time).toBeCloseTo(0.35, DIGITS)
  expect(hit.normal).toBeCloseTo([-1, 0, 0], DIGITS)
  expect(hit.point).toBeCloseTo([-1, 0, 0], DIGITS)
  expect(scene.sweep({ center: [-5, 0, 0], radius: 0.5 }, [3, 0, 0], { meshes: [cube] }).length).toBe(0)
})

test("testing is in world space: a scaled cube is hit where its scaled face is", async app => {
  let { scene, wide } = await mounted(app)
  // The wide cube's near face is at x = 8, not at its unit box's x = 9.5.
  let scaled = scene.sweep({ center: [3, 0, 0], radius: 0.5 }, [10, 0, 0], { meshes: [wide] })
  expect(scaled.length).toBe(1)
  expect(scaled[0]!.time).toBeCloseTo(0.45, DIGITS)
})

test("the slide filter: along a contact nothing, into it time 0 with the surface normal, away from it nothing", async app => {
  let { scene, cube } = await mounted(app)
  // A sphere resting on the cube's top (y = 1), moved half a unit so it
  // stays clear of the top's edge (reaching the edge is a legitimate
  // grazing touch).
  let resting = { center: [0, 1.5, 0] as Vec3, radius: 0.5 }
  expect(scene.sweep(resting, [0.5, 0, 0], { meshes: [cube] }).length).toBe(0)
  let press = scene.sweep(resting, [0.5, -0.5, 0], { meshes: [cube] })
  expect(press.length).toBe(1)
  expect(press[0]!.time).toBeCloseTo(0, DIGITS)
  expect(press[0]!.normal).toBeCloseTo([0, 1, 0], DIGITS)
  expect(scene.sweep(resting, [0, 1, 0], { meshes: [cube] }).length).toBe(0)
})

test("layers: a scene-masked-out mesh is invisible to overlap and sweep unless the query passes its own, and { meshes } is an include-list", async app => {
  let { scene, collision, wide } = await mounted(app)
  let probe = { center: [0, -10, 0] as Vec3, radius: 1.5 }
  expect(scene.overlap(probe).length).toBe(0)
  let seen = scene.overlap(probe, { layers: 2 })
  expect(seen.length).toBe(1)
  expect(seen[0]!.mesh).toBe(collision)
  expect(scene.sweep({ center: [-5, -10, 0], radius: 0.5 }, [10, 0, 0]).length).toBe(0)
  expect(scene.sweep({ center: [-5, -10, 0], radius: 0.5 }, [10, 0, 0], { layers: 2 }).length).toBe(1)
  expect(scene.overlap({ center: [0, 0, 0], radius: 3 }, { meshes: [wide] }).length).toBe(0)
})

test("the box tier: an instanced mesh contacts by its population box like any surface", async app => {
  let { scene, instanced } = await mounted(app)
  // A sphere 0.3 into the box's face at y = 9.
  let contacts = scene.overlap({ center: [0, 8.7, 0], radius: 0.5 })
  expect(contacts.length).toBe(1)
  let contact = contacts[0]!
  expect(contact.mesh).toBe(instanced)
  expect(contact.depth).toBeCloseTo(0.2, DIGITS)
  expect(contact.normal).toBeCloseTo([0, -1, 0], DIGITS)
  expect(contact.point).toBeCloseTo([0, 9, 0], DIGITS)
})

test("moveAndSlide lands a capsule a skin above the floor and reports it", async app => {
  let { scene, cube } = await mounted(app)
  let body = { a: [0, 3, 0] as Vec3, b: [0, 4, 0] as Vec3, radius: 0.5 }
  let move = moveAndSlide(scene, body, [0, -3, 0], { meshes: [cube] })
  expect(move.floor).not.toBeNull()
  expect(move.floor!).toBeCloseTo([0, 1, 0], DIGITS)
  expect(move.motion[1]).toBeCloseTo(-(1.5 - SKIN), DIGITS)
  expect(move.wall).toBe(false)
  expect(move.ceiling).toBe(false)
})
