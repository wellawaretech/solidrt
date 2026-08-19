// The zero-copy per-frame geometry path: instanced quads whose records are
// rewritten every frame through a buffer write lease. beginBufferWrite hands
// back a Float32Array over runtime-owned memory (contents unspecified - fill
// everything you publish), the frame callback writes every live record into
// it, and endBufferWrite publishes by MOVING the block to the raster thread:
// no data copy anywhere on the CPU path, and no per-sprite property writes.
// Compare gpu-instancing.tsx, where the records are static and only the draw
// range changes; here the records themselves are the animation.
//
// The buffer is created from a byte LENGTH (zeroed storage) - the natural
// create when every byte arrives through the lease. Publishing a prefix and
// setDraw({ instanceCount }) keep buffer capacity and live population
// independent: reserve the max up front, publish what exists this frame.
import { render, onFrame } from "@solidrt/core"
import { beginBufferWrite, createBuffer, createPipelineTexture, endBufferWrite, glsl, setDraw } from "@solidrt/core/gpu"

const MAX_SPRITES = 2000
// floats per record: center vec2, half-size f32, tint vec3.
const RECORD = 6

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

// Simulation state lives in plain arrays; the instance buffer holds only
// this frame's published snapshot of it.
let x = new Float32Array(MAX_SPRITES)
let y = new Float32Array(MAX_SPRITES)
let vx = new Float32Array(MAX_SPRITES)
let vy = new Float32Array(MAX_SPRITES)
for (let i = 0; i < MAX_SPRITES; i++) {
  x[i] = Math.random() * 1.9 - 0.95
  y[i] = Math.random() * 1.9 - 0.95
  vx[i] = (Math.random() * 2 - 1) * 0.01
  vy[i] = (Math.random() * 2 - 1) * 0.01
}

function App() {
  // One unit quad (triangle strip), reused by every instance.
  let quad = createBuffer(new Float32Array([-0.5, -0.5, 0.5, -0.5, -0.5, 0.5, 0.5, 0.5]), { label: "sprite-quad" })
  let records = createBuffer(MAX_SPRITES * RECORD * 4, { label: "sprite-records" })
  let id = createPipelineTexture(VERTEX, FRAGMENT, 720, 720, null, {
    label: "sprites",
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
    instanceCount: MAX_SPRITES,
    clearColor: [0.03, 0.03, 0.06, 1],
  })

  onFrame(() => {
    let out = beginBufferWrite(records)
    for (let i = 0; i < MAX_SPRITES; i++) {
      let nx = x[i]! + vx[i]!
      let ny = y[i]! + vy[i]!
      if (nx < -0.95 || nx > 0.95) vx[i] = -vx[i]!
      else x[i] = nx
      if (ny < -0.95 || ny > 0.95) vy[i] = -vy[i]!
      else y[i] = ny
      let at = i * RECORD
      out[at] = x[i]!
      out[at + 1] = y[i]!
      out[at + 2] = 0.01 + 0.008 * (i % 5)
      out[at + 3] = 0.5 + 0.5 * Math.cos(i * 0.11)
      out[at + 4] = 0.5 + 0.5 * Math.cos(i * 0.13 + 2.1)
      out[at + 5] = 0.5 + 0.5 * Math.cos(i * 0.17 + 4.2)
    }
    endBufferWrite(records)
  })

  return (
    <window alignItems="center" justifyContent="center">
      <texture src={id} width={480} height={480} />
    </window>
  )
}

render(() => <App />)
