// Large-world precision (okf/plans/3d-large-world-precision.md): the
// arena keeps positions in f64 and the GPU gets f32 relative to each
// target's render origin (camera.ts, the core's set_origin), so a scene a
// million units out must render exactly like the same scene at zero -
// the lit cube, its cast shadow on the ground, the camera, all of it -
// and pick the same. Two scenes side by side, the far one offset by a
// power of two large enough to be a multiple of every origin cell in
// play (the camera's and the shadow map's), so both snap to the same
// relative picture and the pixels compare byte for byte. GPU state
// throughout, so this is an app test.

import { test, expect } from "@solidrt/test"
import { add, box, createDirectionalLight, createMesh, createScene, lookAt, setTransform, standard } from "../src/index.ts"
import type { Scene as SceneApi } from "../src/scene.ts"
import type { Vec3 } from "../src/math.ts"

const SIZE = 96
/** The far scene's offset: 2^20 units, a multiple of every origin cell
 * a camera here snaps on. */
const FAR = 2 ** 20
const EYE: Vec3 = [3, 2.5, 6]
const SUN: Vec3 = [3, 5, 2]
const CLEAR: [number, number, number, number] = [0.1, 0.1, 0.12, 1]
/** A micron: what the f64 arena holds a hit to, a million units out. */
const HIT_EPS = 1e-6

let at = (o: Vec3, p: Vec3): Vec3 => [o[0] + p[0], o[1] + p[1], o[2] + p[2]]

function build(offset: Vec3, label: string): SceneApi {
  let scene = createScene(SIZE, SIZE, { label, clearColor: CLEAR })
  scene.setCamera({ position: at(offset, EYE), target: offset, near: 0.1, far: 50 })
  let ground = createMesh(box({ width: 6, height: 0.2, depth: 6 }), standard({ color: [0.6, 0.6, 0.6] }))
  setTransform(ground, { position: at(offset, [0, -0.6, 0]) })
  let cube = createMesh(box({ width: 1, height: 1, depth: 1 }), standard({ color: [0.8, 0.3, 0.2] }))
  setTransform(cube, { position: offset, rotation: [0, 0.6, 0] })
  let sun = createDirectionalLight({ castShadow: true })
  setTransform(sun, { position: at(offset, SUN) })
  lookAt(sun, offset)
  add(scene.root, ground)
  add(scene.root, cube)
  add(scene.root, sun)
  return scene
}

test("a scene a million units out renders byte for byte like the one at zero, shadows included, and picks the same", async app => {
  let near!: SceneApi
  let far!: SceneApi
  let root = await app.mount(() => {
    near = build([0, 0, 0], "near")
    far = build([FAR, 0, 0], "far")
    return (
      <view>
        <texture src={near.texture} width={SIZE} height={SIZE} />
        <texture src={far.texture} width={SIZE} height={SIZE} />
      </view>
    )
  })
  await app.settle()
  await app.frame()
  let [a, b] = root.findAll({ kind: "texture" })
  let pa = a!.pixels()
  let pb = b!.pixels()
  expect(pb.width).toBe(pa.width)
  expect(pb.height).toBe(pa.height)
  let differing = 0
  for (let i = 0; i < pa.data.length; i++) if (pa.data[i] !== pb.data[i]) differing++
  expect(differing).toBe(0)
  // The picture is a picture: the ground and the cube cover a good part
  // of it (the corner pixel is the clear color).
  let painted = 0
  for (let i = 0; i < pa.data.length; i += 4) {
    if (pa.data[i] !== pa.data[0] || pa.data[i + 1] !== pa.data[1] || pa.data[i + 2] !== pa.data[2]) painted++
  }
  expect(painted).toBeGreaterThan((pa.width * pa.height) / 8)
  // The rays agree to a micron: the index holds the world in f64.
  let h0 = near.raycast([0.2, 5, 0.1], [0, -1, 0])[0]
  let h1 = far.raycast([FAR + 0.2, 5, 0.1], [0, -1, 0])[0]
  expect(h0).toBeDefined()
  expect(h1).toBeDefined()
  expect(Math.abs(h1!.distance - h0!.distance)).toBeLessThan(HIT_EPS)
  expect(Math.abs(h1!.point[0] - FAR - h0!.point[0])).toBeLessThan(HIT_EPS)
  expect(Math.abs(h1!.point[1] - h0!.point[1])).toBeLessThan(HIT_EPS)
  expect(Math.abs(h1!.point[2] - h0!.point[2])).toBeLessThan(HIT_EPS)
  // A pixel pick through the far camera lands on the cube as the near one does.
  let p0 = near.pick(SIZE / 2, SIZE / 2)
  let p1 = far.pick(SIZE / 2, SIZE / 2)
  expect(p0.length).toBeGreaterThan(0)
  expect(p1.length).toBe(p0.length)
  expect(Math.abs(p1[0]!.distance - p0[0]!.distance)).toBeLessThan(HIT_EPS)
})
