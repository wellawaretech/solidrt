// createShader compiles a GLSL ES 3.00 fragment shader and renders it into a
// texture, returning a texture id you display with <texture src={id}>. The
// texture is freed automatically when the reactive owner is disposed.
//
// There are two source dialects and the source itself picks which one applies.
// WITHOUT a #version line the runtime injects a preamble, so the body may
// reference vUV (0..1, top-left origin), iResolution, iTime, and any
// `uniform float` it declares - the left square below. A source that STARTS
// with #version 300 es is taken as complete and compiled exactly as written:
// nothing is injected and it names its own uniforms - the right square.
//
// That second dialect is what lets a shader written for somewhere else run
// here unchanged, without dropping to the raw layer; see gpu-raw-program.tsx
// for what compileShader/linkProgram are actually for (sharing one compile
// across several targets). The built-in vertex stage supplies vUV either way -
// a complete source just has to declare `in vec2 vUV;` itself - and a uniform
// named iResolution is filled with the target size by name in both dialects.
//
// iResolution is filled in for you, but iTime is NOT - drive it (and any other
// uniform) declaratively via the <texture> element's params prop; it applies at
// the next repaint, so a signal updated every frame stays paced to actual frames.
// The shader's size is baked in at creation.
import { render, onFrame, createSignal } from "@solidrt/core"
import { createShader } from "@solidrt/core/gpu"

// Injected-preamble dialect: no #version, no declarations, no main() plumbing.
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

// Complete-source dialect: declares its own version, precision, varying and
// output, and calls its time uniform uSpin rather than iTime.
let RAW_FRAGMENT = `#version 300 es
precision highp float;
in vec2 vUV;
out vec4 fragColor;
uniform float uSpin;
void main() {
  vec2 p = vUV - 0.5;
  float a = atan(p.y, p.x) + uSpin;
  float r = length(p);
  float band = 0.5 + 0.5 * sin(a * 6.0 + r * 18.0);
  fragColor = vec4(band * 0.9, band * 0.4, 1.0 - band * 0.6, 1.0);
}
`

function App() {
  let id = createShader(FRAGMENT, 512, 512, { iTime: 0 })
  let rawId = createShader(RAW_FRAGMENT, 512, 512, { uSpin: 0 })
  let [time, setTime] = createSignal(0)
  onFrame((tick) => setTime(tick / 1000))

  return (
    <window alignItems="center" justifyContent="center" flexDirection="row" gap={16}>
      <texture src={id} params={{ iTime: time() }} width={260} height={260} />
      <texture src={rawId} params={{ uSpin: time() }} width={260} height={260} />
    </window>
  )
}

render(() => <App />)
