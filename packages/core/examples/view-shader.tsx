// A boundary shader: a view with repaintBoundary="snapshot" runs its
// rasterized subtree through a linked program and composites the result in
// its place. The program contract matches shader targets, not the window
// pass: uSource is the subtree's rasterization (top-left origin, so vUV
// needs no flip), iResolution is the boundary in physical pixels. The pass
// is split from content invalidation: animating params here re-runs only
// the pass - the panel's content is rasterized once and stays cached.
//
// outset adds a transparent margin the effect may write into: without it the
// wave clips at the layout box, with it the crests escape the box edge.
//
// Click the panel to toggle the warp between 0 and 1: at 0 the program is an
// identity pass, which must be indistinguishable from the plain snapshot.
import { render, onFrame, createSignal } from "@solidrt/core"
import { compileShader, destroyShader, glsl, linkProgram } from "@solidrt/core/gpu"

// { header: true } supplies #version and precision; the varyings are the
// program's own. Unlike the window pass there is no flip here: a target
// pass's vUV origin already matches the sampled texture's top-left.
let VERTEX = glsl`
  out vec2 vUV;
  void main() {
    vec2 p = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2));
    vUV = p;
    gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);
  }
`

let WARP = glsl`
  uniform sampler2D uSource;
  uniform float uAmount;
  uniform float uTime;
  in vec2 vUV;
  void main() {
    vec2 uv = vUV;
    uv.x += sin(uv.y * 24.0 + uTime * 3.0) * 0.03 * uAmount;
    uv.y += sin(uv.x * 18.0 - uTime * 2.0) * 0.03 * uAmount;
    fragColor = texture(uSource, uv);
  }
`

function App() {
  let vs = compileShader("vertex", VERTEX, { header: true })
  let fs = compileShader("fragment", WARP, { header: true })
  let warp = linkProgram(vs, fs, { label: "panel-warp" })
  destroyShader(vs)
  destroyShader(fs)

  let [time, setTime] = createSignal(0)
  let [amount, setAmount] = createSignal(1)
  onFrame(tick => setTime(tick / 1000))

  return (
    <window alignItems="center" justifyContent="center">
      <view
        repaintBoundary="snapshot"
        shader={{ program: warp, params: { uTime: time(), uAmount: amount() }, outset: 16 }}
        onPointerDown={() => setAmount(a => (a > 0 ? 0 : 1))}
        flexDirection="column"
        gap={12}
        alignItems="center"
        justifyContent="center"
        width={360}
        height={240}
      >
        {/* Fills the boundary, so the box edge is a content edge: the warp
            visibly ripples it out into the outset margin. */}
        <rect position="absolute" width="100%" height="100%" radius={16} color="#dde3ec" />
        <text fontSize={24} color="#222">Boundary shader</text>
        <view flexDirection="row" gap={12}>
          <rect width={70} height={70} radius={12} color="#0077ff" />
          <rect width={70} height={70} radius={12} color="#ff6a00" />
          <rect width={70} height={70} radius={12} color="#00c46a" />
        </view>
        <text fontSize={13} color="#666">Click to toggle warp (identity at 0)</text>
      </view>
    </window>
  )
}

render(() => <App />)
