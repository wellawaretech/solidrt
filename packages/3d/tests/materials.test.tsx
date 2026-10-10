// The surface slot on the unlit materials, pinned against a running
// scene: `unlit({ prelude, surface })` runs its function with the base
// resolved (a discard on half the quad cuts the picture, and the shadow
// twin is attached so the cut casts), and `sprite({ shape: "radial",
// surface })` composes the user's function after the radial falloff (the
// center takes the rewrite at full alpha). The same slot phong and
// standard already took; GPU state throughout, so this is an app test.

import { test, expect } from "@solidrt/test"
import type { TestApp } from "@solidrt/test"
import { glsl, readTexture } from "@solidrt/core/gpu"
import { add, createMesh, createScene, createSprite, plane, setTransform, sprite, unlit } from "../src/index.ts"
import type { Material, SceneHandle } from "../src/index.ts"

const SIZE = 128
// A camera at this distance with a 90 degree vertical fov covers 2 x DEPTH
// world units over SIZE pixels: 8 pixels per unit.
const DEPTH = 8
const FOV = 90
// The quad's world size: 32 px on screen.
const QUAD = 4
// Half the quad in pixels, where the discard test reads either side of
// the center.
const HALF_PX = 8
// A channel above this reads as painted over the dark clear.
const LIT = 128
const CLEAR: [number, number, number, number] = [0.07, 0.07, 0.1, 1]

async function mounted(app: TestApp, material: Material, billboard: boolean) {
  let scene!: SceneHandle
  await app.mount(() => {
    scene = createScene(SIZE, SIZE, { clearColor: CLEAR, label: "materials" })
    scene.setCamera({ fov: FOV, position: [0, 0, DEPTH], target: [0, 0, 0] })
    let mesh = billboard ? createSprite(material) : createMesh(plane({ width: QUAD, height: QUAD }), material)
    if (billboard) setTransform(mesh, { scale: [QUAD, QUAD, 1] })
    add(scene.root, mesh)
    return <texture src={scene.texture} width={SIZE} height={SIZE} />
  })
  return scene
}

function pixel(scene: SceneHandle, x: number, y: number): number[] {
  let { width, data } = readTexture(scene.texture)
  let at = (y * width + x) * 4
  return [data[at]!, data[at + 1]!, data[at + 2]!, data[at + 3]!]
}

test("unlit's surface function discards what it says, and the cut casts through the shadow twin", async app => {
  // The fragment declares vUv only with a map; the prelude declares it
  // for the surface, the vertex stage always writes it.
  let material = unlit({
    color: [1, 1, 1],
    prelude: glsl`
      in vec2 vUv;
    `,
    surface: glsl`
      void surface(inout Surface s) {
        if (vUv.x > 0.5) discard;
      }
    `,
  })
  expect(material.shadow).not.toBe(undefined)
  let scene = await mounted(app, material, false)
  await app.frame()
  let mid = SIZE / 2
  let left = pixel(scene, mid - HALF_PX, mid)
  let right = pixel(scene, mid + HALF_PX, mid)
  expect(left[0]! > LIT).toBe(true)
  expect(right[0]! < LIT).toBe(true)
})

test("sprite's surface function composes after the radial falloff: the center takes the rewrite at full alpha, the rim stays clear", async app => {
  let material = sprite({
    color: [1, 1, 1],
    shape: "radial",
    surface: glsl`
      void surface(inout Surface s) {
        s.base = vec4(0.0, 0.0, 1.0, 1.0) * s.base.a;
      }
    `,
  })
  let scene = await mounted(app, material, true)
  await app.frame()
  let mid = SIZE / 2
  let center = pixel(scene, mid, mid)
  expect(center[2]! > LIT).toBe(true)
  expect(center[0]! < LIT).toBe(true)
  // The quad's corner lies outside the inscribed disc: nothing painted.
  let corner = pixel(scene, mid - HALF_PX * 2 + 1, mid - HALF_PX * 2 + 1)
  expect(corner[2]! < LIT).toBe(true)
})
