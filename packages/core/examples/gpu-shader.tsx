// createShaderTexture compiles a GLSL ES 3.00 fragment shader and renders it
// into a texture, returning a texture id you display with <texture src={id}>
// (the name says what comes back). The texture is freed automatically when
// the reactive owner is disposed.
//
// There are two source dialects and the source itself picks which one applies.
// WITHOUT a #version line the runtime injects a preamble, so the body may
// reference vUV (0..1, top-left origin), iResolution, and any uniform it
// declares - the left square below. A source that STARTS
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
// The preamble declares exactly what the runtime provides; anything the app
// drives is the shader's own declaration in both dialects - `uniform float
// uTime;` below - fed declaratively via the <texture> element's params prop.
// A param applies at the next repaint, so a signal updated every frame stays
// paced to actual frames. A param value is a number for a scalar uniform or a
// flat number array for a typed one (2/3/4 numbers for vec2/vec3/vec4, 16
// column-major for mat4), dispatched by the shader's own declaration - uTint
// below is a vec3 driven from one array value. The shader's size is baked in
// at creation.
import { render, onFrame, createSignal } from "@solidrt/core"
import { createShaderTexture, glsl } from "@solidrt/core/gpu"

// Injected-preamble dialect: no #version, no main() plumbing; uniforms the
// app drives are declared here, like any other.
let FRAGMENT = glsl`
  uniform float uTime;
  void main() {
    vec2 uv = vUV;
    float t = uTime * 2.0;
    float a = 0.5 + 0.5 * sin(uv.x * 10.0 + t);
    float b = 0.5 + 0.5 * sin(uv.y * 10.0 - t * 1.3);
    float c = 0.5 + 0.5 * sin((uv.x + uv.y) * 8.0 + t * 0.7);
    fragColor = vec4(a, b, c, 1.0);
  }
`

// Complete-source dialect: declares its own version, precision, varying and
// output, and calls its time uniform uSpin. uTint is a typed (vec3) uniform,
// filled from a 3-number array param.
let RAW_FRAGMENT = glsl`#version 300 es
  precision highp float;
  in vec2 vUV;
  out vec4 fragColor;
  uniform float uSpin;
  uniform vec3 uTint;
  void main() {
    vec2 p = vUV - 0.5;
    float a = atan(p.y, p.x) + uSpin;
    float r = length(p);
    float band = 0.5 + 0.5 * sin(a * 6.0 + r * 18.0);
    fragColor = vec4(band * uTint.r, band * uTint.g, 1.0 - band * uTint.b, 1.0);
  }
`

function App() {
  // The label names the target in the dev tooling's GPU inventory and in
  // engine log messages - free-form, purely diagnostic, worth the habit.
  let id = createShaderTexture(FRAGMENT, 512, 512, { uTime: 0 }, { label: "waves" })
  let rawId = createShaderTexture(RAW_FRAGMENT, 512, 512, { uSpin: 0, uTint: [0.9, 0.4, 0.6] }, { label: "spiral" })
  let [time, setTime] = createSignal(0)
  onFrame((tick) => setTime(tick / 1000))

  return (
    <window alignItems="center" justifyContent="center" flexDirection="row" gap={16}>
      <texture src={id} params={{ uTime: time() }} width={260} height={260} />
      <texture src={rawId} params={{ uSpin: time() }} width={260} height={260} />
    </window>
  )
}

render(() => <App />)
