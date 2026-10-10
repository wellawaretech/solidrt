// Native motion steps before the frame's JS (flux's frame protocol:
// `advance` steps the clip players and the node transitions, then
// `deliver` hands the frame to JS), pinned against a running scene: a
// pose read in onFrame is this frame's, not the last one's; a node
// created with `from` starts its enter after the frame's callbacks, so
// the late pass and the paint see from, never its created pose; a settle
// handler runs in the frame's turn ahead of its onFrame callbacks; and a
// casting light moved only by a transition has its shadow cameras placed
// in the frame it moves, exactly where a JS write of the same pose
// places them (the scene's sync runs in every publish pass, written or
// not). The scene is GPU state, so this is an app test, and frames are
// stepped so each read is at a known app time.

import { test, expect } from "@solidrt/test"
import type { TestApp } from "@solidrt/test"
import { onBeforeRender, onFrame } from "@solidrt/core"
import { add, box, createGroup, createMesh, createScene, createSpotLight, getTransform, plane, setCastShadow, setTransform, setTransition, standard } from "../src/index.ts"
import type { SceneNode } from "../src/index.ts"
import type { Scene } from "../src/scene.ts"

const SIZE = 32
const LABEL = "native-motion"
// A linear tween over a second to x = 10: app time in ms maps to x / 100.
const DURATION = 1000
const TARGET_X = 10
// A pose read is exact to float rounding; a frame-stale one is a whole
// frame off (0.17 at 60 fps), so the tolerance only absorbs rounding.
const EPSILON = 1e-3
// The floor lies flat: the plane's +z normal turned up by a quarter turn
// about x.
const FLAT: [number, number, number, number] = [-Math.SQRT1_2, 0, 0, Math.SQRT1_2]

/** A scene with a floor that receives and a box that casts. */
async function mounted(app: TestApp): Promise<Scene> {
  let scene!: Scene
  await app.mount(() => {
    scene = createScene(SIZE, SIZE, { label: LABEL })
    let floor = createMesh(plane({ width: 20, height: 20 }), standard())
    setTransform(floor, { quaternion: FLAT })
    add(scene.root, floor)
    let caster = createMesh(box(), standard())
    setCastShadow(caster, true)
    setTransform(caster, { position: [0, 1, 0] })
    add(scene.root, caster)
    return <texture src={scene.texture} width={SIZE} height={SIZE} />
  })
  return scene
}

/** A group under `parent` whose position tweens linearly, sent to x = 10. */
function mover(parent: SceneNode, duration = DURATION): SceneNode {
  let node = createGroup()
  add(parent, node)
  setTransition(node, { position: { duration, curve: "linear" } })
  setTransform(node, { position: [TARGET_X, 0, 0] })
  return node
}

let x = (node: SceneNode) => getTransform(node).position[0]

/** Where a linear mover started at `since` is at app time `tick`. */
let expected = (tick: number, since: number) => ((tick - since) * TARGET_X) / DURATION

/** The receiving scene target's shadow matrices, as the engine holds them. */
function shadowMatrix(app: TestApp): number[] {
  let pipelines = app.gpu({ label: LABEL }).pipelines as { kind: string; label?: string; params: Record<string, unknown> }[]
  let target = pipelines.find(p => p.kind === "draws" && p.label === LABEL)
  if (target === undefined) throw new Error("The scene's own draw target is not in the inventory")
  return target.params.uShadowMatrix as number[]
}

test("a transitioned node's pose read in onFrame is this frame's, not the last frame's", async app => {
  let scene = await mounted(app)
  // The mount frame is the bootstrap, which stamps no clock: a track
  // written before the first stamp is anchored at the first frame that
  // runs (the startup anchor), so one frame first, and the write below
  // is stamped at the clock as of that frame.
  await app.frame()
  let node = mover(scene.root)
  let since = app.time
  let seen: [tick: number, x: number][] = []
  let stop = onFrame(tick => {
    seen.push([tick, x(node)])
  })
  await app.advance(DURATION / 4)
  stop()
  expect(seen.length).toBeGreaterThan(3)
  for (let [tick, px] of seen) expect(Math.abs(px - expected(tick, since))).toBeLessThan(EPSILON)
})

test("a node created with from starts after the frame's callbacks: the late pass and the paint see from, never its created pose", async app => {
  let scene = await mounted(app)
  let node = createGroup()
  setTransition(node, { position: { duration: DURATION, curve: "linear", from: [-TARGET_X, 0, 0] } })
  setTransform(node, { position: [TARGET_X, 0, 0] })
  let inCallbacks: number | null = null
  let inLatePass: number | null = null
  let stopFrame = onFrame(() => {
    if (inCallbacks === null) inCallbacks = x(node)
  })
  let stopLate = onBeforeRender(() => {
    if (inLatePass === null) inLatePass = x(node)
  })
  add(scene.root, node)
  await app.frame()
  stopFrame()
  stopLate()
  // The frame's callbacks read what the creating code wrote (Unity's
  // Update after Instantiate); the start pass then snaps to from, so the
  // late pass reads the pose the frame draws, at progress 0.
  expect(inCallbacks).toBe(TARGET_X)
  expect(inLatePass).toBe(-TARGET_X)
  expect(x(node)).toBe(-TARGET_X)
  await app.frame()
  // The next frame's step moved it one frame along, from from.
  expect(x(node)).toBeGreaterThan(-TARGET_X)
  expect(x(node)).toBeLessThan(-TARGET_X + expected(2 * (1000 / 60), 0))
})

test("a settle handler runs in the frame's turn, ahead of its onFrame callbacks", async app => {
  let scene = await mounted(app)
  let long = mover(scene.root)
  let short = mover(scene.root, DURATION / 10)
  let atSettle: number | null = null
  let atFrame: number | null = null
  short.onTransitionEnd = () => {
    atSettle = x(long)
  }
  let stop = onFrame(() => {
    if (atSettle !== null && atFrame === null) atFrame = x(long)
  })
  await app.advance(DURATION / 4)
  stop()
  expect(atSettle).not.toBeNull()
  // The handler fired after the step that moved the long one to this
  // frame's pose, so it read exactly what the frame's callbacks read; a
  // handler run after the previous frame's callbacks would be a frame of
  // progress behind them.
  expect(atFrame).toBe(atSettle)
})

test("a casting light moved only by a transition has its shadow placed in the frame it moves, where a write of the same pose places it", async app => {
  let scene = await mounted(app)
  let rig = createGroup()
  add(scene.root, rig)
  let light = createSpotLight({ castShadow: true, direction: [0, -1, 0], angle: 45 })
  setTransform(light, { position: [0, 4, 0] })
  add(rig, light)
  await app.frame()
  let before = shadowMatrix(app)
  setTransition(rig, { position: { duration: DURATION, curve: "linear" } })
  setTransform(rig, { position: [TARGET_X, 0, 0] })
  await app.advance(DURATION / 4)
  let moving = shadowMatrix(app)
  expect(moving).not.toEqual(before)
  await app.frame()
  expect(shadowMatrix(app)).not.toEqual(moving)
  // Frozen mid-flight: the matrix the follow placed for the pose the core
  // holds is the one a JS write of that pose places (through a different
  // pose first, so the write really re-places).
  scene.setTimeScale(0)
  await app.frame()
  let followed = shadowMatrix(app)
  let frozen = getTransform(rig).position
  setTransition(rig, null)
  setTransform(rig, { position: [frozen[0] + 1, frozen[1], frozen[2]] })
  await app.frame()
  expect(shadowMatrix(app)).not.toEqual(followed)
  setTransform(rig, { position: frozen })
  await app.frame()
  expect(shadowMatrix(app)).toEqual(followed)
})
