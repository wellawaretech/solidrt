// An additive particle field: createPipeline with topology "points" and
// blend "add". Each vertex is one particle; the vertex stage sets
// gl_PointSize (honored across 4..64px) and the fragment stage shapes the
// splat from gl_PointCoord. With blend: "add" overlapping splats accumulate
// (glBlendFunc(ONE, ONE)) - order-independent, so the buffer needs no
// sorting - which is what turns discrete discs into a smooth glowing field.
// Without it a target's draw overwrites, and a point cloud can only thicken
// into scaly overlap.
//
// Additive output is premultiplied by construction: write vec4(color * a, a)
// and the target stays composite-correct in the tree. No depth buffer here -
// nothing occludes anything in a pure additive pass. A scene where opaque
// geometry should occlude the particles would add depth: true and pair the
// blended draw with depthWrite: false, explicitly - neither option implies
// the other.
//
// The tints are typed (vec3) uniforms driven from 3-number array params.
import { render, onFrame, createSignal } from "@solidrt/core"
import { createBuffer, createPipeline } from "@solidrt/core/gpu"

let VERTEX = `
in vec3 aPos;
in float aSeed;
out float vSeed;
uniform float uTime;

void main() {
  float cy = cos(uTime * 0.4), sy = sin(uTime * 0.4);
  vec3 p = vec3(cy * aPos.x - sy * aPos.z, aPos.y, sy * aPos.x + cy * aPos.z);
  // Each particle breathes on its own phase.
  p *= 1.0 + 0.15 * sin(uTime * 1.7 + aSeed * 40.0);
  p.z += 2.2;

  // Same perspective mapping as gpu-pipeline.tsx (near 1, far 10), clip y
  // negated so camera-up displays up.
  float f = 2.0;
  gl_Position = vec4(p.x * f, -p.y * f, p.z * (11.0 / 9.0) - 20.0 / 9.0, p.z);
  gl_PointSize = mix(10.0, 26.0, aSeed) / p.z;
  vSeed = aSeed;
}
`

let FRAGMENT = `
in float vSeed;
uniform vec3 uTintA;
uniform vec3 uTintB;

void main() {
  // Soft gaussian falloff over the point sprite; gl_PointCoord is 0..1
  // across the splat.
  vec2 d = gl_PointCoord - 0.5;
  float a = exp(-dot(d, d) * 14.0) * 0.35;
  vec3 tint = mix(uTintA, uTintB, vSeed);
  fragColor = vec4(tint * a, a);
}
`

// Interleaved [pos vec3, seed f32]: points on a fibonacci sphere, so the
// field reads as a volume from every angle.
function particles(count: number): Float32Array {
  let verts: number[] = []
  let golden = Math.PI * (3.0 - Math.sqrt(5.0))
  for (let i = 0; i < count; i++) {
    let y = 1.0 - (2.0 * (i + 0.5)) / count
    let r = Math.sqrt(1.0 - y * y)
    let t = golden * i
    let seed = (i * 0.61803399) % 1.0
    verts.push(0.7 * r * Math.cos(t), 0.7 * y, 0.7 * r * Math.sin(t), seed)
  }
  return new Float32Array(verts)
}

function App() {
  let bufferId = createBuffer(particles(1500))
  let id = createPipeline(VERTEX, FRAGMENT, 512, 512, {
    params: { uTime: 0, uTintA: [1.0, 0.45, 0.15], uTintB: [0.25, 0.5, 1.0] },
    attributes: [
      { name: "aPos", format: "vec3" },
      { name: "aSeed", format: "f32" },
    ],
    buffer: bufferId,
    topology: "points",
    blend: "add",
    clearColor: [0.02, 0.02, 0.05, 1],
  })
  let [time, setTime] = createSignal(0)
  onFrame((tick) => setTime(tick / 1000))

  return (
    <window alignItems="center" justifyContent="center">
      <texture src={id} params={{ uTime: time() }} width={420} height={420} />
    </window>
  )
}

render(() => <App />)
