// The spring-bone contract (AGENTS.md Spring bones, springs.ts) pinned
// against a running scene: a chain under a still bone rests and asks for
// no frames, a turned bone drags it and it settles along the new rest
// direction, a frame's own write to the bone reaches the chain in the
// same frame (the pose post-pass), the declaration survives a scene
// re-enter, reset puts it back at rest, and dispose leaves the bones at
// rest. The chains are core state over live scene nodes, so this is an
// app test.

import { test, expect } from "@solidrt/test"
import type { TestApp } from "@solidrt/test"
import { onFrame } from "@solidrt/core"
import { add, createGroup, createModel, createScene, createSpringBones, getTransform, remove, setTransform } from "../src/index.ts"
import type { Model, ModelData, SceneNode } from "../src/index.ts"
import type { Scene } from "../src/scene.ts"

const SIZE = 32
/** The port's rig: two 10 cm bones hanging from an anchor at y = 1. */
const BONE = 0.1
/** Frames a chain with drag 0.4 needs to come to rest after a quarter
 * turn: a few hundred at 60 fps, well under this. */
const SETTLE_MS = 3000
/** A quarter turn about z: the rest direction swings from -y to +x. */
const QUARTER_Z: [number, number, number, number] = [0, 0, Math.SQRT1_2, Math.SQRT1_2]

async function mounted(app: TestApp): Promise<Scene> {
  let scene!: Scene
  await app.mount(() => {
    scene = createScene(SIZE, SIZE, { label: "springs" })
    return <texture src={scene.texture} width={SIZE} height={SIZE} />
  })
  return scene
}

/** A meshless model: anchor, bone 1 at the anchor, bone 2 at bone 1's tail. */
function rig(scene: Scene): { model: Model; anchor: SceneNode; b1: SceneNode; b2: SceneNode } {
  let data: ModelData = {
    nodes: [
      { name: "anchor", parent: null, position: [0, 1, 0], rotation: [0, 0, 0, 1], scale: [1, 1, 1] },
      { name: "b1", parent: 0, position: [0, 0, 0], rotation: [0, 0, 0, 1], scale: [1, 1, 1] },
      { name: "b2", parent: 1, position: [0, -BONE, 0], rotation: [0, 0, 0, 1], scale: [1, 1, 1] },
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
  return { model, anchor: model.nodes[0]!.node, b1: model.nodes[1]!.node, b2: model.nodes[2]!.node }
}

let w = (node: SceneNode) => getTransform(node).quaternion[3]

test("a chain under a still bone rests and asks for no frames; a turned bone drags it", async app => {
  let scene = await mounted(app)
  let { model, anchor, b1, b2 } = rig(scene)
  createSpringBones(model, { springs: [{ joints: [{ node: b1 }, { node: b2 }] }] })
  await app.settle({ maxMs: 500 })
  expect(Math.abs(w(b1))).toBeCloseTo(1, 6)
  expect(Math.abs(w(b2))).toBeCloseTo(1, 6)

  setTransform(anchor, { quaternion: QUARTER_Z })
  await app.advance(50)
  // Lagging: the bones have not followed the anchor all the way.
  expect(Math.abs(w(b1))).toBeLessThan(0.9999)
  await app.settle({ maxMs: SETTLE_MS })
  // Settled: back at its local rest, hanging along the anchor's turned
  // -y (world +x), and asking for no frames.
  expect(Math.abs(w(b1))).toBeCloseTo(1, 3)
  expect(Math.abs(w(b2))).toBeCloseTo(1, 3)
})

test("a frame's own write to the bone reaches the chain in the same frame", async app => {
  let scene = await mounted(app)
  let { model, anchor, b1, b2 } = rig(scene)
  createSpringBones(model, { springs: [{ joints: [{ node: b1 }, { node: b2 }] }] })
  await app.settle({ maxMs: 500 })
  // The anchor is turned from onFrame, the primary producer the post-pass
  // waits for: the chain lags in the frame of the turn itself.
  let frames = 0
  let lagged: number | null = null
  onFrame(() => {
    frames++
    if (frames === 1) setTransform(anchor, { quaternion: QUARTER_Z })
    if (frames === 2 && lagged === null) lagged = w(b1)
  })
  await app.advance(40)
  expect(lagged).not.toBeNull()
  expect(Math.abs(lagged!)).toBeLessThan(0.9999)
})

test("the declaration survives a scene re-enter, and reset puts the chain back at rest", async app => {
  let scene = await mounted(app)
  let { model, anchor, b1, b2 } = rig(scene)
  let springs = createSpringBones(model, { springs: [{ joints: [{ node: b1 }, { node: b2 }] }] })
  setTransform(anchor, { quaternion: QUARTER_Z })
  await app.advance(50)
  expect(Math.abs(w(b1))).toBeLessThan(0.9999)

  // Out and back in: the chain re-binds and swings again from rest.
  remove(model)
  await app.settle({ maxMs: 500 })
  setTransform(anchor, { quaternion: [0, 0, 0, 1] })
  add(scene.root, model)
  await app.settle({ maxMs: 500 })
  expect(Math.abs(w(b1))).toBeCloseTo(1, 6)
  setTransform(anchor, { quaternion: QUARTER_Z })
  await app.advance(50)
  expect(Math.abs(w(b1))).toBeLessThan(0.9999)

  // Reset: the next frame has the bones at rest under the turned anchor.
  springs.reset()
  await app.advance(20)
  expect(Math.abs(w(b1))).toBeCloseTo(1, 6)
  springs.dispose()
})

test("dispose puts the joints back at rest, and a new declaration takes over from there", async app => {
  let scene = await mounted(app)
  let { model, anchor, b1, b2 } = rig(scene)
  let springs = createSpringBones(model, { springs: [{ joints: [{ node: b1 }, { node: b2 }] }] })
  setTransform(anchor, { quaternion: QUARTER_Z })
  await app.advance(50)
  expect(Math.abs(w(b1))).toBeLessThan(0.9999)
  expect(Math.abs(w(b2))).toBeLessThan(0.9999)

  // Hair physics off mid-swing: the bones hang as authored, and stay.
  springs.dispose()
  await app.settle({ maxMs: 500 })
  expect(Math.abs(w(b1))).toBeCloseTo(1, 6)
  expect(Math.abs(w(b2))).toBeCloseTo(1, 6)
  await app.advance(50)
  expect(Math.abs(w(b1))).toBeCloseTo(1, 6)

  // On again: a new declaration starts at rest and swings from there.
  springs = createSpringBones(model, { springs: [{ joints: [{ node: b1 }, { node: b2 }] }] })
  setTransform(anchor, { quaternion: [0, 0, 0, 1] })
  await app.advance(50)
  expect(Math.abs(w(b1))).toBeLessThan(0.9999)
  springs.dispose()
})

test("a chain without its model is refused, and a joint outside it too", async app => {
  let scene = await mounted(app)
  let { model, b1, b2 } = rig(scene)
  let stray = createGroup()
  add(scene.root, stray)
  expect(() => createSpringBones(model, { springs: [{ joints: [{ node: stray }] }] })).toThrow("not under the root")
  expect(() => createSpringBones(model, { springs: [{ joints: [{ node: b2 }, { node: b1, tail: [0, -BONE, 0] }] }] })).toThrow("parents first")
  remove(model)
  expect(() => createSpringBones(model, { springs: [{ joints: [{ node: b1 }] }] })).toThrow("must be in a scene")
})

test("a force leans the chain and a joint update takes from where it is", async app => {
  let scene = await mounted(app)
  let { model, b1, b2 } = rig(scene)
  let springs = createSpringBones(model, { springs: [{ joints: [{ node: b1 }, { node: b2 }] }] })
  await app.settle({ maxMs: 500 })
  // A wind along +x leans the hanging chain; a constant one settles.
  springs.setForce([1, 0, 0])
  await app.advance(50)
  expect(Math.abs(w(b1))).toBeLessThan(0.9999)
  await app.settle({ maxMs: SETTLE_MS })
  let leaning = Math.abs(w(b1))
  expect(leaning).toBeLessThan(0.99)
  // Total drag and no stiffness freeze the chain where it is, wind off.
  springs.setForce([0, 0, 0])
  springs.setJoint(0, 0, { drag: 1, stiffness: 0 })
  springs.setJoint(0, 1, { drag: 1, stiffness: 0 })
  await app.advance(500)
  expect(Math.abs(w(b1))).toBeCloseTo(leaning, 3)
  expect(() => springs.setJoint(1, 0, {})).toThrow("no chain 1")
  expect(() => springs.setJoint(0, 2, {})).toThrow("no joint 2")
  expect(() => springs.setJoint(0, 0, { drag: 2 })).toThrow("drag")
})

test("a chain over plain nodes under a group is dragged like a model's", async app => {
  let scene = await mounted(app)
  let anchor = createGroup()
  let b1 = createGroup()
  let b2 = createGroup()
  setTransform(anchor, { position: [0, 1, 0] })
  setTransform(b2, { position: [0, -BONE, 0] })
  add(anchor, b1)
  add(b1, b2)
  add(scene.root, anchor)
  createSpringBones(anchor, { springs: [{ joints: [{ node: b1 }, { node: b2 }] }] })
  await app.settle({ maxMs: 500 })
  expect(Math.abs(w(b1))).toBeCloseTo(1, 6)
  setTransform(anchor, { quaternion: QUARTER_Z })
  await app.advance(50)
  expect(Math.abs(w(b1))).toBeLessThan(0.9999)
  await app.settle({ maxMs: SETTLE_MS })
  expect(Math.abs(w(b1))).toBeCloseTo(1, 3)
})
