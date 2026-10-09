// A chain of targets: a plasma fragment pass feeding a cube pipeline that
// samples it. Sampler bindings are live dependencies (the model in
// @solidrt/core/docs/reference/gpu.md): the cube's target is bound to the
// plasma's through `textures`, so whenever the plasma re-renders, the cube
// re-renders after it in the same frame, in that order. The app therefore
// drives only the first pass: the plasma's uTime through the <texture>
// params prop, as in gpu-shader.tsx. The cube has no uniform of its own
// and no per-frame code; its view is fixed in the vertex shader, and its
// faces move because what they sample moves.
//
// The plasma is shown beside the cube because that <texture> is where its
// params prop lives. A source pass that is not on screen is driven the same
// way from any <texture> of its id. The cube's buffers and index buffer are
// the gpu-pipeline.tsx mesh with a UV per corner in place of a color.
import { render, onFrame, createSignal } from "@solidrt/core"
import { createBuffer, createPipelineTexture, createShaderTexture, glsl } from "@solidrt/core/gpu"

// The plasma target's side: what the cube's faces sample.
let PLASMA_SIZE = 256
// The cube target's side: what the window shows.
let CUBE_SIZE = 512

// Pass 1: a plasma, the only thing the app animates.
let PLASMA = glsl`
  uniform float uTime;
  void main() {
    vec2 uv = vUV * 6.0;
    float t = uTime;
    float v = sin(uv.x + t) + sin(uv.y + t * 0.8) + sin((uv.x + uv.y) * 0.7 + t * 1.3)
      + sin(length(uv - 3.0) * 1.5 - t);
    vec3 col = 0.5 + 0.5 * cos(v * 1.2 + vec3(0.0, 2.1, 4.2));
    fragColor = vec4(col, 1.0);
  }
`

// Pass 2: the cube, viewed from one fixed angle. The projection rig and the
// clip-y flip are gpu-pipeline.tsx's.
let VERTEX = glsl`
  in vec3 aPos;
  in vec2 aUV;
  out vec2 vTex;

  void main() {
    float cy = cos(0.7), sy = sin(0.7);
    float cx = cos(0.5), sx = sin(0.5);
    mat3 rotY = mat3(cy, 0.0, -sy, 0.0, 1.0, 0.0, sy, 0.0, cy);
    mat3 rotX = mat3(1.0, 0.0, 0.0, 0.0, cx, sx, 0.0, -sx, cx);
    vec3 p = rotX * (rotY * aPos);
    p.z -= 2.5;
    float w = -p.z;
    float f = 2.0;
    float a = 11.0 / 9.0;
    float b = -20.0 / 9.0;
    gl_Position = vec4(p.x * f, -p.y * f, w * a + b, w);
    vTex = aUV;
  }
`

// The sampler is the chain: uMap is bound to the plasma target below.
let FRAGMENT = glsl`
  in vec2 vTex;
  uniform sampler2D uMap;
  void main() {
    fragColor = texture(uMap, vTex);
  }
`

// Interleaved [pos vec3, uv vec2]: 24 vertices, 4 per face, each face the
// whole plasma once. Every face winds counter-clockwise seen from outside.
function cubeVertices(): Float32Array {
  type Vec3 = [number, number, number]
  let verts: number[] = []
  let quad = (a: Vec3, b: Vec3, c: Vec3, d: Vec3) => {
    let uvs = [[0, 1], [1, 1], [1, 0], [0, 0]]
    for (let [i, p] of [a, b, c, d].entries()) verts.push(p[0], p[1], p[2], uvs[i]![0]!, uvs[i]![1]!)
  }
  let s = 0.5
  quad([-s, -s, s], [s, -s, s], [s, s, s], [-s, s, s]) // front
  quad([s, -s, -s], [-s, -s, -s], [-s, s, -s], [s, s, -s]) // back
  quad([s, -s, s], [s, -s, -s], [s, s, -s], [s, s, s]) // right
  quad([-s, -s, -s], [-s, -s, s], [-s, s, s], [-s, s, -s]) // left
  quad([-s, s, s], [s, s, s], [s, s, -s], [-s, s, -s]) // top
  quad([-s, -s, -s], [s, -s, -s], [s, -s, s], [-s, -s, s]) // bottom
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
  let plasmaId = createShaderTexture(PLASMA, PLASMA_SIZE, PLASMA_SIZE, { uTime: 0 }, { label: "plasma" })
  let bufferId = createBuffer(cubeVertices(), { label: "cube-verts" })
  let indexId = createBuffer(cubeIndices(), { label: "cube-indices" })
  // No params: nothing of the cube's is ever written. Its one input is the
  // plasma target, and that binding is what re-renders it.
  let cubeId = createPipelineTexture(VERTEX, FRAGMENT, CUBE_SIZE, CUBE_SIZE, null, {
    label: "cube",
    textures: { uMap: plasmaId },
    buffers: [
      {
        attributes: [
          { name: "aPos", format: "float32x3" },
          { name: "aUV", format: "float32x2" },
        ],
        buffer: bufferId,
      },
    ],
    indexBuffer: indexId,
    indexFormat: "uint16",
    depth: true,
    cull: "back",
    clearColor: [0.08, 0.08, 0.12, 1],
  })
  let [time, setTime] = createSignal(0)
  onFrame((tick) => setTime(tick / 1000))

  return (
    <window alignItems="center" justifyContent="center" flexDirection="row" gap={24}>
      <texture src={cubeId} width={CUBE_SIZE} height={CUBE_SIZE} />
      <view flexDirection="column" alignItems="center" gap={8}>
        <texture src={plasmaId} params={{ uTime: time() }} width={PLASMA_SIZE} height={PLASMA_SIZE} />
        <text fontSize={14} color="#888">the plasma pass, the only one driven</text>
      </view>
    </window>
  )
}

render(() => <App />)
