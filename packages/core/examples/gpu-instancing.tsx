// Instanced drawing with per-instance attributes: one 3-vertex triangle in
// the vertex buffer, drawn hundreds of times, each copy reading its own
// record from an instance buffer (instanceAttributes on the pipeline +
// instanceBuffer on the target, vertex divisor 1 underneath). Here each
// record is one petal of a phyllotaxis spiral - its angle, radius, size, and
// tint, computed once in JS - so the vertex stage just reads state instead
// of re-deriving it from gl_InstanceID every frame, and the geometry buffer
// stays 6 floats no matter the population.
//
// The draw range is data: setDraw merges partial updates like params, so the
// per-frame write below changes only instanceCount while firstVertex and
// vertexCount keep their values (the whole buffer). With an instance buffer
// bound, an omitted instanceCount would default to one instance per record
// (the whole population); the explicit 1 here starts the bloom closed, and
// the count is bounds-checked against the records either way.
import { render, onFrame, createSignal } from "@solidrt/core"
import { createBuffer, createPipelineTexture, glsl, setDraw } from "@solidrt/core/gpu"

let MAX_PETALS = 324

let VERTEX = glsl`
  in vec2 aPos;
  in float iAngle;
  in float iRadius;
  in float iScale;
  in vec3 iTint;
  out vec3 vTint;
  uniform float uTime;

  void main() {
    // The record places the petal; time only spins the whole flower.
    float angle = iAngle + uTime * 0.3;
    float c = cos(angle), s = sin(angle);
    vec2 p = iRadius * vec2(c, s) + mat2(c, s, -s, c) * (aPos * iScale);
    gl_Position = vec4(p, 0.0, 1.0);
    vTint = iTint;
  }
`

let FRAGMENT = glsl`
  in vec3 vTint;

  void main() {
    fragColor = vec4(vTint, 1.0);
  }
`

// One record per petal (matching instanceAttributes: 6 floats): radius grows
// with sqrt(index), the angle steps by the golden angle (~2.39996 rad) - the
// sunflower layout - petals shrink toward the rim, and the tint walks a
// cosine palette.
function petalRecords() {
  let records = new Float32Array(MAX_PETALS * 6)
  for (let i = 0; i < MAX_PETALS; i++) {
    let at = i * 6
    let t = Math.sqrt(i) / 18
    records[at] = i * 2.39996
    records[at + 1] = 0.045 * Math.sqrt(i)
    records[at + 2] = 0.05 * (1 - t) + 0.016 * t
    for (let k = 0; k < 3; k++) {
      records[at + 3 + k] = 0.5 + 0.5 * Math.cos(6.2832 * (i / 96) + k * 2.1)
    }
  }
  return records
}

function App() {
  // The whole mesh: one triangle, reused by every instance.
  let bufferId = createBuffer(new Float32Array([0, 1.3, -1, -0.75, 1, -0.75]), { label: "petal-tri" })
  let instanceId = createBuffer(petalRecords(), { label: "petal-records" })
  let id = createPipelineTexture(VERTEX, FRAGMENT, 512, 512, { uTime: 0 }, {
    label: "petals",
    attributes: [{ name: "aPos", format: "vec2" }],
    buffer: bufferId,
    instanceAttributes: [
      { name: "iAngle", format: "f32" },
      { name: "iRadius", format: "f32" },
      { name: "iScale", format: "f32" },
      { name: "iTint", format: "vec3" },
    ],
    instanceBuffer: instanceId,
    instanceCount: 1,
    clearColor: [0.03, 0.03, 0.06, 1],
  })
  let [time, setTime] = createSignal(0)
  onFrame((tick) => {
    let t = tick / 1000
    setTime(t)
    setDraw(id, { instanceCount: Math.round(MAX_PETALS / 2 + (MAX_PETALS / 2 - 1) * Math.sin(t * 0.7)) })
  })

  return (
    <window alignItems="center" justifyContent="center">
      <texture src={id} params={{ uTime: time() }} width={420} height={420} />
    </window>
  )
}

render(() => <App />)
