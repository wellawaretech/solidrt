// lookAt on a light (node.ts): a directional or spot light shines along
// its own `direction`, so lookAt aims that, not the +z every other node
// aims - under a rotated parent too, and for a non-default direction. A
// light whose direction IS +z takes exactly a group's rotation. Imports
// flux:spatial through the package root, so this is an app test, though
// it mounts nothing: unattached nodes compose their world matrix in JS.

import { test, expect } from "@solidrt/test"
import { add, createGroup, createSpotLight, lookAt, setTransform, worldInto, worldPosition } from "../src/index.ts"
import { mat4, transformVector } from "../src/math.ts"
import type { Vec3 } from "../src/math.ts"

/** Float noise in a unit vector composed through two quaternions. */
const DIR_EPS = 1e-9

let unit = (v: Vec3): Vec3 => {
  let l = Math.hypot(v[0], v[1], v[2])
  return [v[0] / l, v[1] / l, v[2] / l]
}

test("lookAt points a light's direction at the target, and a +z light aims like a group", async () => {
  let parent = createGroup()
  setTransform(parent, { position: [1, 2, 3], rotation: [0.3, -0.8, 0.2] })
  let spot = createSpotLight({ direction: [1, -2, 0.5] })
  setTransform(spot, { position: [0.5, 1, -1] })
  add(parent, spot)
  let target: Vec3 = [-4, 0, 2]
  lookAt(spot, target)
  let beam = unit(transformVector([0, 0, 0], worldInto(mat4(), spot), spot.direction))
  let from = worldPosition(spot)
  let aim = unit([target[0] - from[0], target[1] - from[1], target[2] - from[2]])
  for (let i = 0; i < 3; i++) expect(Math.abs(beam[i]! - aim[i]!)).toBeLessThan(DIR_EPS)

  let forward = createSpotLight({ direction: [0, 0, 1] })
  let group = createGroup()
  lookAt(forward, target)
  lookAt(group, target)
  for (let i = 0; i < 4; i++) expect(Math.abs(forward.quaternion[i]! - group.quaternion[i]!)).toBeLessThan(DIR_EPS)
})
