// createPipelineTexture compiles a custom GLSL ES 3.00 vertex+fragment pair
// that draws an interleaved vertex buffer into a texture, here a spinning
// cube with depth testing. The vertex shader declares `in` attributes
// matching the pipeline's attribute list (locations are resolved by name)
// and its own varyings; the fragment preamble provides fragColor/iResolution
// but no vUV, and app-driven uniforms (uTime below) are the source's own
// declarations. Uniforms are driven exactly like createShaderTexture:
// declaratively via the <texture> params prop, applied at the next repaint.
// The cube draws indexed (indexBuffer + indexFormat: 24 shared vertices
// stitched by 36 uint16 indices instead of 36 unshared vertices) with its
// back faces culled (cull: "back") - a closed mesh never shows them, so
// rastering them is pure waste.
import { render, onFrame, createSignal, pct } from "@solidrt/core"
import { createBuffer, createPipelineTexture, glsl } from "@solidrt/core/gpu"

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
    p.z -= 2.5;

    // Standard right-handed camera at the origin looking down -z, perspective
    // near 1 far 10. Clip y is negated: the target's memory row 0 is clip
    // y = -1, and Impeller samples row 0 as the top, so camera-up needs the
    // flip to be displayed up. With this textbook rig the CCW-outward mesh
    // winds counter-clockwise AS DISPLAYED on its camera-facing faces -
    // exactly the front-face rule cull: "back" tests against.
    float w = -p.z;
    float f = 2.0;
    float a = 11.0 / 9.0;
    float b = -20.0 / 9.0;
    gl_Position = vec4(p.x * f, -p.y * f, w * a + b, w);
    vColor = aColor;
  }
`

let FRAGMENT = glsl`
  in vec3 vColor;
  void main() {
    fragColor = vec4(vColor, 1.0);
  }
`

// Interleaved [pos vec3, color vec3]: 24 vertices, 4 per face with that
// face's color - each corner stored once and stitched into 2 triangles by
// the index buffer below, the sharing real meshes are made of. Every face
// winds counter-clockwise seen from outside, so with the projection's y
// negation the cube culls correctly with cull: "back".
function cubeVertices(): Float32Array {
  type Vec3 = [number, number, number]
  let verts: number[] = []
  let quad = (a: Vec3, b: Vec3, c: Vec3, d: Vec3, color: Vec3) => {
    for (let p of [a, b, c, d]) verts.push(p[0], p[1], p[2], color[0], color[1], color[2])
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

// Two triangles per face over its 4 shared vertices: 36 uint16 indices.
function cubeIndices(): Uint16Array {
  let indices: number[] = []
  for (let face = 0; face < 6; face++) {
    let v = face * 4
    indices.push(v, v + 1, v + 2, v, v + 2, v + 3)
  }
  return new Uint16Array(indices)
}

function App() {
  // Labels name the buffers and target in the dev tooling's GPU inventory
  // (and in engine log messages) - free-form, purely diagnostic.
  let bufferId = createBuffer(cubeVertices(), { label: "cube-verts" })
  let indexId = createBuffer(cubeIndices(), { label: "cube-indices" })
  let id = createPipelineTexture(VERTEX, FRAGMENT, 1024, 1024, { uTime: 0 }, {
    label: "cube",
    attributes: [
      { name: "aPos", format: "vec3" },
      { name: "aColor", format: "vec3" },
    ],
    buffer: bufferId,
    indexBuffer: indexId,
    indexFormat: "uint16",
    depth: true,
    cull: "back",
    clearColor: [0.08, 0.08, 0.12, 1],
  })
  let [time, setTime] = createSignal(0)
  onFrame((tick) => setTime(tick / 1000))

  // Fill the window: the design-size fits and centers the square content into
  // the full-window view, so the projection is never stretched.
  return (
    <window>
      <view width={pct(100)} height={pct(100)} designSize={[1024, 1024]}>
        <texture src={id} params={{ uTime: time() }} width={1024} height={1024} />
      </view>
    </window>
  )
}

render(() => <App />)
