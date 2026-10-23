// The mixer's layered model (agents/animation.md Animation, mixer.ts)
// pinned against a running scene: an override layer replaces its masked
// joints and nothing else, a stop-with-fade lands on rest, an additive
// layer adds its clip's deltas, markers and curves reach the facade, a
// window ends on a stop, a subclip holds within its window, and the rig
// survives a scene re-enter. The rig is core state over live scene nodes,
// so this is an app test; every clip here is one or two keys, so the app
// settles.

import { test, expect } from "@solidrt/test"
import { add, createIk, createMixer, createModel, createScene, getTransform, remove } from "../src/index.ts"
import type { MarkerEvent, Model, ModelChannel, ModelClip, ModelData } from "../src/index.ts"
import type { Scene } from "../src/scene.ts"
import type { TestApp } from "@solidrt/test"

const SIZE = 32
const SETTLE_MS = 3000

async function mounted(app: TestApp): Promise<Scene> {
  let scene!: Scene
  await app.mount(() => {
    scene = createScene(SIZE, SIZE, { label: "mixer" })
    return <texture src={scene.texture} width={SIZE} height={SIZE} />
  })
  return scene
}

/** A constant position channel on `node`. */
function at(node: number, x: number): ModelChannel {
  return { node, path: "position", interpolation: "linear", times: new Float32Array([0]), values: new Float32Array([x, 0, 0]) }
}

/** A position channel on `node` sliding from `from` to `to` over `seconds`. */
function slide(node: number, from: number, to: number, seconds: number): ModelChannel {
  return { node, path: "position", interpolation: "linear", times: new Float32Array([0, seconds]), values: new Float32Array([from, 0, 0, to, 0, 0]) }
}

/** A meshless model: a root with two joints (1 and 2) at rest x = 0, and
 * the given clips. */
function rig(scene: Scene, clips: ModelClip[]): Model {
  let data: ModelData = {
    nodes: [
      { name: "root", parent: null, position: [0, 0, 0], rotation: [0, 0, 0, 1], scale: [1, 1, 1] },
      { name: "spine", parent: 0, position: [0, 0, 0], rotation: [0, 0, 0, 1], scale: [1, 1, 1] },
      { name: "head", parent: 1, position: [0, 0, 0], rotation: [0, 0, 0, 1], scale: [1, 1, 1] },
    ],
    parts: [],
    skins: [],
    clips,
    materials: [],
    images: [],
    bounds: new Float32Array([-1, 0, -1, 1, 1, 1]),
  }
  let model = createModel(data, { label: "rig", autoFree: false })
  add(scene.root, model)
  return model
}

let x = (model: Model, i: number): number => getTransform(model.nodes[i]!.node).position[0]

test("an override layer replaces its masked joints only; a layer fade reveals the base", async app => {
  let scene = await mounted(app)
  let model = rig(scene, [
    { name: "walk", duration: 0, channels: [at(1, 1), at(2, 1)] },
    { name: "aim", duration: 0, channels: [at(1, 9), at(2, 9)] },
  ])
  let mixer = createMixer(model, { layers: { aim: { mask: { include: ["head"] } } } })
  mixer.play("walk", { loop: false })
  mixer.play("aim", { layer: "aim", loop: false })
  await app.settle({ maxMs: SETTLE_MS })
  expect(x(model, 1)).toBeCloseTo(1, 4)
  expect(x(model, 2)).toBeCloseTo(9, 4)
  // A subtree mask takes the head with the spine.
  mixer.layer("aim", { mask: { include: [{ node: "spine", subtree: true }] } })
  await app.settle({ maxMs: SETTLE_MS })
  expect(x(model, 1)).toBeCloseTo(9, 4)
  // The layer's weight ramps in core: at 0 the base shows again.
  mixer.setLayerWeight("aim", 0, { fadeMs: 100 })
  await app.settle({ maxMs: SETTLE_MS })
  expect(x(model, 1)).toBeCloseTo(1, 4)
  expect(x(model, 2)).toBeCloseTo(1, 4)
  expect(mixer.playing("aim")).toEqual(["aim"])
})

test("a stop with a fade lands on rest; an additive layer adds its deltas", async app => {
  let scene = await mounted(app)
  let model = rig(scene, [
    { name: "pose", duration: 0, channels: [at(1, 4)] },
    // Authored from x = 1: a delta of 2 by its end.
    { name: "flinch", duration: 0.2, channels: [slide(1, 1, 3, 0.2)] },
  ])
  let mixer = createMixer(model, { layers: { hit: { mode: "additive" } } })
  mixer.play("pose", { loop: false })
  await app.settle({ maxMs: SETTLE_MS })
  expect(x(model, 1)).toBeCloseTo(4, 4)
  let finished: string[] = []
  mixer.onFinish = name => finished.push(name)
  mixer.play("flinch", { layer: "hit", loop: false })
  await app.settle({ maxMs: SETTLE_MS })
  expect(x(model, 1)).toBeCloseTo(6, 3)
  expect(finished).toEqual(["flinch"])
  // Stopped with a fade, the pose fades to rest rather than popping.
  mixer.stop({ fadeMs: 100 })
  await app.settle({ maxMs: SETTLE_MS })
  expect(x(model, 1)).toBeCloseTo(0, 3)
  expect(mixer.playing()).toEqual([])
})

test("markers fire with their data, a window ends on a stop, a curve reads back, a subclip holds its window", async app => {
  let scene = await mounted(app)
  let model = rig(scene, [
    {
      name: "swing",
      duration: 1,
      channels: [slide(1, 0, 10, 1)],
      markers: [
        { time: 0.5, name: "hit", data: { power: 2 } },
        { time: 0.2, name: "active", duration: 0.6 },
      ],
      curves: [{ name: "grip", times: new Float32Array([0, 1]), values: new Float32Array([0, 1]) }],
    },
  ])
  let mixer = createMixer(model)
  let events: MarkerEvent[] = []
  mixer.onMarker = e => events.push(e)
  mixer.play("swing", { loop: false })
  await app.settle({ maxMs: SETTLE_MS })
  expect(events.map(e => e.name + ":" + e.phase).join()).toBe("active:begin,hit:instant,active:end")
  expect(events[1]!.data).toEqual({ power: 2 })
  expect(events[1]!.weight).toBeCloseTo(1, 4)
  expect(events[1]!.layer).toBe("base")
  expect(mixer.curve("grip")).toBeCloseTo(1, 4)
  expect(mixer.time("swing")).toBeCloseTo(1, 4)
  // A seek back into the window begins it; a stop inside ends it; both
  // report at once, from the call itself.
  mixer.seek("swing", 0.3)
  mixer.stop()
  expect(events.slice(3).map(e => e.name + ":" + e.phase).join()).toBe("active:begin,active:end")
  // A subclip: the second half, held at its end.
  mixer.play("swing", { start: 0.5, end: 0.8, loop: false })
  await app.settle({ maxMs: SETTLE_MS })
  expect(x(model, 1)).toBeCloseTo(8, 3)
  expect(mixer.time("swing")).toBeCloseTo(0.8, 4)
})

test("a curve drives an IK weight; the rig survives a scene re-enter", async app => {
  let scene = await mounted(app)
  let model = rig(scene, [
    { name: "pose", duration: 0, channels: [at(1, 2)], curves: [{ name: "aim", times: new Float32Array([0]), values: new Float32Array([0.5]) }] },
  ])
  let mixer = createMixer(model)
  let ik = createIk(model, { solvers: [{ kind: "lookAt", bones: [model.nodes[2]!.node], target: { point: [10, 0, 0] }, weight: 0, easeMs: 0 }] })
  ik.setWeight(0, { mixer, curve: "aim" })
  mixer.play("pose", { loop: false })
  await app.settle({ maxMs: SETTLE_MS })
  expect(x(model, 1)).toBeCloseTo(2, 4)
  // Half the aim: the head turned half way toward +x (45 degrees about y).
  let q = getTransform(model.nodes[2]!.node).quaternion
  expect(Math.abs(2 * Math.atan2(q[1], q[3]))).toBeCloseTo(Math.PI / 4, 2)
  // Out and back in: the rig re-binds; nothing plays until play().
  remove(model)
  await app.settle({ maxMs: SETTLE_MS })
  expect(mixer.playing()).toEqual([])
  add(scene.root, model)
  mixer.play("pose", { loop: false })
  await app.settle({ maxMs: SETTLE_MS })
  expect(x(model, 1)).toBeCloseTo(2, 4)
  mixer.dispose()
  ik.dispose()
})
