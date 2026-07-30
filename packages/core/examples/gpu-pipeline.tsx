// createPipeline compiles a custom GLSL ES 3.00 vertex+fragment pair that draws
// an interleaved vertex buffer into a texture, here a spinning cube with depth
// testing. The vertex shader declares `in` attributes matching the pipeline's
// attribute list (locations are resolved by name) and its own varyings; the
// fragment preamble provides fragColor/iResolution/iTime but no vUV. Uniforms
// are driven exactly like createShader: declaratively via the <texture> params
// prop, applied at the next repaint.
import { render, onFrame, createSignal } from "@solidrt/core"
import { createBuffer, createPipeline, glsl } from "@solidrt/core/gpu"

let VERTEX = glsl`
  in vec3 aPos;
  in vec3 aColor;
  out vec3 vColor;
  uniform float uTime;

  void main() {
    float cy = cos(uTime), sy = sin(uTime);
    float cx = cos(uTime * 0.7), sx = sin(uTime * 0.7);
    mat3 rotY = mat3(cy, 0.0, -sy, 0.0, 1.0, 0.0, sy, 0.0, cy);
    mat3 rotX = mat3(1.0, 0.0, 0.0, 0.0, cx, sx, 0.0, -sx, cx);
    vec3 p = rotX * (rotY * aPos);
    p.z += 2.5;

    // Perspective projection (near 1, far 10). Clip y is negated: the target's
    // memory row 0 is clip y = -1, and Impeller samples row 0 as the top, so
    // camera-up needs the flip to be displayed up.
    float f = 2.0;
    float a = 11.0 / 9.0;
    float b = -20.0 / 9.0;
    gl_Position = vec4(p.x * f, -p.y * f, p.z * a + b, p.z);
    vColor = aColor;
  }
`

let FRAGMENT = glsl`
  in vec3 vColor;
  void main() {
    fragColor = vec4(vColor, 1.0);
  }
`

// Interleaved [pos vec3, color vec3], 6 faces x 2 triangles, one color per face.
function cube(): Float32Array {
  type Vec3 = [number, number, number]
  let verts: number[] = []
  let quad = (a: Vec3, b: Vec3, c: Vec3, d: Vec3, color: Vec3) => {
    for (let p of [a, b, c, a, c, d]) verts.push(p[0], p[1], p[2], color[0], color[1], color[2])
  }
  let s = 0.5
  quad([-s, -s, s], [s, -s, s], [s, s, s], [-s, s, s], [0.9, 0.3, 0.3]) // front
  quad([s, -s, -s], [-s, -s, -s], [-s, s, -s], [s, s, -s], [0.3, 0.9, 0.4]) // back
  quad([s, -s, s], [s, -s, -s], [s, s, -s], [s, s, s], [0.3, 0.5, 0.9]) // right
  quad([-s, -s, -s], [-s, -s, s], [-s, s, s], [-s, s, -s], [0.9, 0.8, 0.3]) // left
  quad([-s, s, s], [s, s, s], [s, s, -s], [-s, s, -s], [0.8, 0.4, 0.9]) // top
  quad([-s, -s, -s], [s, -s, -s], [s, -s, s], [-s, -s, s], [0.4, 0.9, 0.9]) // bottom
  return new Float32Array(verts)
}

function App() {
  let bufferId = createBuffer(cube())
  let id = createPipeline(VERTEX, FRAGMENT, 512, 512, {
    params: { uTime: 0 },
    attributes: [
      { name: "aPos", format: "vec3" },
      { name: "aColor", format: "vec3" },
    ],
    buffer: bufferId,
    depth: true,
    clearColor: [0.08, 0.08, 0.12, 1],
  })
  let [time, setTime] = createSignal(0)
  onFrame((tick) => setTime(tick / 1000))

  return (
    <window alignItems="center" justifyContent="center">
      <texture src={id} params={{ uTime: time() }} width={400} height={400} />
    </window>
  )
}

render(() => <App />)
