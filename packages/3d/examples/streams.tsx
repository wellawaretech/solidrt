// Vertex streams and the draw range: a sphere whose surface heaves every
// frame without its positions, normals or uvs ever re-uploading. The
// displacement rides in a stream of its own - `withAttribute(..., {
// stream: 1 })` opens a second vertex buffer holding one float per vertex
// - and each frame writes that stream through the attribute accessor and
// re-uploads it alone with updateVertices, Three's `needsUpdate` for one
// BufferAttribute. The static channels stay in the buffer they were
// uploaded to once; the pipeline reads both buffers.
//
// The wireframe over the same geometry shares every stream (and every GPU
// upload) with the surface, so the one update moves both meshes.
//
// setDrawRange reveals the sphere: the mesh draws a growing slice of its
// index list over the first seconds, then the whole of it - the same
// buffers, only the range moving.
import { onFrame, pct, render } from "@solidrt/core"
import { glsl } from "@solidrt/core/gpu"
import {
  DirectionalLight,
  geometryAttribute,
  geometryVertexCount,
  HemisphereLight,
  Mesh,
  PerspectiveCamera,
  Scene,
  setDrawRange,
  shaderMaterialClass,
  sphere,
  updateVertices,
  wireframeGeometry,
  withAttribute,
} from "@solidrt/3d"
import type { MeshNode } from "@solidrt/3d"
import { litFragment } from "@solidrt/3d/glsl"

// The lit varyings from a position pushed along its normal by the
// per-vertex aWave, which the pipeline fetches from the second stream.
const WAVE_VERTEX = glsl`
  in vec3 aPos;
  in vec3 aNormal;
  in vec2 aUV;
  in float aWave;
  out vec3 vWorldPos;
  out vec3 vNormal;
  out vec2 vUv;
  uniform mat4 uModel;
  uniform mat4 uViewProj;
  uniform mat4 uNormal;

  void main() {
    vec4 world = uModel * vec4(aPos + aNormal * aWave, 1.0);
    gl_Position = uViewProj * world;
    vWorldPos = world.xyz;
    vNormal = mat3(uNormal) * aNormal;
    vUv = aUV;
  }
`

// The same displacement for the wireframe, drawn flat.
const WAVE_LINE_VERTEX = glsl`
  in vec3 aPos;
  in vec3 aNormal;
  in float aWave;
  uniform mat4 uModel;
  uniform mat4 uViewProj;

  void main() {
    gl_Position = uViewProj * uModel * vec4(aPos + aNormal * (aWave + 0.004), 1.0);
  }
`

const LINE_FRAGMENT = glsl`
  void main() {
    fragColor = vec4(0.12, 0.16, 0.2, 1.0);
  }
`

// Displacement amplitude in model units, and the seconds the reveal takes.
const WAVE_HEIGHT = 0.06
const REVEAL_SECONDS = 3

let surfaceLook = shaderMaterialClass({ vertex: WAVE_VERTEX, fragment: litFragment(), label: "wave-surface" })
let lineLook = shaderMaterialClass({ vertex: WAVE_LINE_VERTEX, fragment: LINE_FRAGMENT, label: "wave-lines" })

function App() {
  // The sphere with its wave channel in stream 1: stream 0 keeps the
  // generator's positions, normals and uvs untouched.
  let ball = withAttribute(sphere({ radius: 1, widthSegments: 64, heightSegments: 40, label: "ball" }), { name: "aWave", format: "float32" }, () => [0], { stream: 1 })
  let wave = geometryAttribute(ball, "aWave")!
  let pos = geometryAttribute(ball, "aPos")!
  let count = geometryVertexCount(ball, "streams")
  let triangles = ball.indices.length / 3
  let surface!: MeshNode
  onFrame(tick => {
    let t = tick / 1000
    for (let i = 0; i < count; i++) {
      let y = pos.get(i, 1)
      let x = pos.get(i, 0)
      wave.set(i, 0, WAVE_HEIGHT * Math.sin(y * 6 + t * 3) * Math.cos(x * 5 - t * 2))
    }
    // One write of the wave stream; the other buffers never move.
    updateVertices(ball, { stream: 1 })
    let shown = Math.min(triangles, Math.floor(triangles * Math.min(1, t / REVEAL_SECONDS)))
    if (surface) setDrawRange(surface, 0, shown * 3)
  })

  return (
    <window>
      <view width={pct(100)} height={pct(100)}>
        <Scene clearColor={[0.85, 0.87, 0.9, 1]} label="streams">
          <PerspectiveCamera fov={45} position={[0, 1.4, 3.4]} lookAt={[0, 0, 0]} />
          <HemisphereLight sky={[0.55, 0.58, 0.62]} ground={[0.25, 0.22, 0.2]} />
          <DirectionalLight color={[1, 0.96, 0.9]} intensity={0.8} position={[3, 4, 2]} direction={[-3, -4, -2]} />
          <Mesh geometry={ball} material={surfaceLook.instance({ params: { uColor: [0.9, 0.55, 0.3, 1], uSpecular: 0.3, uShininess: 40 } })} ref={m => (surface = m)} />
          <Mesh geometry={wireframeGeometry(ball)} material={lineLook.instance()} />
        </Scene>
      </view>
    </window>
  )
}

render(() => <App />)
