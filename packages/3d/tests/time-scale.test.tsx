// The time scale contract (AGENTS.md Retargeted motion, node.ts
// setTimeScale) pinned against a running scene: a frozen scene holds a
// transition where it is, asks for no frames (so `settle` resolves under
// it) and resumes from the same place; a group declaring its own rate
// keeps running inside the frozen scene; half rate covers half the
// distance. The scene is GPU state, so this is an app test, and frames are
// stepped so the read is at a known app time.

import { test, expect } from "@solidrt/test"
import type { TestApp } from "@solidrt/test"
import { add, createGroup, createScene, getTransform, remove, setTimeScale, setTransform, setTransition, timeRate } from "../src/index.ts"
import type { SceneNode } from "../src/index.ts"
import type { Scene } from "../src/scene.ts"

const SIZE = 32
// A linear tween over a second to x = 10: app time in ms maps to x / 100.
const DURATION = 1000
const TARGET_X = 10
// Frames land on the 60 fps grid and the mount frame only anchors the
// clock, so a read is within a frame and a half of its time.
const SLACK = (TARGET_X / 60) * 1.5

async function mounted(app: TestApp): Promise<Scene> {
  let scene!: Scene
  await app.mount(() => {
    scene = createScene(SIZE, SIZE, { label: "time-scale" })
    return <texture src={scene.texture} width={SIZE} height={SIZE} />
  })
  return scene
}

/** A group under `parent` whose position tweens linearly, sent to x = 10. */
function mover(parent: SceneNode): SceneNode {
  let node = createGroup()
  add(parent, node)
  setTransition(node, { position: { duration: DURATION, curve: "linear" } })
  setTransform(node, { position: [TARGET_X, 0, 0] })
  return node
}

let x = (node: SceneNode) => getTransform(node).position[0]

test("a frozen scene holds its motion, idles, and resumes from where it was", async app => {
  let scene = await mounted(app)
  let node = mover(scene.root)
  await app.advance(DURATION / 2)
  expect(Math.abs(x(node) - TARGET_X / 2)).toBeLessThan(SLACK)

  scene.setTimeScale(0)
  expect(timeRate(node)).toBe(0)
  expect(scene.timeRate()).toBe(0)
  let held = x(node)
  await app.advance(DURATION)
  expect(x(node)).toBe(held)
  // A frozen track is not frame demand: the app is at rest.
  await app.settle({ maxMs: 100 })
  expect(x(node)).toBe(held)

  scene.setTimeScale(null)
  expect(timeRate(node)).toBe(1)
  await app.advance(DURATION / 4)
  // A quarter further, not where app time would have taken it.
  expect(Math.abs(x(node) - (TARGET_X * 3) / 4)).toBeLessThan(SLACK)
  await app.settle()
  expect(x(node)).toBe(TARGET_X)
})

test("a group declaring its own rate keeps running inside a frozen scene", async app => {
  let scene = await mounted(app)
  let frozen = mover(scene.root)
  let preview = createGroup()
  add(scene.root, preview)
  setTimeScale(preview, 1)
  let spinning = mover(preview)
  scene.setTimeScale(0)
  expect(timeRate(frozen)).toBe(0)
  expect(timeRate(spinning)).toBe(1)
  await app.advance(DURATION / 2)
  expect(x(frozen)).toBe(0)
  expect(Math.abs(x(spinning) - TARGET_X / 2)).toBeLessThan(SLACK)
  // Clearing the group's declaration folds it back under the scene's.
  setTimeScale(preview, null)
  expect(timeRate(spinning)).toBe(0)
  let held = x(spinning)
  await app.advance(DURATION / 2)
  expect(x(spinning)).toBe(held)
})

test("a rate scales every step, and the declaration survives a scene re-enter", async app => {
  let scene = await mounted(app)
  let node = mover(scene.root)
  setTimeScale(node, 0.5)
  await app.advance(DURATION)
  expect(Math.abs(x(node) - TARGET_X / 2)).toBeLessThan(SLACK)
  // The prop lives on the SceneNode: out of the scene and back in, the
  // core node is new and carries the same rate.
  remove(node)
  expect(timeRate(node)).toBe(null)
  add(scene.root, node)
  expect(timeRate(node)).toBe(0.5)
})
