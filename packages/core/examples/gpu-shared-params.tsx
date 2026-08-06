// Shared target state on a draw target: values every entry reads, written
// ONCE per target instead of once per entry. A ring of quads spins under a
// single "camera" uniform (uView) and color-cycles under a shared tint
// (uTint) - one target-level write per frame however many entries the
// list holds, where per-entry setDrawParams would cost one call and one
// value's worth of JS arithmetic per quad (the cost profile @solidrt/3d's
// camera rides on). setTargetTextures is the sampler analog: the patterned
// quads read one shared uMap source, swapped for the whole target every
// two seconds.
//
// The rules to notice: an entry's OWN value beats the shared one - the
// amber quad seeds uTint at addDraw and ignores the color cycle, the
// striped quad binds its own uMap and ignores the swap - and coverage may
// be partial: the tint program never declares uMap and the patterned
// program never declares uTint, so each shared write simply skips entries
// whose program does not declare the name. Shared state is target state:
// createDrawTarget seeds it (positional params + opts.textures) before any
// entry exists, and entry add/remove/rebuild cannot lose it.
//
// Two channels drive the same shared params. uView goes imperatively
// (setTargetParams in onFrame), uTint goes declaratively: the `<texture
// params>` prop means "the target's params" on every target kind, so on a
// draw target it writes the shared record - a signal into the prop is all
// the wiring a shared value needs.
import { render, onFrame, createSignal } from "@solidrt/core"
import {
  addDraw,
  compileShader,
  createBuffer,
  createDrawTarget,
  createRenderPipeline,
  createTexture,
  glsl,
  linkProgram,
  setTargetParams,
  setTargetTextures,
} from "@solidrt/core/gpu"

let VERTEX = glsl`
  in vec2 aPos;
  uniform float uView;
  uniform vec2 uCenter;

  void main() {
    vec2 p = uCenter + aPos * 0.13;
    float c = cos(uView), s = sin(uView);
    gl_Position = vec4(c * p.x - s * p.y, s * p.x + c * p.y, 0.0, 1.0);
  }
`

let FRAGMENT_TINT = glsl`
  uniform vec4 uTint;
  void main() {
    fragColor = uTint;
  }
`

let FRAGMENT_MAP = glsl`
  uniform sampler2D uMap;
  void main() {
    fragColor = texture(uMap, gl_FragCoord.xy / 32.0);
  }
`

function App() {
  let quad = createBuffer(new Float32Array([-1, -1, 1, -1, 1, 1, -1, -1, 1, 1, -1, 1]), { label: "quad" })
  let vs = compileShader("vertex", VERTEX, { header: true })
  let attrs = [{ name: "aPos", format: "vec2" as const }]
  let tint = createRenderPipeline(linkProgram(vs, compileShader("fragment", FRAGMENT_TINT, { header: true })), {
    attributes: attrs,
  })
  let mapped = createRenderPipeline(linkProgram(vs, compileShader("fragment", FRAGMENT_MAP, { header: true })), {
    attributes: attrs,
  })

  // 2x2 patterns, nearest + repeat, so gl_FragCoord tiling shows hard cells.
  let pattern = (a: number[], b: number[]) =>
    createTexture(new Uint8Array([...a, ...b, ...b, ...a]), 2, 2, { filter: "nearest", wrap: "repeat" })
  let checker = pattern([15, 15, 20, 255], [235, 235, 235, 255])
  let ember = pattern([250, 160, 30, 255], [45, 10, 60, 255])
  let stripes = createTexture(new Uint8Array([220, 40, 60, 255, 245, 245, 245, 255]), 2, 1, {
    filter: "nearest",
    wrap: "repeat",
  })

  // The positional params argument and opts.textures seed the shared state
  // before any entry exists; the entries added below pick it up.
  let target = createDrawTarget(
    512,
    512,
    { uView: 0, uTint: [0.3, 0.8, 0.9, 1] },
    { textures: { uMap: checker }, clearColor: [0.05, 0.05, 0.09, 1], label: "shared-ring" },
  )

  const RING = 8
  for (let i = 0; i < RING; i++) {
    let a = (i / RING) * Math.PI * 2
    let uCenter = [0.62 * Math.cos(a), 0.62 * Math.sin(a)]
    if (i === 1 || i === 5) {
      // Patterned entries read the shared uMap; the one at i === 5 brings
      // its own binding and keeps its stripes through every shared swap.
      addDraw(target, mapped, { uCenter }, i === 5 ? { buffer: quad, textures: { uMap: stripes } } : { buffer: quad })
    } else if (i === 3) {
      // The override quad: its own uTint beats the shared color cycle.
      addDraw(target, tint, { uCenter, uTint: [1, 0.62, 0.1, 1] }, { buffer: quad })
    } else {
      addDraw(target, tint, { uCenter }, { buffer: quad })
    }
  }

  let [sharedTint, setSharedTint] = createSignal([0.3, 0.8, 0.9, 1])
  let mapFlip = -1
  onFrame(tick => {
    let t = tick / 1000
    // The whole ring, one write: uView spins every entry. uTint takes the
    // declarative channel instead - the signal feeds the params prop below.
    setTargetParams(target, { uView: t * 0.5 })
    setSharedTint([0.45 + 0.45 * Math.sin(t), 0.45 + 0.45 * Math.sin(t + 2.1), 0.45 + 0.45 * Math.sin(t + 4.2), 1])
    // The shared sampler source swaps every two seconds; only the entry
    // with its own uMap keeps its pattern.
    let flip = Math.floor(t / 2) % 2
    if (flip !== mapFlip) {
      mapFlip = flip
      setTargetTextures(target, { uMap: flip === 0 ? checker : ember })
    }
  })

  return (
    <window alignItems="center" justifyContent="center">
      <texture src={target} width={420} height={420} params={{ uTint: sharedTint() }} />
    </window>
  )
}

render(() => <App />)
