// Compositing two GPU passes in the element tree: stack <texture> layers and
// give the upper one a blendMode. This is how several shader targets combine -
// a base pass plus an additive pass - without writing a third shader that
// samples both. The full Skia blend set is available ("plus", "screen",
// "multiply", ...), and texture alpha is premultiplied, so additive modes need
// no manual premultiplication.
//
// It is also the ONLY blending there is. A target's own draw runs with GL
// blending disabled, so overlapping geometry inside one shader or pipeline
// overwrites rather than accumulates; splitting the work across targets and
// compositing them here is the way to get transparency between passes.
//
// Click to toggle the upper layer between "plus" and "source-over". The glow
// pass paints opaque black outside its ring, so source-over hides the base
// entirely while plus adds only the lit pixels - black contributes nothing.
import { render, onFrame, createSignal } from "@solidrt/core"
import { createShader, glsl } from "@solidrt/core/gpu"

let SIZE = 360

// Base pass: a static gradient with a soft vignette. Nothing drives it, so it
// renders once at creation and then holds - shaders re-render on params writes.
let BASE = glsl`
  void main() {
    vec2 uv = vUV;
    float v = 1.0 - length(uv - 0.5) * 1.1;
    vec3 col = mix(vec3(0.04, 0.05, 0.14), vec3(0.15, 0.10, 0.42), uv.y);
    fragColor = vec4(col * v, 1.0);
  }
`

// Additive pass: a breathing ring, black everywhere else.
let GLOW = glsl`
  void main() {
    vec2 uv = vUV;
    float r = length(uv - 0.5);
    float radius = 0.28 + 0.04 * sin(iTime * 2.0);
    float ring = smoothstep(0.06, 0.0, abs(r - radius));
    fragColor = vec4(vec3(1.0, 0.55, 0.15) * ring, 1.0);
  }
`

function App() {
  let baseId = createShader(BASE, SIZE, SIZE, undefined, undefined, { label: "base" })
  let glowId = createShader(GLOW, SIZE, SIZE, { iTime: 0 }, undefined, { label: "glow" })
  let [time, setTime] = createSignal(0)
  let [mode, setMode] = createSignal<"plus" | "source-over">("plus")
  onFrame((tick) => setTime(tick / 1000))

  return (
    <window
      onPointerDown={() => setMode((m) => (m === "plus" ? "source-over" : "plus"))}
      flexDirection="column"
      alignItems="center"
      justifyContent="center"
      gap={12}
    >
      {/* The layers resolve against this box: absolute children need an
          ancestor with position "relative". */}
      <view position="relative" width={SIZE} height={SIZE}>
        <texture src={baseId} position="absolute" top={0} left={0} width={SIZE} height={SIZE} />
        <texture
          src={glowId}
          position="absolute"
          top={0}
          left={0}
          width={SIZE}
          height={SIZE}
          blendMode={mode()}
          params={{ iTime: time() }}
        />
      </view>
      <text fontSize={14} color="#888">blendMode "{mode()}" - click to toggle</text>
    </window>
  )
}

render(() => <App />)
