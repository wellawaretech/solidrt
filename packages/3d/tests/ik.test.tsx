// The IK contract (agents/animation.md Inverse kinematics, ik.ts) pinned
// against a running scene: a look-at aims the head at a node target and
// follows it when it moves, a two-bone limb reaches a point and reports its
// reach, the declaration survives a scene re-enter, a weight fade is ramped
// in core, and dispose lets the joints go. The stacks are core state over
// live scene nodes, so this is an app test.

import { test, expect } from "@solidrt/test"
import { add, createGroup, createIk, createModel, createScene, getTransform, remove, setTransform, worldPosition } from "../src/index.ts"
import type { Model, ModelData, Quat, SceneNode, Vec3 } from "../src/index.ts"
import type { Scene } from "../src/scene.ts"
import type { TestApp } from "@solidrt/test"

const SIZE = 32
/** The arm: two bones of this length hanging from the shoulder. */
const BONE = 0.3
const SETTLE_MS = 2000

async function mounted(app: TestApp): Promise<Scene> {
  let scene!: Scene
  await app.mount(() => {
    scene = createScene(SIZE, SIZE, { label: "ik" })
    return <texture src={scene.texture} width={SIZE} height={SIZE} />
  })
  return scene
}

/** A meshless model: a head at y = 1 looking down +z, an arm hanging
 * from a shoulder beside it. */
function rig(scene: Scene): { model: Model; head: SceneNode; shoulder: SceneNode; elbow: SceneNode; wrist: SceneNode } {
  let data: ModelData = {
    nodes: [
      { name: "root", parent: null, position: [0, 0, 0], rotation: [0, 0, 0, 1], scale: [1, 1, 1] },
      { name: "head", parent: 0, position: [0, 1, 0], rotation: [0, 0, 0, 1], scale: [1, 1, 1] },
      { name: "shoulder", parent: 0, position: [0.3, 1, 0], rotation: [0, 0, 0, 1], scale: [1, 1, 1] },
      { name: "elbow", parent: 2, position: [0, -BONE, 0], rotation: [0, 0, 0, 1], scale: [1, 1, 1] },
      { name: "wrist", parent: 3, position: [0, -BONE, 0], rotation: [0, 0, 0, 1], scale: [1, 1, 1] },
    ],
    parts: [],
    skins: [],
    clips: [],
    materials: [],
    images: [],
    bounds: new Float32Array([-1, 0, -1, 1, 1, 1]),
  }
  let model = createModel(data, { label: "rig", autoFree: false })
  add(scene.root, model)
  let n = (i: number) => model.nodes[i]!.node
  return { model, head: n(1), shoulder: n(2), elbow: n(3), wrist: n(4) }
}

/** +z rotated by q: the head's forward in its parent's (the model's) space. */
function forward(q: Quat): Vec3 {
  let [x, y, z, w] = q
  return [2 * (x * z + w * y), 2 * (y * z - w * x), 1 - 2 * (x * x + y * y)]
}

function dist(a: ArrayLike<number>, b: ArrayLike<number>): number {
  return Math.hypot(a[0]! - b[0]!, a[1]! - b[1]!, a[2]! - b[2]!)
}

test("a look-at aims the head at a node and follows it when it moves", async app => {
  let scene = await mounted(app)
  let { model, head } = rig(scene)
  let ball = createGroup()
  setTransform(ball, { position: [10, 1, 0] })
  add(scene.root, ball)
  createIk(model, { solvers: [{ kind: "lookAt", bones: [head], target: { node: ball }, easeMs: 0 }] })
  await app.settle({ maxMs: SETTLE_MS })
  let f = forward(getTransform(head).quaternion)
  expect(f[0]).toBeCloseTo(1, 3)
  expect(f[2]).toBeCloseTo(0, 3)
  // The target node moves: the core reads it at solve time, no JS between.
  setTransform(ball, { position: [0, 1, -10] })
  await app.settle({ maxMs: SETTLE_MS })
  f = forward(getTransform(head).quaternion)
  expect(f[2]).toBeCloseTo(-1, 3)
})

test("a two-bone limb reaches a point, reports its reach, survives a re-enter, and dispose lets go", async app => {
  let scene = await mounted(app)
  let { model, shoulder, elbow, wrist } = rig(scene)
  let target: Vec3 = [0.5, 0.6, 0.1]
  let ik = createIk(model, { solvers: [{ kind: "twoBone", root: shoulder, mid: elbow, tip: wrist, target: { point: target } }] })
  await app.settle({ maxMs: SETTLE_MS })
  expect(dist(worldPosition(wrist), target)).toBeLessThan(1e-3)
  expect(ik.reach(0)).toBeLessThan(1e-3)
  // Out and back in: the stack re-binds and reaches again.
  remove(model)
  await app.settle({ maxMs: SETTLE_MS })
  add(scene.root, model)
  await app.settle({ maxMs: SETTLE_MS })
  expect(dist(worldPosition(wrist), target)).toBeLessThan(1e-3)
  // A target past reach: the residual says how far.
  ik.setTarget(0, { point: [0.3, 0, 0] })
  await app.settle({ maxMs: SETTLE_MS })
  expect(ik.reach(0)).toBeCloseTo(1 - 2 * BONE, 3)
  ik.dispose()
  await app.settle({ maxMs: SETTLE_MS })
  expect(dist(worldPosition(wrist), [0.3, 1 - 2 * BONE, 0])).toBeLessThan(1e-4)
})

test("a weight fade is ramped in core", async app => {
  let scene = await mounted(app)
  let { model, shoulder, elbow, wrist } = rig(scene)
  let target: Vec3 = [0.5, 0.6, 0.1]
  let rest: Vec3 = [0.3, 1 - 2 * BONE, 0]
  let ik = createIk(model, { solvers: [{ kind: "twoBone", root: shoulder, mid: elbow, tip: wrist, target: { point: target }, weight: 0 }] })
  await app.settle({ maxMs: SETTLE_MS })
  expect(dist(worldPosition(wrist), rest)).toBeLessThan(1e-4)
  ik.setWeight(0, 1, { fadeMs: 400 })
  await app.advance(200)
  let mid = dist(worldPosition(wrist), target)
  expect(mid).toBeGreaterThan(0.02)
  expect(mid).toBeLessThan(dist(rest, target) - 0.02)
  await app.settle({ maxMs: SETTLE_MS })
  expect(dist(worldPosition(wrist), target)).toBeLessThan(1e-3)
})

test("a stack over plain nodes: a two-bone limb under a group reaches a point", async app => {
  let scene = await mounted(app)
  let rig = createGroup()
  let shoulder = createGroup()
  let elbow = createGroup()
  let wrist = createGroup()
  setTransform(shoulder, { position: [0.3, 1, 0] })
  setTransform(elbow, { position: [0, -BONE, 0] })
  setTransform(wrist, { position: [0, -BONE, 0] })
  add(rig, shoulder)
  add(shoulder, elbow)
  add(elbow, wrist)
  add(scene.root, rig)
  let target: Vec3 = [0.5, 0.6, 0.1]
  let ik = createIk(rig, { solvers: [{ kind: "twoBone", root: shoulder, mid: elbow, tip: wrist, target: { point: target } }] })
  await app.settle({ maxMs: SETTLE_MS })
  expect(dist(worldPosition(wrist), target)).toBeLessThan(1e-3)
  expect(ik.reach(0)).toBeLessThan(1e-3)
  // A joint outside the subtree is refused.
  let stray = createGroup()
  add(scene.root, stray)
  expect(() => createIk(rig, { solvers: [{ kind: "lookAt", bones: [stray], target: { point: target } }] })).toThrow("not under the root")
})
