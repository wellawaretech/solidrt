// The sprite material's screen-size clamp (minScreenPx/maxScreenPx),
// pinned against a running scene: a clamped sprite paints the clamped
// width whatever the camera distance, under perspective and orthographic
// cameras alike, a floored one releases to its natural size up close,
// `scene.pick` tests the drawn quad (hits and misses move with the
// clamp, the hit carries the ray distance and a facing normal), a
// fixed-y sprite picks the same way, a floored sprite turns its frustum
// culling off, and the pair is validated. GPU state throughout, so this
// is an app test; pixels are read after a frame, picks flush on their own.

import { test, expect } from "@solidrt/test"
import type { TestApp } from "@solidrt/test"
import { readTexture } from "@solidrt/core/gpu"
import { add, createScene, createSprite, setCulling, setMaterial, setTransform, sprite } from "../src/index.ts"
import type { Material, SceneHandle, Vec3 } from "../src/index.ts"

const SIZE = 128
// The clamp every clamped sprite here uses, pixels.
const PX = 16
// A camera at this distance with a 90 degree vertical fov covers 2 x DEPTH
// world units over SIZE pixels: 8 pixels per unit, so a unit sprite is 8
// px natural, half the clamp.
const DEPTH = 8
const FOV = 90
// Pixel slack on a painted width: a 16 px quad centered on a pixel
// boundary covers 16 or 17 pixels.
const SLACK = 1
// Decimals a hit's distance and normal are compared to.
const DIGITS = 3
// Red above this reads as the white sprite over the dark clear.
const LIT = 128

let rounded = (v: readonly number[]) => v.map(x => Number(x.toFixed(DIGITS)) + 0)

async function mounted(app: TestApp, material: Material, position: Vec3 = [0, 0, 0]) {
  let scene!: SceneHandle
  let mesh!: ReturnType<typeof createSprite>
  await app.mount(() => {
    scene = createScene(SIZE, SIZE, { clearColor: [0.07, 0.07, 0.1, 1], label: "sprite-clamp" })
    scene.setCamera({ fov: FOV, position: [0, 0, DEPTH], target: [0, 0, 0] })
    mesh = createSprite(material)
    setTransform(mesh, { position })
    add(scene.root, mesh)
    return <texture src={scene.texture} width={SIZE} height={SIZE} />
  })
  return { scene, mesh }
}

/** The lit span of the row through the target's center, in pixels. */
function paintedWidth(scene: SceneHandle): number {
  let { width, data } = readTexture(scene.texture)
  let row = SIZE / 2
  let lit = 0
  for (let x = 0; x < width; x++) if (data[(row * width + x) * 4]! > LIT) lit++
  return lit
}

function expectWidth(got: number, want: number): void {
  if (Math.abs(got - want) > SLACK) throw new Error(`painted ${got} px, want ${want}`)
}

let constant = () => sprite({ color: [1, 1, 1], transparent: false, minScreenPx: PX, maxScreenPx: PX })

test("equal bounds hold a constant screen size at any camera distance, and pick tests the drawn quad", async app => {
  let { scene, mesh } = await mounted(app, constant())
  let mid = SIZE / 2
  await app.frame()
  expectWidth(paintedWidth(scene), PX)
  // 6 px off the center is inside the 16 px quad, 10 px is not.
  expect(scene.pick(mid + 6, mid).map(h => h.mesh)).toEqual([mesh])
  expect(scene.pick(mid + 10, mid)).toEqual([])
  // Four times further: the same 16 px, where the unit quad would be 2.
  scene.setCamera({ position: [0, 0, 4 * DEPTH] })
  await app.frame()
  expectWidth(paintedWidth(scene), PX)
  expect(scene.pick(mid + 6, mid).map(h => h.mesh)).toEqual([mesh])
  expect(scene.pick(mid + 10, mid)).toEqual([])
  // Four times closer: still 16, where the unit quad would be 32.
  scene.setCamera({ position: [0, 0, DEPTH / 4] })
  await app.frame()
  expectWidth(paintedWidth(scene), PX)
  expect(scene.pick(mid + 12, mid)).toEqual([])
  // The hit itself: the ray's distance to the quad's plane, the normal
  // facing the camera.
  let [hit] = scene.pick(mid, mid)
  expect(hit!.mesh).toBe(mesh)
  expect(rounded([hit!.distance])).toEqual([DEPTH / 4])
  expect(rounded(hit!.normal)).toEqual([0, 0, 1])
})

test("an unclamped sprite stays world-sized, the control for the clamp", async app => {
  let { scene, mesh } = await mounted(app, sprite({ color: [1, 1, 1], transparent: false }))
  let mid = SIZE / 2
  scene.setCamera({ position: [0, 0, 4 * DEPTH] })
  await app.frame()
  expectWidth(paintedWidth(scene), 2)
  expect(scene.pick(mid + 6, mid)).toEqual([])
  expect(mesh.frustumCulled).toBe(true)
})

test("a floor releases up close, a ceiling releases far away, and a floored sprite is never frustum-culled", async app => {
  let { scene, mesh } = await mounted(app, sprite({ color: [1, 1, 1], transparent: false, minScreenPx: PX }))
  let mid = SIZE / 2
  expect(mesh.frustumCulled).toBe(false)
  await app.frame()
  expectWidth(paintedWidth(scene), PX)
  // Four times closer the unit quad is 32 px on its own: no floor needed.
  scene.setCamera({ position: [0, 0, DEPTH / 4] })
  await app.frame()
  expectWidth(paintedWidth(scene), 2 * PX)
  expect(scene.pick(mid + 12, mid).map(h => h.mesh)).toEqual([mesh])
  // A ceiling alone: 32 px natural capped at 16; 2 px natural far away.
  let capped = sprite({ color: [1, 1, 1], transparent: false, maxScreenPx: PX })
  setMaterial(mesh, capped)
  await app.frame()
  expectWidth(paintedWidth(scene), PX)
  expect(scene.pick(mid + 12, mid)).toEqual([])
  expect(scene.pick(mid + 6, mid).map(h => h.mesh)).toEqual([mesh])
  scene.setCamera({ position: [0, 0, 4 * DEPTH] })
  await app.frame()
  expectWidth(paintedWidth(scene), 2)
  // The floor's culling switch is one way: a capped material does not
  // turn it back on, setCulling does.
  expect(mesh.frustumCulled).toBe(false)
  setCulling(mesh, { frustumCulled: true })
  expect(mesh.frustumCulled).toBe(true)
  setMaterial(mesh, constant())
  expect(mesh.frustumCulled).toBe(false)
})

test("an orthographic camera clamps the same way, from its extents", async app => {
  let { scene, mesh } = await mounted(app, constant())
  let mid = SIZE / 2
  // 16 units across 128 px: 8 px per unit, the perspective case's density.
  scene.setCamera({ ortho: { left: -DEPTH, right: DEPTH, top: DEPTH, bottom: -DEPTH } })
  await app.frame()
  expectWidth(paintedWidth(scene), PX)
  expect(scene.pick(mid + 6, mid).map(h => h.mesh)).toEqual([mesh])
  // Four times the extent: 2 px per unit, still 16.
  scene.setCamera({ ortho: { left: -4 * DEPTH, right: 4 * DEPTH, top: 4 * DEPTH, bottom: -4 * DEPTH } })
  await app.frame()
  expectWidth(paintedWidth(scene), PX)
  expect(scene.pick(mid + 6, mid).map(h => h.mesh)).toEqual([mesh])
  expect(scene.pick(mid + 10, mid)).toEqual([])
})

test("a fixed-y sprite picks through its yaw basis at the drawn size", async app => {
  let { scene, mesh } = await mounted(app, sprite({ color: [1, 1, 1], transparent: false, billboard: "fixed-y", minScreenPx: PX, maxScreenPx: PX }))
  let mid = SIZE / 2
  scene.setCamera({ position: [0, 0, 4 * DEPTH] })
  await app.frame()
  expectWidth(paintedWidth(scene), PX)
  expect(scene.pick(mid + 6, mid).map(h => h.mesh)).toEqual([mesh])
  expect(scene.pick(mid + 10, mid)).toEqual([])
  // Seen from the side the quad yaws to face the camera and still picks.
  scene.setCamera({ position: [4 * DEPTH, 0, 0] })
  expect(scene.pick(mid + 6, mid).map(h => h.mesh)).toEqual([mesh])
  expect(scene.pick(mid + 10, mid)).toEqual([])
})

test("the clamp pair is validated", async app => {
  await mounted(app, constant())
  expect(() => sprite({ minScreenPx: PX, maxScreenPx: PX / 2 })).toThrow()
  expect(() => sprite({ minScreenPx: -1 })).toThrow()
  expect(() => sprite({ maxScreenPx: Number.NaN })).toThrow()
})
