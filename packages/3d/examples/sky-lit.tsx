// A sky-lit scene: the background is a procedural GLSL sky (a horizon
// gradient, a sun disc and its glow, written along the view ray), and
// `scene.bakeBackground()` renders that same fragment into a cube and
// prefilters it into the roughness chain the scene's environment samples -
// Godot's sky-to-radiance bake: the sky both draws behind the meshes and
// lights them, with no light nodes at all. A row of `standard` spheres,
// metal above, dielectric below, roughness 0 to 1 across: the sharp ones
// mirror the sun and horizon, the rough ones catch the sky's soft
// gradient. The sky is a scene param (uSunDir), so the bake sees the same
// sun the backdrop draws. Drag to look around.

import { pct, render } from "@solidrt/core"
import { glsl } from "@solidrt/core/gpu"
import { Mesh, OrbitCamera, Scene, sphere, standard } from "@solidrt/3d"
import type { SceneHandle } from "@solidrt/3d"
import { normalize } from "@solidrt/3d/math"
import type { Vec3 } from "@solidrt/3d/math"

// Spheres per row, their radius and spacing.
const COUNT = 6
const RADIUS = 0.45
const GAP = 1.15
// The sun's direction.
const SUN: Vec3 = normalize([0.5, 0.4, -0.75])
// The bake's face edge: 128 is the environment default; the sun disc
// blurs into its glow at this size, which is what a rough surface wants.
const BAKE_SIZE = 128

// Horizon gradient (zenith blue to a pale horizon, a dark ground), a sun
// disc with a glow. Linear colors in, the output stage encodes for the
// screen and leaves them linear for the bake.
const SKY = glsl`
  uniform vec3 uSunDir;
  // Cosine of the disc's angular radius (about 2 degrees) and the glow's
  // falloff exponent.
  const float SUN_DISC = 0.9994;
  const float SUN_GLOW = 24.0;
  void main() {
    vec3 d = normalize(vRay);
    vec3 zenith = vec3(0.08, 0.22, 0.55);
    vec3 horizon = vec3(0.65, 0.72, 0.85);
    vec3 ground = vec3(0.05, 0.045, 0.04);
    float h = clamp(d.y, -1.0, 1.0);
    vec3 col = h >= 0.0 ? mix(horizon, zenith, pow(h, 0.6)) : mix(horizon, ground, pow(-h, 0.2));
    float s = max(dot(d, uSunDir), 0.0);
    col += vec3(1.0, 0.85, 0.6) * pow(s, SUN_GLOW) * 0.6;
    if (s > SUN_DISC) col = vec3(6.0, 5.2, 4.0);
    fragColor = outputColor(col, 1.0);
  }
`

function App() {
  let spheres: { position: Vec3; material: ReturnType<typeof standard> }[] = []
  for (let i = 0; i < COUNT; i++) {
    let roughness = i / (COUNT - 1)
    let x = (i - (COUNT - 1) / 2) * GAP
    spheres.push({ position: [x, GAP / 2, 0], material: standard({ color: [0.95, 0.93, 0.88], metalness: 1, roughness }) })
    spheres.push({ position: [x, -GAP / 2, 0], material: standard({ color: [0.8, 0.15, 0.1], metalness: 0, roughness }) })
  }
  let geometry = sphere({ radius: RADIUS, widthSegments: 48, heightSegments: 32 })
  // The ref hands out the scene with its props applied (the background
  // is set), so the bake happens right here, before the first frame.
  let lit = (scene: SceneHandle) => {
    scene.setParams({ uSunDir: SUN })
    scene.setEnvironment({ cube: scene.bakeBackground(BAKE_SIZE) })
  }
  return (
    <window>
      <view width={pct(100)} height={pct(100)}>
        <Scene background={SKY} toneMapping="aces" samples={4} label="sky-lit" ref={lit}>
          <OrbitCamera target={[0, 0, 0]} azimuth={0.3} elevation={0.15} distance={7} />
          {spheres.map(s => (
            <Mesh geometry={geometry} material={s.material} position={s.position} />
          ))}
        </Scene>
      </view>
    </window>
  )
}

render(() => <App />)
