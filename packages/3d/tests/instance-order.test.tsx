// Instance order on a record mesh (instanceOrder), as the painted frame
// sees it: every camera view draws its own draw list, sorted under its
// own camera, and a write publishes only its dirty range. The quads are
// opaque colors under a blending material with no depth write, so the
// last one drawn is what the center shows: the order is a pixel read. The
// scene is GPU state, so this is an app test.

import { test, expect } from "@solidrt/test"
import type { Pixels, RefLocator, TestApp } from "@solidrt/test"
import { onFrame } from "@solidrt/core"
import { glsl } from "@solidrt/core/gpu"
import type { TextureId } from "@solidrt/core/gpu"
import { add, createRecordMesh, createScene, plane, records, shaderMaterialClass, updateRecords } from "../src/index.ts"
import type { InstanceOrderOptions, SceneHandle, Vec3 } from "../src/index.ts"

const SIZE = 64
// One record: iPos float32x3, iColor float32x4, iKey float32.
const RECORD_FLOATS = 8
const KEY_FIELD = 7
const RED = [255, 0, 0, 255]
const GREEN = [0, 255, 0, 255]
const BLUE = [0, 0, 255, 255]
const FRONT: Vec3 = [0, 0, 5]
const BEHIND: Vec3 = [0, 0, -9]
const TARGET: Vec3 = [0, 0, -2]

let look = () =>
  shaderMaterialClass({
    vertex: glsl`
      in vec3 aPos;
      in vec3 iPos;
      in vec4 iColor;
      in float iKey;
      uniform mat4 uModel;
      uniform mat4 uViewProj;
      out vec4 vColor;
      void main() {
        gl_Position = uViewProj * uModel * vec4(aPos + iPos, 1.0);
        vColor = iColor;
      }
    `,
    fragment: glsl`
      in vec4 vColor;
      void main() {
        fragColor = vColor;
      }
    `,
    instanceBuffers: [
      {
        attributes: [
          { name: "iPos", format: "float32x3" },
          { name: "iColor", format: "float32x4" },
          { name: "iKey", format: "float32" },
        ],
      },
    ],
    transparent: true,
    cull: "none",
    label: "order-look",
  })

/** Records of one quad each: [x, y, z], an opaque color, a key. */
function quads(...rows: [Vec3, number[], number][]): Float32Array {
  let out = new Float32Array(rows.length * RECORD_FLOATS)
  rows.forEach(([p, c, key], i) => {
    out.set(p, i * RECORD_FLOATS)
    out.set([c[0]! / 255, c[1]! / 255, c[2]! / 255, 1], i * RECORD_FLOATS + 3)
    out[i * RECORD_FLOATS + KEY_FIELD] = key
  })
  return out
}

function world(data: Float32Array, order: InstanceOrderOptions) {
  let scene = createScene(SIZE, SIZE, { blendSpace: "display", clearColor: [0, 0, 0, 1], label: "order" })
  scene.setCamera({ position: FRONT, target: TARGET })
  let mesh = createRecordMesh(plane({ width: 1.5, height: 1.5 }), look().instance(), data, data.length / RECORD_FLOATS, {
    instanceOrder: order,
    bounds: [-1, -1, -4, 1, 1, 0],
  })
  add(scene.root, mesh)
  return { scene, mesh }
}

async function mounted<T extends { scene: SceneHandle }>(app: TestApp, build: () => T, extra?: (w: T) => TextureId): Promise<T & { leaves: RefLocator[] }> {
  let leaves = [app.ref(), app.ref()]
  let w!: T
  await app.mount(() => {
    w = build()
    let second = extra?.(w)
    return (
      <view>
        <texture ref={leaves[0]} src={w.scene.texture} width={SIZE} height={SIZE} />
        {second !== undefined && <texture ref={leaves[1]} src={second} width={SIZE} height={SIZE} />}
      </view>
    )
  })
  return { ...w, leaves }
}

/** Runs `write` among the next frame's callbacks; what that frame drew of `leaf`. */
function painted(app: TestApp, leaf: RefLocator, write: () => void = () => {}): Promise<Pixels> {
  let stop = onFrame(() => {
    stop()
    write()
  })
  return app.painted(leaf)
}

function center(pixels: Pixels): number[] {
  let i = ((pixels.height >> 1) * pixels.width + (pixels.width >> 1)) * 4
  return [pixels.data[i]!, pixels.data[i + 1]!, pixels.data[i + 2]!, pixels.data[i + 3]!]
}

test("a field key orders the records, and a one-record write re-orders them", async app => {
  // Slot order red, green, blue; keys 2, 0, 1 draw green, blue, red.
  let { mesh, leaves } = await mounted(app, () =>
    world(
      quads(
        [[0, 0, -2], RED, 2],
        [[0, 0, -2], GREEN, 0],
        [[0, 0, -2], BLUE, 1],
      ),
      { field: "iKey" },
    ),
  )
  expect(center(await painted(app, leaves[0]!))).toEqual(RED)
  // Green's key alone rises past the others: a one-record range.
  let raised = await painted(app, leaves[0]!, () => {
    ;(records(mesh) as Float32Array)[1 * RECORD_FLOATS + KEY_FIELD] = 5
    updateRecords(mesh, { first: 1, count: 1 })
  })
  expect(center(raised)).toEqual(GREEN)
  // A tie with red's key keeps slot order: green, slot 1, still draws last.
  ;(records(mesh) as Float32Array)[1 * RECORD_FLOATS + KEY_FIELD] = 2
  expect(center(await painted(app, leaves[0]!, () => updateRecords(mesh, { first: 1, count: 1 })))).toEqual(GREEN)
})

test("every camera view sorts a position key under its own camera", async app => {
  // Near red, far green, mid blue, in a scrambled slot order. Back to
  // front from the front: green, blue, red; from behind: red, blue, green.
  let { leaves } = await mounted(
    app,
    () =>
      world(
        quads(
          [[0, 0, -1], RED, 0],
          [[0.2, 0, -3], GREEN, 0],
          [[0.1, 0, -2], BLUE, 0],
        ),
        { position: "iPos", descending: true },
      ),
    w => {
      let view = w.scene.createView({ width: SIZE, height: SIZE, clearColor: [0, 0, 0, 1], label: "order-behind" })
      view.setCamera({ position: BEHIND, target: TARGET })
      return view.texture
    },
  )
  expect(center(await painted(app, leaves[0]!))).toEqual(RED)
  expect(center(await painted(app, leaves[1]!))).toEqual(GREEN)
})
