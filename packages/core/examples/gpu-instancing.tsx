// Instanced drawing: one 3-vertex triangle in the buffer, drawn hundreds of
// times with instanceCount (glDrawArraysInstanced, native ES 3.0). The vertex
// stage tells copies apart by gl_InstanceID - here each instance becomes one
// petal on a phyllotaxis spiral, placed and tinted from its index alone, so
// the geometry buffer stays 6 floats no matter the population.
//
// The draw range is data: setDraw merges partial updates like params, so the
// per-frame write below changes only instanceCount while firstVertex and
// vertexCount keep their values (the whole buffer). instanceCount 0 would
// draw nothing - the population legitimately breathes down to a single
// petal and could go dark the same way. gl_InstanceID always counts from 0
// (ES 3.0 has no base instance).
import { render, onFrame, createSignal } from "@solidrt/core"
import { createBuffer, createPipeline, glsl, setDraw } from "@solidrt/core/gpu"

let MAX_PETALS = 324

let VERTEX = glsl`
  in vec2 aPos;
  out vec3 vTint;
  uniform float uTime;

  void main() {
    // One instance = one petal: radius grows with sqrt(index) and the angle
    // steps by the golden angle (~2.39996 rad) - the sunflower layout.
    float i = float(gl_InstanceID);
    float angle = i * 2.39996 + uTime * 0.3;
    float radius = 0.045 * sqrt(i);
    // Petals shrink toward the rim and point outward along the spiral.
    float scale = mix(0.05, 0.016, sqrt(i) / 18.0);
    float c = cos(angle), s = sin(angle);
    vec2 p = radius * vec2(c, s) + mat2(c, s, -s, c) * (aPos * scale);
    gl_Position = vec4(p, 0.0, 1.0);
    vTint = 0.5 + 0.5 * cos(6.2832 * (i / 96.0) + vec3(0.0, 2.1, 4.2));
  }
`

let FRAGMENT = glsl`
  in vec3 vTint;

  void main() {
    fragColor = vec4(vTint, 1.0);
  }
`

function App() {
  // The whole mesh: one triangle, reused by every instance.
  let bufferId = createBuffer(new Float32Array([0, 1.3, -1, -0.75, 1, -0.75]), { label: "petal-tri" })
  let id = createPipeline(VERTEX, FRAGMENT, 512, 512, {
    label: "petals",
    params: { uTime: 0 },
    attributes: [{ name: "aPos", format: "vec2" }],
    buffer: bufferId,
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
