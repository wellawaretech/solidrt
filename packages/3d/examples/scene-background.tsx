// A scene background: fragment GLSL drawn as the first entry of the
// scene's own pass - no second target, no stacked texture layers, no
// resize plumbing. The source below uses only the shader-target contract
// (vUV, iResolution, fragColor), so it would work unchanged in
// createShaderTexture; here it costs nothing extra - the one scene pass
// paints backdrop and meshes together, and a static scene still renders
// zero passes when idle.

import { pct, render } from "@solidrt/core"
import { glsl } from "@solidrt/core/gpu"
import { box, Mesh, PerspectiveCamera, Scene, sphere, torusKnot, unlit } from "@solidrt/3d"

const SIZE = 720

// A radial night-sky gradient with hash grain so the ramp does not band.
let BACKDROP = glsl`
  void main() {
    float d = distance(vUV, vec2(0.5, 0.35));
    vec3 near = vec3(0.10, 0.13, 0.22);
    vec3 far = vec3(0.02, 0.03, 0.06);
    vec3 col = mix(near, far, smoothstep(0.05, 0.9, d));
    float n = fract(sin(dot(vUV * iResolution, vec2(12.9898, 78.233))) * 43758.5453);
    col += (n - 0.5) * 0.012;
    fragColor = vec4(col, 1.0);
  }
`

function App() {
  return (
    <window>
      <view width={pct(100)} height={pct(100)} viewBox={[SIZE, SIZE]}>
        <Scene width={SIZE} height={SIZE} background={BACKDROP} label="backdrop-demo">
          <PerspectiveCamera fov={55} position={[0, 1.8, 4.4]} lookAt={[0, 0.4, 0]} />
          <Mesh geometry={torusKnot(0.7, 0.2, 128, 16)} material={unlit({ color: [0.85, 0.55, 0.25] })} position={[0, 0.9, 0]} />
          <Mesh geometry={box()} material={unlit({ color: [0.3, 0.5, 0.8] })} position={[-1.5, 0.4, -0.5]} scale={0.8} />
          <Mesh geometry={sphere(0.4)} material={unlit({ color: [0.4, 0.75, 0.45] })} position={[1.5, 0.4, -0.5]} />
        </Scene>
      </view>
    </window>
  )
}

render(() => <App />)
