// Check rig for the buffer write lease (beginBufferWrite/endBufferWrite):
// the happy path pixel-verified via readTexture, the recycle path across
// frames, prefix publishing with setDraw, and the full throw matrix. The
// lease is GPU state, so unlike the pure-module checks (packages/3d/checks)
// this cannot run on the headless flux binary - flux:gpu lives behind the
// gui feature. It runs on the playback client instead, from the repo root:
//
//   bunx srt render packages/core/checks/gpu-lease-check.tsx --duration 1 --size 128x128
//
// The app asserts across three frames and prints one PASS/FAIL summary, then
// exits; read the output, not the exit code. Deterministic (no PRNG): three
// quads at fixed positions, colors chosen so a wrong pixel names the wrong
// frame.
import { exit, onFrame, render } from "@solidrt/core"
import {
  beginBufferWrite,
  createBuffer,
  createPipelineTexture,
  destroyBuffer,
  endBufferWrite,
  glsl,
  readTexture,
  setDraw,
} from "@solidrt/core/gpu"

const SIZE = 64
// floats per record: center vec2, half-size f32, tint vec3.
const RECORD = 6
const CAPACITY = 4

let failures = 0
function fail(msg: string) {
  failures++
  console.log(`FAIL: ${msg}`)
}

function assertThrows(what: string, fn: () => void) {
  try {
    fn()
    fail(`${what}: expected a throw`)
  } catch {
    // expected
  }
}

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

/** Sample the target at clip-space (cx, cy) and compare to (r, g, b) 0..255. */
function expectPixel(id: number, cx: number, cy: number, rgb: [number, number, number], what: string) {
  let { width, height, data } = readTexture(id as never)
  // Pipeline clip space is y-DOWN (gl_Position y = -1 is the top row - the
  // pixel contract in core gpu.ts), and rows read back top-to-bottom.
  let px = Math.round((cx * 0.5 + 0.5) * (width - 1))
  let py = Math.round((cy * 0.5 + 0.5) * (height - 1))
  let at = (py * width + px) * 4
  let got = [data[at]!, data[at + 1]!, data[at + 2]!]
  let ok = got.every((v, i) => Math.abs(v - rgb[i]!) <= 2)
  if (!ok) fail(`${what}: pixel at clip (${cx}, ${cy}) is [${got}], expected [${rgb}]`)
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

function App() {
  let quad = createBuffer(new Float32Array([-0.5, -0.5, 0.5, -0.5, -0.5, 0.5, 0.5, 0.5]), { label: "check-quad" })
  // The number overload: zeroed storage, filled through the lease.
  let records = createBuffer(CAPACITY * RECORD * 4, { label: "check-records" })
  let target = createPipelineTexture(VERTEX, FRAGMENT, SIZE, SIZE, null, {
    label: "lease-check",
    topology: "triangle-strip",
    vertexCount: 4,
    attributes: [{ name: "aPos", format: "vec2" }],
    buffer: quad,
    instanceAttributes: [
      { name: "iCenter", format: "vec2" },
      { name: "iSize", format: "f32" },
      { name: "iTint", format: "vec3" },
    ],
    instanceBuffer: records,
    instanceCount: 0,
    clearColor: [0, 0, 0, 1],
  })

  let frame = 0
  onFrame(() => {
    frame++
    if (frame === 1) {
      // Throw matrix first, on a fresh id.
      let out = beginBufferWrite(records)
      if (out.length !== CAPACITY * RECORD) fail(`lease length ${out.length}, expected ${CAPACITY * RECORD}`)
      assertThrows("double begin", () => beginBufferWrite(records))
      // Oversize publish closes the lease and throws...
      assertThrows("oversize publish", () => endBufferWrite(records, CAPACITY * RECORD * 4 + 1))
      // ...so the view is detached and a fresh begin works.
      if (out.buffer.byteLength !== 0) fail("view not detached after oversize publish")
      assertThrows("end without begin", () => endBufferWrite(records))

      // Happy path: two quads, full publish.
      let out2 = beginBufferWrite(records)
      writeRecord(out2, 0, -0.5, 0, 0.4, [1, 0, 0])
      writeRecord(out2, 1, 0.5, 0, 0.4, [0, 1, 0])
      endBufferWrite(records)
      setDraw(target, { instanceCount: 2 })
      if (out2.buffer.byteLength !== 0) fail("view not detached after publish")
      let stale = out2[0]
      if (stale !== undefined) fail("detached view still reads values")
    }
    if (frame === 2) {
      expectPixel(target, -0.5, 0, [255, 0, 0], "frame 1 left quad")
      expectPixel(target, 0.5, 0, [0, 255, 0], "frame 1 right quad")
      expectPixel(target, 0, 0.75, [0, 0, 0], "frame 1 background")

      // Recycle: begin again (reuses a pooled block - contents unspecified,
      // rewrite everything), publish a ONE-record prefix, shrink the draw.
      let out = beginBufferWrite(records)
      writeRecord(out, 0, 0, 0.5, 0.3, [0, 0, 1])
      endBufferWrite(records, RECORD * 4)
      setDraw(target, { instanceCount: 1 })
    }
    if (frame === 3) {
      expectPixel(target, 0, 0.5, [0, 0, 255], "frame 2 top quad")
      expectPixel(target, -0.5, 0, [0, 0, 0], "frame 2: frame 1 quads gone")

      // Cancel: publish nothing; pixels must be unchanged next frame.
      beginBufferWrite(records)
      endBufferWrite(records, 0)
    }
    if (frame === 4) {
      expectPixel(target, 0, 0.5, [0, 0, 255], "cancel left pixels unchanged")

      // Destroy mid-lease: the view detaches, later lease calls throw.
      let out = beginBufferWrite(records)
      destroyBuffer(records)
      if (out.buffer.byteLength !== 0) fail("view not detached by destroy")
      assertThrows("begin after destroy", () => beginBufferWrite(records))
      assertThrows("end after destroy", () => endBufferWrite(records))

      if (failures === 0) console.log("PASS: lease happy path, recycle, prefix, cancel, throw matrix")
      else console.log(`${failures} FAILURES`)
      exit()
    }
  })

  return (
    <window alignItems="center" justifyContent="center">
      <texture src={target} width={SIZE} height={SIZE} />
    </window>
  )
}

render(() => <App />)
