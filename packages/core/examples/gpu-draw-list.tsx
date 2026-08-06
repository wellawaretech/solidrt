// A draw target: one render target holding an ordered, mutable LIST of
// draws - two orbiting triangles from two different programs sharing one
// depth buffer (they occlude each other correctly as they cross), plus a
// third entry added and removed on a timer. One render of the target is one
// GPU pass no matter how many entries it holds, and per-entry params
// (setDrawParams) are the per-object channel: each triangle carries its own
// angle uniform, the model-matrix pattern at toy scale.
//
// The depth split to notice: the TARGET owns the storage (depth: true on
// createDrawTarget - what makes cross-entry occlusion work), each PIPELINE
// owns the behavior (depth: true on createRenderPipeline - this draw tests
// and writes). A depth-testing pipeline added to a depthless draw target
// throws at addDraw.
import { render, onFrame } from "@solidrt/core"
import {
  addDraw,
  compileShader,
  createBuffer,
  createDrawTarget,
  createRenderPipeline,
  glsl,
  linkProgram,
  removeDraw,
  setDrawParams,
} from "@solidrt/core/gpu"
import type { DrawId } from "@solidrt/core/gpu"

let VERTEX = glsl`
  in vec2 aPos;
  uniform float uAngle;

  void main() {
    // Orbit in x, swing through depth in z: the two entries cross each
    // other, and the shared depth buffer decides who is in front.
    float c = cos(uAngle), s = sin(uAngle);
    vec2 p = aPos * 0.55 + vec2(0.55 * c, 0.0);
    gl_Position = vec4(p, 0.5 * s, 1.0);
  }
`

let FRAGMENT_WARM = glsl`
  void main() {
    fragColor = vec4(0.95, 0.45, 0.2, 1.0);
  }
`

let FRAGMENT_COOL = glsl`
  void main() {
    fragColor = vec4(0.25, 0.55, 0.95, 1.0);
  }
`

let FRAGMENT_PULSE = glsl`
  uniform float uPhase;
  void main() {
    fragColor = vec4(vec3(0.6 + 0.4 * sin(uPhase)), 1.0);
  }
`

function App() {
  let triangle = createBuffer(new Float32Array([0, 0.6, -0.5, -0.4, 0.5, -0.4]), { label: "tri" })
  let vs = compileShader("vertex", VERTEX, { header: true })
  let attrs = [{ name: "aPos", format: "vec2" as const }]
  let warm = createRenderPipeline(linkProgram(vs, compileShader("fragment", FRAGMENT_WARM, { header: true })), {
    attributes: attrs,
    depth: true,
  })
  let cool = createRenderPipeline(linkProgram(vs, compileShader("fragment", FRAGMENT_COOL, { header: true })), {
    attributes: attrs,
    depth: true,
  })
  let pulse = createRenderPipeline(linkProgram(vs, compileShader("fragment", FRAGMENT_PULSE, { header: true })), {
    attributes: attrs,
  })

  let target = createDrawTarget(512, 512, null, { depth: true, clearColor: [0.04, 0.04, 0.08, 1], label: "orbits" })
  let warmDraw = addDraw(target, warm, { uAngle: 0 }, { buffer: triangle })
  let coolDraw = addDraw(target, cool, { uAngle: Math.PI }, { buffer: triangle })

  // A third entry blinks in and out every second: add/remove are ordinary
  // per-frame-affordable writes, and a removed DrawId simply retires. Its
  // pipeline declares no depth, so it neither tests nor writes: list order
  // alone places it, and inserting it BEFORE the warm triangle
  // (before: warmDraw) keeps the flash beneath both orbits - appended
  // instead, it would cover them. setDrawOrder is the wholesale form when a
  // scene sorts its whole list.
  let pulseDraw: DrawId | null = null
  onFrame((tick) => {
    let t = tick / 1000
    setDrawParams(target, warmDraw, { uAngle: t })
    setDrawParams(target, coolDraw, { uAngle: t + Math.PI })
    let wantPulse = Math.floor(t) % 2 === 0
    if (wantPulse && pulseDraw === null) {
      pulseDraw = addDraw(target, pulse, { uAngle: t * 0.3, uPhase: t * 5 }, { buffer: triangle, before: warmDraw })
    } else if (!wantPulse && pulseDraw !== null) {
      removeDraw(target, pulseDraw)
      pulseDraw = null
    } else if (pulseDraw !== null) {
      setDrawParams(target, pulseDraw, { uAngle: t * 0.3, uPhase: t * 5 })
    }
  })

  return (
    <window alignItems="center" justifyContent="center">
      <texture src={target} width={420} height={420} />
    </window>
  )
}

render(() => <App />)
