// The record mirror, as a painted frame sees it: records(mesh) is the
// mirror of a population's first stream (a record mesh's records, an
// instanced mesh's style records) and updateRecords publishes a RANGE of
// it (a record left out keeps drawing what the GPU holds); growth replaces
// the mirror, so a re-read sees the grown capacity and a hoisted view is a
// dead copy; a transferred population has no mirror to hand out. The scene
// is GPU state, so this is an app test, and the probe is the frame itself:
// `app.painted` reads the leaf showing the scene as that frame's own paint
// drew it. A display-space scene, so a white box reads 255. The same
// verbs, one dimension down, are pinned by packages/2d/tests/records.test.tsx.

import { test, expect } from "@solidrt/test"
import type { Pixels, RefLocator, TestApp } from "@solidrt/test"
import { onFrame } from "@solidrt/core"
import { glsl } from "@solidrt/core/gpu"
import { add, addInstance, box, createInstancedMesh, createRecordMesh, createScene, disposeInstances, records, setRecords, shaderMaterialClass, unlit, updateRecords } from "../src/index.ts"
import type { InstancedMeshNode, RecordMeshNode, SceneHandle } from "../src/index.ts"

const SIZE = 64
// The orthographic view: 8 world units across, so a unit box at x = -3 is
// drawn an eighth of the way in from the left edge, one at x = 3 an eighth
// in from the right.
const HALF_EXTENT = 4
const LEFT = -3
const RIGHT = 3
// Floats per record of the test material: iPos, float32x3.
const RECORD_FLOATS = 3
// Components of the stock instanceColors style record (float16x4).
const COLOR_COMPONENTS = 4
const WHITE = [255, 255, 255, 255]
const RED = [255, 0, 0, 255]
const CLEAR = [0, 0, 0, 255]

/** Where a world x is drawn, as a fraction of the view's width. */
let across = (x: number) => (x + HALF_EXTENT) / (2 * HALF_EXTENT)

/** Records placing one unit box per x, in the material's iPos layout. */
function placed(...xs: number[]): Float32Array {
  let out = new Float32Array(xs.length * RECORD_FLOATS)
  xs.forEach((x, i) => (out[i * RECORD_FLOATS] = x))
  return out
}

function scene(): SceneHandle {
  let s = createScene(SIZE, SIZE, { blendSpace: "display", clearColor: [0, 0, 0, 1], label: "records" })
  s.setCamera({
    position: [0, 0, 10],
    target: [0, 0, 0],
    ortho: { left: -HALF_EXTENT, right: HALF_EXTENT, bottom: -HALF_EXTENT, top: HALF_EXTENT },
  })
  return s
}

let look = () =>
  shaderMaterialClass({
    vertex: glsl`
      in vec3 aPos;
      in vec3 iPos;
      uniform mat4 uModel;
      uniform mat4 uViewProj;

      void main() {
        gl_Position = uViewProj * uModel * vec4(aPos + iPos, 1.0);
      }
    `,
    fragment: glsl`
      void main() {
        fragColor = vec4(1.0);
      }
    `,
    instanceBuffers: [{ attributes: [{ name: "iPos", format: "float32x3" }] }],
    label: "records-look",
  })

/** A record mesh of `initial` records drawing `count` of them, in a scene. */
function recordWorld(initial: Float32Array, count: number) {
  let s = scene()
  let mesh = createRecordMesh(box(), look().instance(), initial, count)
  add(s.root, mesh)
  return { scene: s, mesh }
}

/** An instanced mesh with one instance at the origin under the stock
 * instanced unlit material, whose style record is its instance color. */
function instancedWorld() {
  let s = scene()
  let mesh = createInstancedMesh(box(), unlit({ instanced: true, instanceColors: true }), { capacity: 1, label: "records-instanced" })
  add(s.root, mesh)
  addInstance(mesh)
  return { scene: s, mesh }
}

// The scene and its mesh own GPU resources, so they are created under the
// mounted app's root.
async function mounted<T extends { scene: SceneHandle }>(app: TestApp, build: () => T): Promise<T & { leaf: RefLocator }> {
  let leaf = app.ref()
  let w!: T
  await app.mount(() => {
    w = build()
    return <texture ref={leaf} src={w.scene.texture} width={SIZE} height={SIZE} />
  })
  return { ...w, leaf }
}

/** Runs `write` among the next frame's callbacks; what that frame drew of `leaf`. */
function painted(app: TestApp, leaf: RefLocator, write: () => void): Promise<Pixels> {
  let stop = onFrame(() => {
    stop()
    write()
  })
  return app.painted(leaf)
}

/** The pixel at a fraction of the way across and down `pixels`. */
function at(pixels: Pixels, fx: number, fy: number): number[] {
  let x = Math.floor(fx * pixels.width)
  let y = Math.floor(fy * pixels.height)
  let i = (y * pixels.width + x) * 4
  return [pixels.data[i]!, pixels.data[i + 1]!, pixels.data[i + 2]!, pixels.data[i + 3]!]
}

test("records(mesh) is the mirror; updateRecords publishes the range it names and a record left out keeps drawing what the GPU holds", async app => {
  let { mesh, leaf } = await mounted(app, () => recordWorld(placed(LEFT, LEFT), 2))
  let before = await painted(app, leaf, () => {})
  expect(at(before, across(LEFT), 0.5)).toEqual(WHITE)
  expect(at(before, across(RIGHT), 0.5)).toEqual(CLEAR)
  // Both records move right in the mirror; only the first publishes.
  let partial = await painted(app, leaf, () => {
    let r = records(mesh) as Float32Array
    r[0 * RECORD_FLOATS] = RIGHT
    r[1 * RECORD_FLOATS] = RIGHT
    updateRecords(mesh, { first: 0, count: 1 })
  })
  expect(at(partial, across(RIGHT), 0.5)).toEqual(WHITE)
  expect(at(partial, across(LEFT), 0.5)).toEqual(WHITE)
  let whole = await painted(app, leaf, () => updateRecords(mesh))
  expect(at(whole, across(LEFT), 0.5)).toEqual(CLEAR)
  expect(at(whole, across(RIGHT), 0.5)).toEqual(WHITE)
})

test("growth replaces the mirror: a re-read sees the grown capacity, a hoisted view is a dead copy", async app => {
  let { mesh, leaf } = await mounted(app, () => recordWorld(placed(LEFT), 1))
  let old = records(mesh) as Float32Array
  expect(old.length).toBe(RECORD_FLOATS)
  // Two records into a capacity of one: the buffers and the mirror are
  // replaced.
  let grown = await painted(app, leaf, () => setRecords(mesh, placed(LEFT, RIGHT), 2))
  expect(at(grown, across(LEFT), 0.5)).toEqual(WHITE)
  expect(at(grown, across(RIGHT), 0.5)).toEqual(WHITE)
  expect(records(mesh)).not.toBe(old)
  expect(records(mesh).byteLength).toBe(2 * RECORD_FLOATS * Float32Array.BYTES_PER_ELEMENT)
  // The old view publishes nothing; the new one publishes.
  let dead = await painted(app, leaf, () => {
    old[0] = RIGHT
    updateRecords(mesh, { first: 0, count: 1 })
  })
  expect(at(dead, across(LEFT), 0.5)).toEqual(WHITE)
  let live = await painted(app, leaf, () => {
    ;(records(mesh) as Float32Array)[1 * RECORD_FLOATS] = LEFT
    updateRecords(mesh, { first: 1, count: 1 })
  })
  expect(at(live, across(RIGHT), 0.5)).toEqual(CLEAR)
})

test("an instanced mesh's records(mesh) is its style stream, written in bulk as the layout's own format", async app => {
  let { mesh, leaf } = await mounted(app, () => instancedWorld())
  let before = await painted(app, leaf, () => {})
  expect(at(before, 0.5, 0.5)).toEqual(WHITE)
  let tinted = await painted(app, leaf, () => {
    let bytes = records(mesh)
    expect(bytes instanceof Float32Array).toBe(false)
    let colors = new Float16Array(bytes.buffer, bytes.byteOffset, COLOR_COMPONENTS)
    colors.set([1, 0, 0, 1])
    updateRecords(mesh)
  })
  expect(at(tinted, 0.5, 0.5)).toEqual(RED)
})

test("a transferred population has no mirror: records() throws, as updateRecords does", async app => {
  let transferred!: RecordMeshNode
  let plain!: InstancedMeshNode
  await app.mount(() => {
    transferred = createRecordMesh(box(), look().instance(), placed(LEFT), 1, { transfer: true })
    plain = createInstancedMesh(box(), unlit({ instanced: true, instanceColors: true }), { capacity: 1 })
    return null
  })
  expect(() => records(transferred)).toThrow()
  expect(() => updateRecords(transferred)).toThrow()
  expect(() => records(plain, 1)).toThrow()
  disposeInstances(transferred)
  disposeInstances(plain)
})
