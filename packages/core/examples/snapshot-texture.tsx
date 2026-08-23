// A UI subtree as a live texture. snapshotTexture(ref) returns the texture id
// behind a repaintBoundary="snapshot" view: its rasterized pixels, as an
// ordinary texture id any GPU consumer samples. Here a shader texture binds
// it as uPanel and a sibling <texture> shows the result, so the panel on the
// left and its warped twin on the right are the same pixels. The id is
// stable; the runtime re-points it after every re-rasterization, and the
// boundary only re-rasterizes when its subtree changes - so the warp
// animates every frame while the panel's text is painted once per edit.
//
// Tap the panel to count. Both copies update, the mirror through the GPU.
import { render, onFrame, createSignal, snapshotTexture, Show } from "@solidrt/core"
import { createShaderTexture, glsl } from "@solidrt/core/gpu"

let MIRROR = glsl`
  uniform sampler2D uPanel;
  uniform float uTime;
  void main() {
    vec2 uv = vUV;
    uv.x += sin(uv.y * 20.0 + uTime * 3.0) * 0.02;
    vec4 panel = texture(uPanel, uv);
    // Premultiplied source: tint the color, keep the alpha.
    float glow = 0.5 + 0.5 * sin(uTime * 2.0 + uv.y * 6.0);
    fragColor = vec4(panel.rgb * vec3(1.0, 0.7 + 0.3 * glow, 0.6), panel.a);
  }
`

function App() {
  let [count, setCount] = createSignal(0)
  let [time, setTime] = createSignal(0)
  onFrame(tick => setTime(tick / 1000))

  let [panel, setPanel] = createSignal<{ id: number }>()

  return (
    <window alignItems="center" justifyContent="center" gap={40} flexDirection="row">
      <view
        ref={(n: { id: number }) => setPanel(n)}
        repaintBoundary="snapshot"
        onPointerDown={() => setCount(c => c + 1)}
        width={240}
        height={160}
        padding={20}
        gap={12}
        flexDirection="column"
      >
        <rect position="absolute" width="100%" height="100%" radius={12} color="#1e2a44" />
        <text fontSize={20} color="#ffffff">
          Live panel
        </text>
        <text fontSize={40} color="#ffd166">
          {String(count())}
        </text>
        <text fontSize={14} color="#9fb3d9">
          tap to count
        </text>
      </view>
      <Show when={panel()} keyed>
        {p => {
          // The id is valid as soon as the boundary exists; the texture is
          // empty until its first paint, then live.
          let mirror = createShaderTexture(MIRROR, 480, 320, { uTime: 0 }, {
            textures: { uPanel: snapshotTexture(p) },
            label: "panel-mirror",
          })
          return <texture src={mirror} width={240} height={160} params={{ uTime: time() }} />
        }}
      </Show>
    </window>
  )
}

render(() => <App />)
