// The raw shading layer: compileShader compiles one stage from complete GLSL
// ES (nothing injected - the source declares its own #version, precision,
// varyings and uniforms), linkProgram links a vertex and a fragment stage
// into a program, createRenderPipeline pairs the program with draw state
// (none here: attributeless triangles are the defaults), and
// createShaderTarget builds a render target over the pipeline. One program
// can back many pipelines, one pipeline many targets, and only the compile
// step compiles, so swapping precompiled programs is free of compilation.
// Stages can be destroyed right after linking; the program keeps its own
// compiled copies.
//
// A raw program carries its own vertex stage, so a fullscreen pass is a
// covering triangle from gl_VertexID with vertexCount: 3. Compare
// gpu-shader.tsx, where the fused createShader does all of this in one call
// with an injected preamble.
import { render, onFrame, createSignal } from "@solidrt/core"
import {
  compileShader,
  createRenderPipeline,
  createShaderTarget,
  destroyShader,
  linkProgram,
} from "@solidrt/core/gpu"

let VERTEX = `#version 300 es
precision highp float;
out vec2 vUV;
void main() {
  vec2 p = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2));
  vUV = p;
  gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);
}
`

// Two fragment stages linked against the same vertex stage: two programs,
// one shared compile of the vertex half.
let WAVES = `#version 300 es
precision highp float;
in vec2 vUV;
out vec4 fragColor;
uniform float iTime;
void main() {
  float t = iTime * 2.0;
  float a = 0.5 + 0.5 * sin(vUV.x * 10.0 + t);
  float b = 0.5 + 0.5 * sin(vUV.y * 10.0 - t * 1.3);
  fragColor = vec4(a, b, 1.0 - a * b, 1.0);
}
`

// The standard header ({ header: true }) declares #version, precision,
// iResolution/iTime and fragColor, so this source only adds its own inputs.
let RINGS = `
in vec2 vUV;
void main() {
  float d = length(vUV - 0.5);
  float r = 0.5 + 0.5 * sin(d * 40.0 - iTime * 3.0);
  fragColor = vec4(r, r * 0.6, 1.0 - r, 1.0);
}
`

function App() {
  let vs = compileShader("vertex", VERTEX)
  let wavesFs = compileShader("fragment", WAVES)
  let ringsFs = compileShader("fragment", RINGS, { header: true })
  let waves = linkProgram(vs, wavesFs)
  let rings = linkProgram(vs, ringsFs)
  destroyShader(vs)
  destroyShader(wavesFs)
  destroyShader(ringsFs)

  let wavesPipeline = createRenderPipeline(waves)
  let ringsPipeline = createRenderPipeline(rings)

  let wavesId = createShaderTarget(wavesPipeline, 512, 512, { vertexCount: 3, params: { iTime: 0 } })
  let ringsId = createShaderTarget(ringsPipeline, 512, 512, { vertexCount: 3, params: { iTime: 0 } })

  let [time, setTime] = createSignal(0)
  onFrame(tick => setTime(tick / 1000))

  return (
    <window flexDirection="row" gap={16} alignItems="center" justifyContent="center">
      <texture src={wavesId} params={{ iTime: time() }} width={360} height={360} />
      <texture src={ringsId} params={{ iTime: time() }} width={360} height={360} />
    </window>
  )
}

render(() => <App />)
