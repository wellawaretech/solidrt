// createShader compiles a GLSL ES 3.00 fragment shader and renders it into a
// texture, returning a texture id you display with <texture src={id}>. The
// fragment body may reference vUV (0..1, top-left origin), iResolution, iTime, and
// any `uniform float` it declares; there is no #version line - the runtime injects
// the preamble. The texture is freed automatically when the reactive owner is
// disposed.
//
// iResolution is filled in for you, but iTime is NOT - drive it (and any other
// uniform) declaratively via the <texture> element's params prop; it applies at
// the next repaint, so a signal updated every frame stays paced to actual frames.
// The shader's size is baked in at creation.
import { render, onFrame, createSignal } from "@solidrt/core"
import { createShader } from "@solidrt/core/gpu"

let FRAGMENT = `
void main() {
  vec2 uv = vUV;
  float t = iTime * 2.0;
  float a = 0.5 + 0.5 * sin(uv.x * 10.0 + t);
  float b = 0.5 + 0.5 * sin(uv.y * 10.0 - t * 1.3);
  float c = 0.5 + 0.5 * sin((uv.x + uv.y) * 8.0 + t * 0.7);
  fragColor = vec4(a, b, c, 1.0);
}
`

function App() {
  let id = createShader(FRAGMENT, 512, 512, { iTime: 0 })
  let [time, setTime] = createSignal(0)
  onFrame((tick) => setTime(tick / 1000))

  return (
    <window alignItems="center" justifyContent="center">
      <texture src={id} params={{ iTime: time() }} width={400} height={400} />
    </window>
  )
}

render(() => <App />)