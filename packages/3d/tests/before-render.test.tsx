// A write made in a frame is in that frame's paint: the scene publishes
// its pending record bytes in the publish pass of onBeforeRender, ahead of
// renderFrame(), so a record mesh written from onFrame draws the written
// records with the written count in the same frame - never the new count
// over the previous bytes (stale records in the slots past the old count,
// after a shrink), never a grown population from its still-empty
// replacement buffers, never a frame late. The scene is GPU state, so this
// is an app test, and the probe is the frame itself: a capture of the leaf
// showing the scene, requested from inside the frame after the write, is
// serviced by that frame's own paint (a readback would re-render the scene
// first and see the write wherever it landed). A display-space scene, so
// the buffer is the displayable rgba8 texture and a white box reads 255.

import { test, expect } from "@solidrt/test"
import type { Pixels, RefLocator, TestApp } from "@solidrt/test"
import { onFrame } from "@solidrt/core"
import { captureSnapshot, glsl } from "@solidrt/core/gpu"
import { add, box, createRecordMesh, createScene, setRecords, shaderMaterialClass } from "../src/index.ts"

const SIZE = 64
// The orthographic view: 8 world units across, so a unit box at x = -3 is
// drawn an eighth of the way in from the left edge, one at x = 3 an eighth
// in from the right.
const HALF_EXTENT = 4
const LEFT = -3
const RIGHT = 3
const WHITE = [255, 255, 255, 255]
const CLEAR = [0, 0, 0, 255]

/** Where a world x is drawn, as a fraction of the view's width. */
let across = (x: number) => (x + HALF_EXTENT) / (2 * HALF_EXTENT)

/** Records placing one unit box per x, in the material's iPos layout. */
function records(...xs: number[]): Float32Array {
  let out = new Float32Array(xs.length * 3)
  xs.forEach((x, i) => (out[i * 3] = x))
  return out
}

function world(initial: Float32Array, count: number) {
  let scene = createScene(SIZE, SIZE, { blendSpace: "display", clearColor: [0, 0, 0, 1], label: "before-render" })
  scene.setCamera({
    position: [0, 0, 10],
    target: [0, 0, 0],
    ortho: { left: -HALF_EXTENT, right: HALF_EXTENT, bottom: -HALF_EXTENT, top: HALF_EXTENT },
  })
  let look = shaderMaterialClass({
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
    label: "before-render-records",
  })
  let mesh = createRecordMesh(box(), look.instance(), initial, count)
  add(scene.root, mesh)
  return { scene, mesh }
}

// The scene and its mesh own GPU resources, so they are created under the
// mounted app's root.
async function mounted(app: TestApp, initial: Float32Array, count: number) {
  let leaf = app.ref()
  let w!: ReturnType<typeof world>
  await app.mount(() => {
    w = world(initial, count)
    return <texture ref={leaf} src={w.scene.texture} width={SIZE} height={SIZE} />
  })
  return { ...w, leaf }
}

/**
 * Runs `write` among the next frame's callbacks and captures `leaf` from
 * inside that frame: what that frame's own paint drew. The capture's
 * promise settles with the frame after, so two frames run.
 */
async function painted(app: TestApp, leaf: RefLocator, write: () => void): Promise<Pixels> {
  let shot!: Promise<Pixels>
  let stop = onFrame(() => {
    stop()
    write()
    shot = captureSnapshot(leaf.record.id)
  })
  await app.frame()
  await app.frame()
  return shot
}

/** The pixel at a fraction of the way across and down `pixels`. */
function at(pixels: Pixels, fx: number, fy: number): number[] {
  let x = Math.floor(fx * pixels.width)
  let y = Math.floor(fy * pixels.height)
  let i = (y * pixels.width + x) * 4
  return [pixels.data[i]!, pixels.data[i + 1]!, pixels.data[i + 2]!, pixels.data[i + 3]!]
}

test("records written from onFrame draw in that frame with their count, never the new count over the old bytes", async app => {
  // One record on the left, which nothing draws (count 0): the bytes the
  // buffer holds when the frame rewrites it.
  let { mesh, leaf } = await mounted(app, records(LEFT), 0)
  let frame = await painted(app, leaf, () => setRecords(mesh, records(RIGHT), 1))
  expect(at(frame, across(RIGHT), 0.5)).toEqual(WHITE)
  expect(at(frame, across(LEFT), 0.5)).toEqual(CLEAR)
})

test("a write past capacity draws the grown population in that frame, not its empty replacement buffers", async app => {
  let { mesh, leaf } = await mounted(app, records(LEFT), 1)
  let before = await painted(app, leaf, () => {})
  expect(at(before, across(LEFT), 0.5)).toEqual(WHITE)
  // Two records into a capacity of one: the buffers are replaced.
  let frame = await painted(app, leaf, () => setRecords(mesh, records(LEFT, RIGHT), 2))
  expect(at(frame, across(LEFT), 0.5)).toEqual(WHITE)
  expect(at(frame, across(RIGHT), 0.5)).toEqual(WHITE)
})
