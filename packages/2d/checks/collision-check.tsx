// Doc-contract check for the sprite layer's spatial queries: the claims
// AGENTS.md (Picking and queries) and layer.ts make about raycast(),
// overlap(), sweep() and moveAndSlide(), asserted against a running
// layer so the copies cannot drift from the runtime. Claims covered:
//   1. overlap over a circle finds the sprites in a blast radius with
//      in-plane contacts (a sprite is a column in the index, so a circle
//      over a sprite pushes out through its nearest edge, never along z).
//   2. sweep reports the first sprite a shot reaches, at the exact time,
//      with the touch point and the edge normal; raycast is the ray form.
//   3. pickRect is overlap over an unrotated rect, sprites only.
//   4. { sprites } is an include-list.
//   5. moveAndSlide lands a capsule on a floor of sprites a skin short,
//      stops at a wall with `wall` while keeping the floor, and walks up
//      a slope (a rotated sprite) reporting it as floor.
// Runs on the playback client, from the repo root:
//
//   bunx srt render packages/2d/checks/collision-check.tsx --project --duration 3 --size 128x128
//
// Asserts on the second frame and prints one PASS/FAIL summary, then
// exits; read the output, not the exit code.
import { decodeImage, exit, onFrame, pct, render } from "@solidrt/core"
import { addSprite, createAtlas, createSpriteLayer } from "@solidrt/2d"
import type { SpriteHandle } from "@solidrt/2d"
import logoBytes from "../examples/logo.png" with { type: "binary" }

const SIZE = 128
const EPS = 1e-3
// The mover's default skin, what the landing gaps below are measured in.
const SKIN = 0.01
// The slope's tilt: 30 degrees, well inside the 45-degree floor limit.
const SLOPE = -Math.PI / 6

let failures = 0
function fail(msg: string) {
  failures++
  console.log(`FAIL: ${msg}`)
}
let near = (a: [number, number], b: [number, number]) => Math.abs(a[0] - b[0]) < EPS && Math.abs(a[1] - b[1]) < EPS

function App() {
  let atlas = createAtlas(decodeImage(logoBytes), { label: "logo-atlas" })
  let layer = createSpriteLayer(atlas.texture, { capacity: 64, label: "collision-check" })
  let view = layer.createView({ width: SIZE, height: SIZE, clearColor: [0.07, 0.07, 0.1, 1], label: "collision-check" })

  // A floor of 32 px tiles whose top edge is y = 100 (y down), x 0..256.
  let floor: SpriteHandle[] = []
  for (let i = 0; i < 8; i++) floor.push(addSprite(layer, { x: 16 + i * 32, y: 116, w: 32, h: 32 }))
  // A wall standing on the floor's right end: left edge x = 256, y 36..100.
  let wall = addSprite(layer, { x: 272, y: 68, w: 32, h: 64 })
  // A slope off to the right: a long thin sprite tilted 30 degrees, its
  // top face rising toward +x.
  let slope = addSprite(layer, { x: 400, y: 100, w: 120, h: 8, rotation: SLOPE })

  onFrame((_tick, frame) => {
    if (frame !== 2) return

    // 1. A blast circle over the floor: three tiles in reach, the middle
    //    one pushed out through its top edge by radius - gap.
    let blast = layer.overlap({ x: 48, y: 96, radius: 20 })
    if (blast.length !== 3) fail(`a 20 px circle over the floor should touch 3 tiles, got ${blast.length}`)
    let middle = blast.find((c) => c.sprite === floor[1])
    if (middle === undefined) fail("the tile under the circle must be among the contacts")
    else if (Math.abs(middle.depth - 16) > EPS || !near(middle.normal, [0, -1]) || !near(middle.point, [48, 100]))
      fail(`the contact under the circle should be depth 16 out of the top edge at (48, 100), got ${JSON.stringify(middle)}`)
    for (let c of blast) if (Math.abs(Math.hypot(c.normal[0], c.normal[1]) - 1) > EPS) fail("every contact normal is a unit vector in the plane")

    // 2. A shot straight down from (48, 20): the tile at x = 48 first, its
    //    top edge reached when the bullet's front (radius 2) is at y = 100.
    let shot = layer.sweep({ x: 48, y: 20, radius: 2 }, 0, 200)
    if (shot.length === 0 || shot[0]!.sprite !== floor[1]) fail("the shot should reach the tile under it first")
    else {
      let h = shot[0]!
      if (Math.abs(h.time - 0.39) > EPS) fail(`sweep time should be 0.39, got ${h.time}`)
      if (!near(h.point, [48, 100]) || !near(h.normal, [0, -1])) fail(`sweep should touch the top edge facing up, got ${JSON.stringify(h)}`)
    }
    let ray = layer.raycast(48, 20, 0, 1)
    if (ray.length === 0 || ray[0]!.sprite !== floor[1] || Math.abs(ray[0]!.distance - 80) > EPS || !near(ray[0]!.point, [48, 100]))
      fail(`the ray should strike the tile's top edge at distance 80, got ${JSON.stringify(ray[0])}`)

    // 3. pickRect is overlap over the rect: the four tiles the rect touches.
    let marquee = layer.pickRect(0, 84, 100, 32)
    let rect = layer.overlap({ x: 0, y: 84, width: 100, height: 32 }).map((c) => c.sprite)
    if (marquee.length !== 4 || marquee.length !== rect.length || !marquee.every((s) => rect.includes(s)))
      fail(`pickRect should be overlap's sprites (4 tiles), got ${marquee.length} vs ${rect.length}`)

    // 4. The include-list: the same shot sees nothing with only the wall listed.
    if (layer.sweep({ x: 48, y: 20, radius: 2 }, 0, 200, { sprites: [wall] }).length !== 0) fail("{ sprites } must exclude unlisted sprites")

    // 5. The mover. A capsule dropped on the floor lands a skin short.
    let body = { ax: 200, ay: 60, bx: 200, by: 80, radius: 8 }
    let land = layer.moveAndSlide(body, 0, 50)
    if (land.floor === null || !near(land.floor, [0, -1])) fail(`a dropped capsule should report the floor, got ${JSON.stringify(land.floor)}`)
    if (Math.abs(land.motion[1] - (12 - SKIN)) > EPS) fail(`the landing should stop a skin above the floor, got ${land.motion[1]}`)
    if (land.wall || land.ceiling) fail("a landing is neither a wall nor a ceiling")
    // Standing a skin above the floor, walking into the wall: stops a skin
    // short of it, reports the wall and still stands on the floor.
    let stand = { ax: 240, ay: 72, bx: 240, by: 100 - SKIN - 8, radius: 8 }
    let bump = layer.moveAndSlide(stand, 30, 0)
    if (!bump.wall) fail("walking into the wall should report wall")
    if (Math.abs(bump.motion[0] - (8 - SKIN)) > EPS || Math.abs(bump.motion[1]) > EPS)
      fail(`the walk should stop a skin short of the wall, got [${bump.motion}]`)
    if (bump.floor === null) fail("a body walking along the floor keeps its floor")
    // The slope: drop onto it, then walk +x and climb.
    let climber = { ax: 400, ay: 30, bx: 400, by: 50, radius: 8 }
    let drop = layer.moveAndSlide(climber, 0, 80)
    let want: [number, number] = [Math.sin(SLOPE), -Math.cos(SLOPE)]
    if (drop.floor === null || !near(drop.floor, want)) fail(`the slope should be the floor with normal [${want}], got ${JSON.stringify(drop.floor)}`)
    if (drop.hits.length === 0 || drop.hits[0]!.sprite !== slope) fail("the drop should hit the slope sprite")
    let landed = { ax: 400, ay: 30 + drop.motion[1], bx: 400, by: 50 + drop.motion[1], radius: 8 }
    let climb = layer.moveAndSlide(landed, 30, 0)
    if (climb.floor === null) fail("walking along the slope keeps it as floor")
    if (!(climb.motion[0] > 20 && climb.motion[1] < -5)) fail(`walking +x along the slope should climb it, got [${climb.motion}]`)

    if (failures === 0) console.log("PASS: overlap contacts, sweep and raycast, pickRect parity, sprites filter, moveAndSlide landing, wall and slope")
    else console.log(`${failures} FAILURES`)
    exit()
  })

  return (
    <window>
      <view width={pct(100)} height={pct(100)}>
        <texture src={view.texture} width={SIZE} height={SIZE} />
      </view>
    </window>
  )
}

render(App)
