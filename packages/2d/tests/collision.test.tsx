// The sprite layer's spatial queries as AGENTS.md (Picking and queries)
// and layer.ts describe them, pinned against a running layer so the copies
// cannot drift from the runtime: overlap over a circle with in-plane
// contacts (a sprite is a column in the index, so a circle over one pushes
// out through its nearest edge, never along z), sweep at the exact time
// with raycast as its ray form, pickRect as overlap over a rect, the
// { sprites } include-list, and moveAndSlide on a floor, a wall and a
// slope. The layer is GPU state, so this is an app test. A query flushes
// the pending writes, so no frame is run after the mount.

import { test, expect } from "@solidrt/test"
import type { TestApp } from "@solidrt/test"
import { addSprite, createAtlas, createSpriteLayer } from "../src/index.ts"
import type { SpriteHandle } from "../src/index.ts"

const SIZE = 128
// The atlas: one opaque white texel, since the queries never read a pixel.
const ATLAS = { width: 1, height: 1, data: new Uint8Array([255, 255, 255, 255]) }
// Decimals a time, point or normal is compared to.
const DIGITS = 3
// The mover's default skin, what the landing gaps are measured in.
const SKIN = 0.01
// The slope's tilt: 30 degrees, well inside the 45-degree floor limit.
const SLOPE = -Math.PI / 6

let rounded = (v: readonly number[]) => v.map(x => Number(x.toFixed(DIGITS)) + 0)

function world() {
  let atlas = createAtlas(ATLAS, { label: "collision-atlas" })
  let layer = createSpriteLayer(atlas.texture, { capacity: 64, label: "collision" })
  let view = layer.createView({ width: SIZE, height: SIZE, clearColor: [0.07, 0.07, 0.1, 1], label: "collision" })

  // A floor of 32 px tiles whose top edge is y = 100 (y down), x 0..256.
  let floor: SpriteHandle[] = []
  for (let i = 0; i < 8; i++) floor.push(addSprite(layer, { x: 16 + i * 32, y: 116, w: 32, h: 32 }))
  // A wall standing on the floor's right end: left edge x = 256, y 36..100.
  let wall = addSprite(layer, { x: 272, y: 68, w: 32, h: 64 })
  // A slope off to the right: a long thin sprite tilted 30 degrees, its
  // top face rising toward +x.
  let slope = addSprite(layer, { x: 400, y: 100, w: 120, h: 8, rotation: SLOPE })
  return { layer, view, floor, wall, slope }
}

// The atlas, layer and view own GPU resources, so they are created under
// the mounted app's root.
async function mounted(app: TestApp) {
  let w!: ReturnType<typeof world>
  await app.mount(() => {
    w = world()
    return <texture src={w.view.texture} width={SIZE} height={SIZE} />
  })
  return w
}

test("overlap over a circle finds the sprites in a blast radius, each contact out of its nearest edge in the plane", async app => {
  let { layer, floor } = await mounted(app)
  // A 20 px circle over the floor: three tiles in reach, the middle one
  // pushed out through its top edge by radius - gap.
  let blast = layer.overlap({ x: 48, y: 96, radius: 20 })
  expect(blast.length).toBe(3)
  let middle = blast.find(c => c.sprite === floor[1])
  expect(middle).not.toBe(undefined)
  expect(rounded([middle!.depth])).toEqual([16])
  expect(rounded(middle!.normal)).toEqual([0, -1])
  expect(rounded(middle!.point)).toEqual([48, 100])
  for (let c of blast) expect(rounded([Math.hypot(c.normal[0], c.normal[1])])).toEqual([1])
})

test("sweep reports the first sprite a shot reaches, at the exact time, with the touch point and the edge normal; raycast is the ray form", async app => {
  let { layer, floor } = await mounted(app)
  // A shot straight down from (48, 20): the tile at x = 48 first, its top
  // edge reached when the bullet's front (radius 2) is at y = 100.
  let shot = layer.sweep({ x: 48, y: 20, radius: 2 }, 0, 200)
  expect(shot.length).toBeGreaterThan(0)
  let hit = shot[0]!
  expect(hit.sprite).toBe(floor[1])
  expect(rounded([hit.time])).toEqual([0.39])
  expect(rounded(hit.point)).toEqual([48, 100])
  expect(rounded(hit.normal)).toEqual([0, -1])
  let ray = layer.raycast(48, 20, 0, 1)
  expect(ray.length).toBeGreaterThan(0)
  expect(ray[0]!.sprite).toBe(floor[1])
  expect(rounded([ray[0]!.distance])).toEqual([80])
  expect(rounded(ray[0]!.point)).toEqual([48, 100])
})

test("pickRect is overlap over an unrotated rect, sprites only", async app => {
  let { layer } = await mounted(app)
  // The four tiles the rect touches.
  let marquee = layer.pickRect(0, 84, 100, 32)
  let rect = layer.overlap({ x: 0, y: 84, width: 100, height: 32 }).map(c => c.sprite)
  expect(marquee.length).toBe(4)
  expect(rect.length).toBe(4)
  for (let sprite of marquee) expect(rect).toContain(sprite)
})

test("{ sprites } is an include-list: the same shot sees nothing with only the wall listed", async app => {
  let { layer, wall } = await mounted(app)
  expect(layer.sweep({ x: 48, y: 20, radius: 2 }, 0, 200, { sprites: [wall] }).length).toBe(0)
})

test("moveAndSlide lands a capsule a skin short of the floor, stops at a wall keeping the floor, and climbs a slope as floor", async app => {
  let { layer, slope } = await mounted(app)
  // A capsule dropped on the floor lands a skin short.
  let body = { ax: 200, ay: 60, bx: 200, by: 80, radius: 8 }
  let land = layer.moveAndSlide(body, 0, 50)
  expect(land.floor).not.toBeNull()
  expect(rounded(land.floor!)).toEqual([0, -1])
  expect(rounded([land.motion[1]])).toEqual([12 - SKIN])
  expect(land.wall).toBe(false)
  expect(land.ceiling).toBe(false)
  // Standing a skin above the floor, walking into the wall: stops a skin
  // short of it, reports the wall and still stands on the floor.
  let stand = { ax: 240, ay: 72, bx: 240, by: 100 - SKIN - 8, radius: 8 }
  let bump = layer.moveAndSlide(stand, 30, 0)
  expect(bump.wall).toBe(true)
  expect(rounded(bump.motion)).toEqual([8 - SKIN, 0])
  expect(bump.floor).not.toBeNull()
  // The slope: drop onto it, then walk +x and climb.
  let climber = { ax: 400, ay: 30, bx: 400, by: 50, radius: 8 }
  let drop = layer.moveAndSlide(climber, 0, 80)
  expect(drop.floor).not.toBeNull()
  expect(rounded(drop.floor!)).toEqual(rounded([Math.sin(SLOPE), -Math.cos(SLOPE)]))
  expect(drop.hits.length).toBeGreaterThan(0)
  expect(drop.hits[0]!.sprite).toBe(slope)
  let landed = { ax: 400, ay: 30 + drop.motion[1], bx: 400, by: 50 + drop.motion[1], radius: 8 }
  let climb = layer.moveAndSlide(landed, 30, 0)
  expect(climb.floor).not.toBeNull()
  expect(climb.motion[0]).toBeGreaterThan(20)
  expect(climb.motion[1]).toBeLessThan(-5)
})
