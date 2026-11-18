// createPaintShader links a fragment shader that a <shader> element draws
// straight into the frame: one quad over the element's box, under the
// frame's transform and clips, with no offscreen texture and no render pass
// of its own. The fragment contract is createShaderTexture's, so the same
// source runs both ways: vUV is 0..1 over the box (top-left origin),
// iResolution the box in device pixels, fragColor premultiplied, and the
// source has no #version line.
//
// One program backs four panels here, each with its own uTint, all animated
// by one uTime signal from onFrame: a plain panel, a rounded one (`radius`
// clips the quad), one inside a rotated <view>, and in the bottom row the
// direct draw beside the same source through createShaderTexture and a
// <texture>, to compare the two routes on screen. A second program samples
// a small checker texture through the `textures` prop.
//
// The rule: a program that only generates the pixels of one box is a
// <shader>. A program whose output something else samples, or that samples
// its own output, stays a shader texture.
import { render, onFrame, createSignal } from "@solidrt/core"
import { createPaintShader, createShaderTexture, createTexture, glsl } from "@solidrt/core/gpu"
import type { Element } from "@solidrt/core"

// The panel box in logical pixels; the shader texture is made at this size.
const PANEL = 180
// The gap between panels and between a panel and its caption.
const GAP = 12
// The rounded panel's corner radius.
const CORNER = 32
// The rotated panel's angle in radians.
const TILT = 0.3
// Checker texture: cells per side, and how often it tiles across a panel.
const CHECKER_CELLS = 8
const CHECKER_REPEAT = 3

// A plasma tinted by uTint. The bands' frequencies and speeds are named.
let PLASMA = glsl`
  uniform float uTime;
  uniform vec3 uTint;
  const float FREQ_X = 9.0;
  const float FREQ_Y = 7.0;
  const float FREQ_R = 14.0;
  const float SPEED = 1.7;
  void main() {
    vec2 p = vUV * 2.0 - 1.0;
    float v = sin(p.x * FREQ_X + uTime * SPEED)
      + sin(p.y * FREQ_Y - uTime)
      + sin(length(p) * FREQ_R - uTime * SPEED);
    float band = 0.5 + 0.5 * sin(v);
    fragColor = vec4(uTint * band, 1.0);
  }
`

// The plasma multiplied by a sampled texture, tiled across the box.
let PATTERNED = glsl`
  uniform float uTime;
  uniform vec3 uTint;
  uniform sampler2D uPattern;
  const float REPEAT = ${CHECKER_REPEAT}.0;
  const float FREQ_X = 6.0;
  const float FREQ_Y = 4.0;
  const float SPEED = 1.7;
  void main() {
    float band = 0.5 + 0.5 * sin(vUV.x * FREQ_X + vUV.y * FREQ_Y + uTime * SPEED);
    vec4 cell = texture(uPattern, vUV * REPEAT);
    fragColor = vec4(uTint * band * cell.rgb, 1.0);
  }
`

let TINTS = {
  plain: [1.0, 0.55, 0.2],
  rounded: [0.3, 0.8, 1.0],
  rotated: [0.7, 1.0, 0.4],
  patterned: [1.0, 0.9, 0.6],
  compared: [0.95, 0.45, 0.9],
}

// An opaque two-tone checker, CHECKER_CELLS square, nearest-filtered so its
// cells stay sharp.
function checkerPixels(): Uint8Array {
  let data = new Uint8Array(CHECKER_CELLS * CHECKER_CELLS * 4)
  for (let y = 0; y < CHECKER_CELLS; y++) {
    for (let x = 0; x < CHECKER_CELLS; x++) {
      let on = (x + y) % 2 === 0
      let i = (y * CHECKER_CELLS + x) * 4
      data[i] = on ? 255 : 60
      data[i + 1] = on ? 255 : 60
      data[i + 2] = on ? 255 : 90
      data[i + 3] = 255
    }
  }
  return data
}

function Panel(props: { caption: string; children: Element }) {
  return (
    <view flexDirection="column" alignItems="center" gap={GAP}>
      {props.children}
      <text fontSize={13} color="#555">
        {props.caption}
      </text>
    </view>
  )
}

function App() {
  // The labels name the resources in the dev tooling's GPU inventory.
  let plasma = createPaintShader(PLASMA, { label: "plasma" })
  let patterned = createPaintShader(PATTERNED, { label: "patterned" })
  let checker = createTexture(checkerPixels(), CHECKER_CELLS, CHECKER_CELLS, { filter: "nearest", label: "checker" })
  let target = createShaderTexture(PLASMA, PANEL, PANEL, { uTime: 0, uTint: TINTS.compared }, { label: "plasma-texture" })

  let [time, setTime] = createSignal(0)
  onFrame((tick) => setTime(tick / 1000))

  return (
    <window alignItems="center" justifyContent="center" gap={GAP * 2}>
      <view flexDirection="row" gap={GAP * 2}>
        <Panel caption="shader">
          <shader src={plasma} params={{ uTime: time(), uTint: TINTS.plain }} width={PANEL} height={PANEL} />
        </Panel>
        <Panel caption="shader radius">
          <shader
            src={plasma}
            params={{ uTime: time(), uTint: TINTS.rounded }}
            radius={CORNER}
            width={PANEL}
            height={PANEL}
          />
        </Panel>
        <Panel caption="shader in a rotated view">
          <view rotate={TILT}>
            <shader src={plasma} params={{ uTime: time(), uTint: TINTS.rotated }} width={PANEL} height={PANEL} />
          </view>
        </Panel>
      </view>
      <view flexDirection="row" gap={GAP * 2}>
        <Panel caption="shader textures">
          <shader
            src={patterned}
            params={{ uTime: time(), uTint: TINTS.patterned }}
            textures={{ uPattern: { id: checker, wrap: "repeat" } }}
            width={PANEL}
            height={PANEL}
          />
        </Panel>
        <Panel caption="shader (direct)">
          <shader src={plasma} params={{ uTime: time(), uTint: TINTS.compared }} width={PANEL} height={PANEL} />
        </Panel>
        <Panel caption="shader texture">
          <texture src={target} params={{ uTime: time() }} width={PANEL} height={PANEL} />
        </Panel>
      </view>
    </window>
  )
}

render(() => <App />)
