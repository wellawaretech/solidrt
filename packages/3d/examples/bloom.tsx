// Bloom, the stock option and its composition with a custom resolve. The
// scene renders into its linear buffer (`scene.hdrTexture`, half float
// where the device renders it), so radiance above 1 is there to spread:
// `bloom={{ threshold, intensity, radius }}` runs a threshold pass and a
// separable blur chain over the buffer at a quarter of the target's size
// and the resolve adds the result back before `resolveColor` exposes,
// tone maps and encodes - Godot's glow in its tonemap pass, Unity's
// Bloom, Three's UnrealBloomPass before OutputPass. The custom `resolve`
// here shows the composition: it declares the stock chain's two names,
// `uBloom` and `uBloomIntensity` (start from BLOOM_RESOLVE in `/glsl`),
// adds the term and darkens the corners before the stock display stage.
// The sky is sky-lit's (a 40x sun disc, baked into the environment) with
// the sun in frame, so the disc and its mirror images on the sharp
// metals bloom and the sky's gradient does not. Drag to look around.

import { createInputMap, createPointerFeed, gamepad, pct, render } from "@solidrt/core"
import { glsl } from "@solidrt/core/gpu"
import { Mesh, OrbitCamera, orbitActions, orbitBindings, Scene, sphere, standard } from "@solidrt/3d"
import type { SceneHandle } from "@solidrt/3d"
import { normalize } from "@solidrt/3d/math"
import type { Vec3 } from "@solidrt/3d/math"

const COUNT = 6
const RADIUS = 0.45
const GAP = 1.15
// The sun sits in front of the camera, low over the horizon, so its disc
// is in frame: the disc blooms in the backdrop, its mirror images on the
// sharp metals bloom with it.
const SUN: Vec3 = normalize([-0.35, 0.3, -1])
const BAKE_SIZE = 128
// Radiance below the threshold stays out of the bloom (1 = anything
// brighter than a white surface lit to 1); the intensity is how much of
// the blurred excess is added, the radius how many blur rounds spread it.
const BLOOM = { threshold: 1.0, intensity: 0.6, radius: 2 }
// How much the corners darken in the custom resolve's vignette.
const VIGNETTE = 0.6

const SKY = glsl`
  uniform vec3 uSunDir;
  const float SUN_DISC = 0.9994;
  const float SUN_GLOW = 24.0;
  const float SUN_RADIANCE = 40.0;
  void main() {
    vec3 d = normalize(vRay);
    vec3 zenith = vec3(0.08, 0.22, 0.55);
    vec3 horizon = vec3(0.65, 0.72, 0.85);
    vec3 ground = vec3(0.05, 0.045, 0.04);
    float h = clamp(d.y, -1.0, 1.0);
    vec3 col = h >= 0.0 ? mix(horizon, zenith, pow(h, 0.6)) : mix(horizon, ground, pow(-h, 0.2));
    float s = max(dot(d, uSunDir), 0.0);
    col += vec3(1.0, 0.85, 0.6) * pow(s, SUN_GLOW) * 0.6;
    if (s > SUN_DISC) col = vec3(1.0, 0.87, 0.67) * SUN_RADIANCE;
    fragColor = vec4(col, 1.0);
  }
`

// The custom resolve: the stock bloom's term (the scene binds uBloom and
// writes uBloomIntensity on this target), then a vignette, then the stock
// display stage. The scene is opaque, so the bloom is added to
// premultiplied color as is.
const VIGNETTE_RESOLVE = glsl`
  uniform sampler2D uBloom;
  uniform float uBloomIntensity;
  const float VIGNETTE = ${VIGNETTE.toFixed(2)};
  void main() {
    vec4 s = texture(uScene, vUV);
    vec3 rgb = s.rgb + texture(uBloom, vUV).rgb * uBloomIntensity;
    vec2 c = vUV - 0.5;
    rgb *= 1.0 - VIGNETTE * dot(c, c);
    fragColor = resolveColor(rgb, s.a);
  }
`

function App() {
  let pointer = createPointerFeed()
  let input = createInputMap(orbitActions)
  input.bind(orbitBindings({ pointer, gamepad: gamepad() }))
  let spheres: { position: Vec3; material: ReturnType<typeof standard> }[] = []
  for (let i = 0; i < COUNT; i++) {
    let roughness = i / (COUNT - 1)
    let x = (i - (COUNT - 1) / 2) * GAP
    spheres.push({ position: [x, GAP / 2, 0], material: standard({ color: [0.95, 0.93, 0.88], metalness: 1, roughness }) })
    spheres.push({ position: [x, -GAP / 2, 0], material: standard({ color: [0.8, 0.15, 0.1], metalness: 0, roughness }) })
  }
  let geometry = sphere({ radius: RADIUS, widthSegments: 48, heightSegments: 32 })
  let lit = (scene: SceneHandle) => {
    scene.setParams({ uSunDir: SUN })
    scene.setEnvironment({ cube: scene.bakeBackground(BAKE_SIZE) })
  }
  return (
    <window>
      <view width={pct(100)} height={pct(100)}>
        <Scene
          background={SKY}
          toneMapping="aces"
          samples={4}
          label="bloom"
          ref={lit}
          pointer={pointer}
          bloom={BLOOM}
          resolve={VIGNETTE_RESOLVE}
        >
          <OrbitCamera input={input} target={[0, 0, 0]} azimuth={0.3} elevation={0.15} distance={7} />
          {spheres.map(s => (
            <Mesh geometry={geometry} material={s.material} position={s.position} />
          ))}
        </Scene>
      </view>
    </window>
  )
}

render(() => <App />)
