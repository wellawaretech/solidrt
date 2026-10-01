// The buffer write lease (beginBufferWrite/endBufferWrite): the throw
// matrix, the happy path read back through readTexture, the recycle path
// across frames, a prefix publish with setDraw, a cancelled publish, and a
// destroy mid-lease. The lease is GPU state, so this is an app test; a
// publish draws with the next frame, which the test runs before it reads.
// Three quads at fixed positions, colors chosen so a wrong pixel names
// the wrong step.

import { test, expect } from "@solidrt/test"
import type { TestApp } from "@solidrt/test"
import { beginBufferWrite, createBuffer, createPipelineTexture, destroyBuffer, endBufferWrite, glsl, readTexture, setDraw } from "../src/gpu.ts"
import type { TextureId } from "../src/gpu.ts"

const SIZE = 64
// Floats per record: center vec2, half-size f32, tint vec3.
const RECORD = 6
const CAPACITY = 4

let VERTEX = glsl`
  in vec2 aPos;
  in vec2 iCenter;
  in float iSize;
  in vec3 iTint;
  out vec3 vTint;

  void main() {
    gl_Position = vec4(iCenter + aPos * iSize, 0.0, 1.0);
    vTint = iTint;
  }
`

let FRAGMENT = glsl`
  in vec3 vTint;

  void main() {
    fragColor = vec4(vTint, 1.0);
  }
`

/** The target's pixel at clip-space (cx, cy), as [r, g, b] 0..255. */
function pixelAt(id: TextureId, cx: number, cy: number): [number, number, number] {
  let { width, height, data } = readTexture(id)
  // Pipeline clip space is y-DOWN (gl_Position y = -1 is the top row, the
  // pixel contract in core gpu.ts), and rows read back top-to-bottom.
  let px = Math.round((cx * 0.5 + 0.5) * (width - 1))
  let py = Math.round((cy * 0.5 + 0.5) * (height - 1))
  let at = (py * width + px) * 4
  return [data[at]!, data[at + 1]!, data[at + 2]!]
}

/** One record: a quad at (cx, cy), half-size s, solid (r, g, b) 0..1. */
function writeRecord(out: Float32Array, slot: number, cx: number, cy: number, s: number, rgb: [number, number, number]) {
  let at = slot * RECORD
  out[at] = cx
  out[at + 1] = cy
  out[at + 2] = s
  out[at + 3] = rgb[0]
  out[at + 4] = rgb[1]
  out[at + 5] = rgb[2]
}

function rig() {
  let quad = createBuffer(new Float32Array([-0.5, -0.5, 0.5, -0.5, -0.5, 0.5, 0.5, 0.5]), { label: "lease-quad" })
  // The number overload: zeroed storage, filled through the lease.
  let records = createBuffer(CAPACITY * RECORD * 4, { label: "lease-records" })
  let target = createPipelineTexture(VERTEX, FRAGMENT, SIZE, SIZE, null, {
    label: "lease",
    topology: "triangle-strip",
    vertexCount: 4,
    buffers: [
      { attributes: [{ name: "aPos", format: "float32x2" }], buffer: quad },
      {
        stepMode: "instance",
        attributes: [
          { name: "iCenter", format: "float32x2" },
          { name: "iSize", format: "float32" },
          { name: "iTint", format: "float32x3" },
        ],
        buffer: records,
      },
    ],
    instanceCount: 0,
    clearColor: [0, 0, 0, 1],
  })
  return { records, target }
}

// The buffers and the target are GPU resources, so they are created under
// the mounted app's root.
async function mounted(app: TestApp) {
  let r!: ReturnType<typeof rig>
  await app.mount(() => {
    r = rig()
    return <texture src={r.target} width={SIZE} height={SIZE} />
  })
  return r
}

/** Two quads, left red and right green, published whole and drawn. */
async function twoQuads(app: TestApp) {
  let { records, target } = await mounted(app)
  let out = beginBufferWrite(records)
  writeRecord(out, 0, -0.5, 0, 0.4, [1, 0, 0])
  writeRecord(out, 1, 0.5, 0, 0.4, [0, 1, 0])
  endBufferWrite(records)
  setDraw(target, { instanceCount: 2 })
  await app.frame()
  return { records, target, out }
}

test("a lease is the buffer's floats; a double begin, an oversize publish and an end without a begin throw", async app => {
  let { records } = await mounted(app)
  let out = beginBufferWrite(records)
  expect(out.length).toBe(CAPACITY * RECORD)
  expect(() => beginBufferWrite(records)).toThrow()
  // An oversize publish closes the lease and throws...
  expect(() => endBufferWrite(records, CAPACITY * RECORD * 4 + 1)).toThrow()
  // ...so the view is detached and a fresh begin works.
  expect(out.buffer.byteLength).toBe(0)
  expect(() => endBufferWrite(records)).toThrow()
  expect(beginBufferWrite(records).length).toBe(CAPACITY * RECORD)
})

test("a publish detaches the view and draws the records with the next frame", async app => {
  let { target, out } = await twoQuads(app)
  expect(out.buffer.byteLength).toBe(0)
  expect(out[0]).toBe(undefined)
  expect(pixelAt(target, -0.5, 0)).toEqual([255, 0, 0])
  expect(pixelAt(target, 0.5, 0)).toEqual([0, 255, 0])
  expect(pixelAt(target, 0, 0.75)).toEqual([0, 0, 0])
})

test("a recycled lease publishes a prefix, and the draw shrinks to it", async app => {
  let { records, target } = await twoQuads(app)
  // Begin again (a pooled block, contents unspecified: rewrite everything),
  // publish a ONE-record prefix, shrink the draw.
  let out = beginBufferWrite(records)
  writeRecord(out, 0, 0, 0.5, 0.3, [0, 0, 1])
  endBufferWrite(records, RECORD * 4)
  setDraw(target, { instanceCount: 1 })
  await app.frame()
  expect(pixelAt(target, 0, 0.5)).toEqual([0, 0, 255])
  expect(pixelAt(target, -0.5, 0)).toEqual([0, 0, 0])
})

test("a publish of nothing leaves the pixels unchanged", async app => {
  let { records, target } = await twoQuads(app)
  beginBufferWrite(records)
  endBufferWrite(records, 0)
  await app.frame()
  expect(pixelAt(target, -0.5, 0)).toEqual([255, 0, 0])
  expect(pixelAt(target, 0.5, 0)).toEqual([0, 255, 0])
})

test("destroy mid-lease detaches the view, and later lease calls throw", async app => {
  let { records } = await mounted(app)
  let out = beginBufferWrite(records)
  destroyBuffer(records)
  expect(out.buffer.byteLength).toBe(0)
  expect(() => beginBufferWrite(records)).toThrow()
  expect(() => endBufferWrite(records)).toThrow()
})
