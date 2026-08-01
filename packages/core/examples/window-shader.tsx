// The window shader: the finished frame renders into a runtime-owned layer
// texture and a linked program draws over it into the window, as the last
// step before present. The program samples the frame as uSource (top-left
// origin - the vertex stage flips v when mapping onto the window), gets
// iResolution in physical pixels, and draws attributeless at vertexCount
// (default 3, the covering triangle).
//
// Click anywhere to toggle the warp amount between 0 and 1: at 0 the program
// is an identity pass, which must be indistinguishable from no shader at all
// (the orientation/half-pixel regression check from the plan).
import { render, onFrame, createSignal } from "@solidrt/core"
import { compileShader, destroyShader, glsl, linkProgram } from "@solidrt/core/gpu"

let VERTEX = glsl`#version 300 es
  precision highp float;
  out vec2 vUV;
  void main() {
    vec2 p = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2));
    // uSource is top-left origin; flip v so the frame lands upright on the
    // window (the one flip of the frame path, done here in the vertex stage).
    vUV = vec2(p.x, 1.0 - p.y);
    gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);
  }
`

// { header: true } declares #version, precision, iResolution and fragColor;
// uSource, vUV, and the app's own uniforms - the time included - are
// declared here.
let WARP = glsl`
  uniform sampler2D uSource;
  uniform float uAmount;
  uniform float uTime;
  in vec2 vUV;
  void main() {
    vec2 uv = vUV;
    uv.x += sin(uv.y * 24.0 + uTime * 3.0) * 0.012 * uAmount;
    uv.y += sin(uv.x * 18.0 - uTime * 2.0) * 0.012 * uAmount;
    fragColor = texture(uSource, uv);
  }
`

function App() {
  let vs = compileShader("vertex", VERTEX)
  let fs = compileShader("fragment", WARP, { header: true })
  let warp = linkProgram(vs, fs, { label: "warp" })
  destroyShader(vs)
  destroyShader(fs)

  let [time, setTime] = createSignal(0)
  let [amount, setAmount] = createSignal(1)
  onFrame(tick => setTime(tick / 1000))

  return (
    <window
      shader={{ program: warp, params: { uTime: time(), uAmount: amount() } }}
      onPointerDown={() => setAmount(a => (a > 0 ? 0 : 1))}
      flexDirection="column"
      gap={12}
      alignItems="center"
      justifyContent="center"
    >
      <text fontSize={28} color="#222">Window shader</text>
      <view flexDirection="row" gap={12}>
        <rect width={90} height={90} radius={12} color="#0077ff" />
        <rect width={90} height={90} radius={12} color="#ff6a00" />
        <rect width={90} height={90} radius={12} color="#00c46a" />
      </view>
      <text fontSize={14} color="#666">Click to toggle warp (identity at 0)</text>
    </window>
  )
}

render(() => <App />)
