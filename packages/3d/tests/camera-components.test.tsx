// The camera components as an app wires them: `<OrbitCamera>` and
// `<FirstPersonCamera>` driving a Scene's camera from an input map the
// app bound to the scene's pointer feed and to the keyboard. The motion
// itself is tested in orbit.test.ts and first-person.test.ts on the flux
// binary; what is under test here is the glue between a real drag or key
// and the pose, which exists only under the app runtime.

import { test, expect } from "@solidrt/test"
import { createInputMap, createPointerFeed, keyboard, pct } from "@solidrt/core"
import { box, firstPersonActions, firstPersonBindings, FirstPersonCamera, Mesh, OrbitCamera, orbitActions, orbitBindings, PerspectiveCamera, Scene, unlit } from "../src/index.ts"
import type { FirstPersonCameraHandle, OrbitCameraHandle } from "../src/index.ts"

// Decimals a pose component that must not move is compared to.
const DIGITS = 5
// The eye's height, which a walk keeps.
const EYE = 1.6
// The walk's key hold, long enough for the pose to move visibly.
const WALK_MS = 500

// The feed and the map register cleanups, so an app creates them in a
// component body; here the mount callback is that body.
test("a drag over the scene orbits the camera through the input map", async app => {
  let orbit!: OrbitCameraHandle
  let root = await app.mount(() => {
    let pointer = createPointerFeed()
    let input = createInputMap(orbitActions)
    input.bind(orbitBindings({ pointer }))
    return (
      <view width={pct(100)} height={pct(100)}>
        <Scene pointer={pointer} label="orbit">
          <PerspectiveCamera fov={55} />
          <OrbitCamera input={input} ref={c => (orbit = c)} azimuth={0} elevation={0.3} distance={5} />
          <Mesh geometry={box()} material={unlit({ color: [0.5, 0.5, 0.5] })} />
        </Scene>
      </view>
    )
  })
  let before = orbit.pose()
  let scene = root.find({ kind: "texture" })
  let { x, y, width, height } = scene.box
  await app.drag({ x: x + width * 0.3, y: y + height / 2 }, { x: x + width * 0.7, y: y + height / 2 })
  await app.settle()
  let after = orbit.pose()
  // A drag to the right turns the view the way Three's OrbitControls
  // does: the azimuth decreases.
  expect(after.azimuth).toBeLessThan(before.azimuth)
  expect(after.elevation).toBeCloseTo(before.elevation, DIGITS)
  expect(after.distance).toBeCloseTo(before.distance, DIGITS)
})

test("a held key walks the first-person camera forward through the input map", async app => {
  let camera!: FirstPersonCameraHandle
  await app.mount(() => {
    let input = createInputMap(firstPersonActions)
    input.bind(firstPersonBindings({ keyboard }))
    return (
      <window onKeyDown={input.handlers.onKeyDown} onKeyUp={input.handlers.onKeyUp} onBlur={input.handlers.onBlur}>
        <view width={pct(100)} height={pct(100)}>
          <Scene label="walk">
            <PerspectiveCamera fov={70} />
            <FirstPersonCamera input={input} ref={c => (camera = c)} position={[0, EYE, 7]} />
            <Mesh geometry={box()} material={unlit({ color: [0.5, 0.5, 0.5] })} />
          </Scene>
        </view>
      </window>
    )
  })
  let before = camera.pose()
  await app.key("w", { holdMs: WALK_MS })
  await app.settle()
  let after = camera.pose()
  expect(after.position[2]).toBeLessThan(before.position[2])
  expect(after.position[1]).toBeCloseTo(EYE, DIGITS)
  expect(after.yaw).toBeCloseTo(before.yaw, DIGITS)
})
