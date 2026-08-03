// Source history on a boundary shader: previous: true retains the prior
// rasterization of the subtree as uPrevious, rotated when the content
// actually changes - not per frame. That makes it transition material: on
// each content change uPrevious holds the old look and uSource the new, and
// uMix sweeps a cross-dissolve between them. For a static panel uPrevious
// equals uSource (the previous rasterization IS the same content), so
// feedback/accumulation is not what this is for - that stays with manual
// targets.
//
// Click the panel to cycle its colors: the old palette dissolves into the
// new one instead of snapping.
import { render, onFrame, createSignal } from "@solidrt/core"
import { compileShader, destroyShader, glsl, linkProgram } from "@solidrt/core/gpu"

let VERTEX = glsl`
  out vec2 vUV;
  void main() {
    vec2 p = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2));
    vUV = p;
    gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);
  }
`

let DISSOLVE = glsl`
  uniform sampler2D uSource;
  uniform sampler2D uPrevious;
  uniform float uMix;
  in vec2 vUV;
  void main() {
    fragColor = mix(texture(uPrevious, vUV), texture(uSource, vUV), uMix);
  }
`

const PALETTES = [
  ["#0077ff", "#ff6a00", "#00c46a"],
  ["#e63946", "#ffb703", "#8338ec"],
  ["#00b4d8", "#ef476f", "#ffd166"],
] as const

function App() {
  let vs = compileShader("vertex", VERTEX, { header: true })
  let fs = compileShader("fragment", DISSOLVE, { header: true })
  let dissolve = linkProgram(vs, fs, { label: "dissolve" })
  destroyShader(vs)
  destroyShader(fs)

  let [palette, setPalette] = createSignal(0)
  let colors = () => PALETTES[palette() % PALETTES.length]!
  let [mixv, setMix] = createSignal(1)
  // Sweep uMix back to 1 after each change (~250ms). At 1 the signal stops
  // changing, so the pass stops re-running.
  onFrame((_tick, _frame, rate) => setMix(m => Math.min(1, m + 4 / rate)))

  let cycle = () => {
    setPalette(p => (p + 1) % PALETTES.length)
    setMix(0)
  }

  return (
    <window alignItems="center" justifyContent="center">
      <view
        repaintBoundary="snapshot"
        shader={{ program: dissolve, params: { uMix: mixv() }, previous: true }}
        onPointerDown={cycle}
        flexDirection="column"
        gap={12}
        alignItems="center"
        justifyContent="center"
        width={360}
        height={240}
      >
        <rect position="absolute" width="100%" height="100%" radius={16} color="#dde3ec" />
        <text fontSize={24} color="#222">Boundary history</text>
        <view flexDirection="row" gap={12}>
          <rect width={70} height={70} radius={12} color={colors()[0]} />
          <rect width={70} height={70} radius={12} color={colors()[1]} />
          <rect width={70} height={70} radius={12} color={colors()[2]} />
        </view>
        <text fontSize={13} color="#666">Click: colors cross-dissolve</text>
      </view>
    </window>
  )
}

render(() => <App />)
