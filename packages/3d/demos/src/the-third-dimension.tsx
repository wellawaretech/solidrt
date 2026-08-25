// The Third Dimension - a GPU demo on @solidrt/3d.
//
// One file: the scene, its GLSL and the debug commands. The shader
// sources are the "Shaders" section below.
//
// The scene is two meshes - a stock torusKnot() placed by transform and a
// stock plane() rotated flat - each drawn by its own shaderMaterial (custom
// GLSL), depth tested and back-face culled in the scene's single GPU pass,
// over a backdrop shader drawn as that pass's first entry. Three real
// directional lights (red, green and blue, evenly spaced in azimuth) plus a
// hemisphere ambient are scene light NODES: the custom shaders read them
// through @solidrt/3d's standard light uniforms, so nothing about the
// shading is a constant baked into the GLSL, and lights cost no extra pass.
// There are no shadows - the library has no shadow-map support, and this
// demo does not fake one. An orbit
// camera (createOrbitCamera) owns the pose: drag to rotate, wheel or pinch
// to zoom, and an auto-orbit that pauses while dragging (a tap, click or
// space toggles it). Per-frame JS cost is
// constant no matter how many triangles the pass covers - one update(dt),
// and the scene's own single shared camera write when the pose changed.

import {
  capabilities,
  createEffect,
  createMemo,
  createSignal,
  env,
  flush,
  onFrame,
  render,
  safeArea,
  untrack,
  windowSize,
} from "@solidrt/core"
import { glsl, limits } from "@solidrt/core/gpu"
import {
  add,
  createDirectionalLight,
  createHemisphereLight,
  createMesh,
  createOrbitCamera,
  createScene,
  plane,
  setTransform,
  shaderMaterial,
  torusKnot,
  STANDARD_FLOATS,
} from "@solidrt/3d"
import type { OrbitCamera, Vec3 } from "@solidrt/3d"
import { BLINN_SPECULAR, HEMISPHERE, LAMBERT, LIT_VERTEX, MAX_LIGHTS } from "@solidrt/3d/glsl"
import { registerDebug } from "srt:dev"

const KNOT_P = 2
const KNOT_Q = 3
/** Where the knot stands, and the orbit camera's pivot. */
const KNOT_CENTER: Vec3 = [0, 1.4, 0]
const FLOOR_SIZE = 36 // world units across; GROUND_FRAGMENT interpolates it
const FOV = 0.85 // vertical field of view, radians (the scene camera speaks degrees)
const ORBIT_PERIOD = 20 // seconds for one full revolution
const MIN_DISTANCE = 2.6
const MAX_DISTANCE = 14
// Wheel zoom is eased by the app, not taken from orbit.handlers: a notch
// retargets the distance and the camera glides there over the next few
// frames, so a scroll reads as one continuous push instead of a staircase.
// The exponent per wheel-delta unit matches the library's own sensitivity.
const WHEEL_ZOOM = 0.0015
const ZOOM_EASE = 9 // e-foldings per second toward the pending distance
const ZOOM_EPSILON = 0.0005 // world units; inside this the glide lands and stops
// Lowest the eye may sit, world units. The ground is one back-face-culled
// quad, so the picture falls apart the instant the camera dips under y=0 -
// the floor just vanishes. A fixed elevation clamp cannot hold that line:
// eye height is target.y + distance * sin(elevation), so an elevation that
// sits comfortably high up close digs under the floor at full zoom-out.
// Hence a floor on the eye instead, turned back into an elevation clamp
// against the distance of the moment.
const EYE_MIN_Y = 0.35

// Three real directional lights, evenly spaced in azimuth and tilted down by
// the same angle: a triangular rig, so the knot is lit from every side and
// no face falls to ambient alone. These are scene light NODES, not constants
// in a shader - the materials read them through the standard uniform set, so
// setLight/setTransform on one of these re-shades the scene.
const LIGHT_ELEVATION = 0.62 // radians above the horizon
// Pure primaries, one per corner of the triangle. Because all three sit at
// the same elevation, a FLAT surface takes the same Lambert term from each,
// so they sum back to neutral on the floor - the colour separation is a
// curvature effect, and the knot is what shows it.
const LIGHT_TINTS: Vec3[] = [
  [1, 0, 0],
  [0, 1, 0],
  [0, 0, 1],
]
const LIGHT_INTENSITY = 0.72
// Where the triangle starts, so a light is not aimed straight down the
// camera's opening azimuth.
const LIGHT_PHASE = 0.6
// Floor under the display's own scale: on a 1x display there is no downscale
// to soften polygon edges (targets have no MSAA), so render the scene larger
// than the box it is drawn into and let the texture filter resolve it.
const MIN_RENDER_SCALE = 1.5

// ------ Shaders ------
//
// GLSL ES 3.00 sources for the scene. None declares `#version`, so the
// runtime injects its own preamble: `fragColor` and `iResolution` for the
// fragment stages, plus `vUV` for the fragment-only backdrop (a pipeline's
// varyings are its own vertex stage's job). Every other uniform below is
// part of @solidrt/3d's standard set, opt-in by declare-and-use.
//
// The knot and the ground are two @solidrt/3d shaderMaterials sharing one
// vertex stage - LIT_VERTEX from @solidrt/3d/glsl, the package's standard
// stage: it transforms by the per-mesh `uModel` and the target-shared
// `uViewProj`, and hands the fragments world position (vWorldPos), world
// normal (vNormal, through mat3(uNormal)) and UV (vUv). Lighting is
// therefore world-space throughout, and `uCamPos` - the camera's world
// position, which the scene writes once per camera move beside uViewProj -
// is the view vector's other end. Lighting comes from the scene's real
// light nodes through the standard uniform set (see LIGHTING below), so
// there are no app-written uniforms and no baked-in light directions: the
// meshes may be placed by any transform without the shaders caring, and
// moving a light node re-shades everything.

const LIGHTING = glsl`
  uniform vec3 uHemiSky;
  uniform vec3 uHemiGround;
  uniform int uLightCount;
  uniform vec3 uLightDir[${MAX_LIGHTS}];
  uniform vec3 uLightColor[${MAX_LIGHTS}];

  ${HEMISPHERE}
  ${LAMBERT}
  ${BLINN_SPECULAR}
`

/**
 * Knot fragment stage: Blinn-Phong (key + fill + rim + specular) over a
 * cosine palette swept along the knot.
 */
let KNOT_FRAGMENT = glsl`
  in vec3 vWorldPos;
  in vec3 vNormal;
  in vec2 vUv;

  uniform vec3 uCamPos;

  ${LIGHTING}

  const float TAU = 6.28318530718;

  // Cosine gradient swept along the knot. The channel phases are a third of a
  // cycle apart, so no value of t lands on three equal channels - the sweep
  // never passes through grey.
  vec3 palette(float t) {
    vec3 hue = vec3(0.50) + vec3(0.48) * cos(TAU * (t + vec3(0.00, 0.33, 0.67)));
    // Pulled toward a deep cool tone rather than toward white: the hue sweep
    // stays rich instead of going pastel, and leaves headroom for the
    // specular highlight to be the brightest thing on the surface.
    return mix(hue, vec3(0.16, 0.20, 0.30), 0.34);
  }

  void main() {
    vec3 view = normalize(uCamPos - vWorldPos);
    vec3 n = normalize(vNormal);

    vec3 base = palette(vUv.x + 0.05 * sin(vUv.y * TAU * 3.0));
    // Faint bands around the tube, so the surface reads as a swept ring.
    float band = smoothstep(0.30, 0.50, abs(fract(vUv.x * 64.0) - 0.5));
    base *= mix(0.82, 1.0, band);

    vec3 light = hemisphere(n, uHemiSky, uHemiGround);
    vec3 spec = vec3(0.0);
    for (int i = 0; i < ${MAX_LIGHTS}; i++) {
      if (i >= uLightCount) break;
      vec3 l = uLightDir[i];
      light += uLightColor[i] * lambert(n, l);
      spec += uLightColor[i] * blinnSpecular(n, view, l, 64.0);
    }

    vec3 lit = base * light + spec * 0.45;

    // Fresnel rim tinted by the surface's own color, so grazing angles
    // brighten the silhouette instead of bleaching it toward white. A
    // view-dependent term of the shading model, not a stand-in light.
    float rim = pow(1.0 - max(dot(n, view), 0.0), 3.5);
    lit += mix(vec3(0.25, 0.45, 0.85), base, 0.5) * rim * 0.30;

    fragColor = vec4(pow(clamp(lit, 0.0, 1.0), vec3(0.92)), 1.0);
  }
`

/**
 * Ground fragment stage: a distance-faded checkerboard plus a soft
 * contact-shadow disc, over the plane's own UVs scaled to its world
 * footprint (FLOOR_SIZE, interpolated), so the mesh can be a stock plane()
 * under any transform. Output is premultiplied
 * alpha - the ground fades to fully transparent so the scene's background
 * entry shows through it. That fade is why the material is `transparent`.
 */
let GROUND_FRAGMENT = glsl`
  in vec2 vUv;
  in vec3 vNormal;

  ${LIGHTING}

  // World units per square.
  const float TILE = 1.15;

  /**
   * Box-filtered checkerboard: the ANALYTIC average of the checker over the
   * pixel's footprint, not a point sample. A plain step() checker is the
   * worst case for aliasing - at grazing angles one pixel spans many squares
   * and point sampling turns them into crawling moire. Integrating the
   * square wave and differencing that integral across the footprint (w, from
   * the screen-space derivatives) gives each pixel the exact mean instead,
   * so the pattern converges smoothly to flat grey as it recedes and there
   * is nothing left to alias. Inigo Quilez's checkersGradBox, on fwidth.
   */
  float checker(vec2 p) {
    vec2 w = fwidth(p) + 0.001;
    vec2 i = 2.0 * (abs(fract((p - 0.5 * w) * 0.5) - 0.5)
                  - abs(fract((p + 0.5 * w) * 0.5) - 0.5)) / w;
    return 0.5 - 0.5 * i.x * i.y;
  }

  void main() {
    vec2 p = (vUv - 0.5) * ${FLOOR_SIZE.toFixed(1)};
    float r = length(p);
    float fade = 1.0 - smoothstep(6.0, 17.0, r);

    // Albedo, not final colour: the light term below multiplies it.
    vec3 tile = mix(vec3(0.16, 0.18, 0.22), vec3(0.52, 0.55, 0.60), checker(p / TILE));

    // The same real lights the knot uses. The floor's normal is constant, so
    // this resolves to one flat term across the plane - which is exactly
    // right for an unshadowed plane under directional lights, and is why the
    // interest here is the checker and the fade rather than the shading.
    vec3 n = normalize(vNormal);
    vec3 light = hemisphere(n, uHemiSky, uHemiGround);
    for (int i = 0; i < ${MAX_LIGHTS}; i++) {
      if (i >= uLightCount) break;
      light += uLightColor[i] * lambert(n, uLightDir[i]);
    }
    tile *= light;

    // Premultiplied, alpha being the distance fade: the floor is a solid
    // surface that dissolves into the background at the rim, so the shadow
    // simply darkens the tiles rather than contributing coverage of its own.
    fragColor = vec4(tile * fade, fade);
  }
`

/**
 * Backdrop: a static radial gradient with a touch of hash grain so the ramp
 * does not band. Handed to createScene as `background`, so it is the scene
 * pass's FIRST entry - an attributeless fullscreen triangle with depth off,
 * redrawn with the pass rather than cached in a texture of its own. It takes
 * the shader-target contract (vUV, iResolution, fragColor) either way, so
 * this source is unchanged from when it fed createShaderTexture.
 */
let BACKDROP_FRAGMENT = glsl`
  void main() {
    float d = distance(vUV, vec2(0.5, 0.40));
    vec3 near = vec3(0.075, 0.105, 0.170);
    vec3 far = vec3(0.020, 0.028, 0.048);
    vec3 col = mix(near, far, smoothstep(0.05, 0.95, d));
    float n = fract(sin(dot(vUV * iResolution, vec2(12.9898, 78.233))) * 43758.5453);
    col += (n - 0.5) * 0.010;
    fragColor = vec4(col, 1.0);
  }
`

// Pixels per logical unit the scene renders at; 0 means "follow the display".
let [renderScale, setRenderScale] = createSignal(0)
// Assigned in App; the debug commands below only run once the app is up.
let orbit!: OrbitCamera
// Distance the wheel is steering toward, or null when nothing is in flight.
let zoomTarget: number | null = null

// A tap toggles the auto-orbit, so tablets have a pause too. A tap is one
// pointer that goes down and up within TAP_SLOP and TAP_MS without a second
// finger joining: crossing the slop is what makes an orbit drag, so the two
// never overlap. Mouse clicks qualify as taps as well.
const TAP_SLOP = 8
const TAP_MS = 300
let tap: { id: number; x: number; y: number; at: number } | null = null

let clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v))

function App() {
  let knotGeometry = torusKnot({
    radius: 1.25,
    tube: 0.3,
    tubularSegments: 320,
    radialSegments: 26,
    p: KNOT_P,
    q: KNOT_Q,
    label: "knot",
  })
  let groundGeometry = plane({ width: FLOOR_SIZE, height: FLOOR_SIZE, label: "ground" })
  let triangles = (knotGeometry.indices.length + groundGeometry.indices.length) / 3
  let vertexCount = (knotGeometry.vertices.length + groundGeometry.vertices.length) / STANDARD_FLOATS
  let bytes =
    knotGeometry.vertices.byteLength +
    knotGeometry.indices.byteLength +
    groundGeometry.vertices.byteLength +
    groundGeometry.indices.byteLength
  console.log(
    `scene: ${triangles} triangles, ${vertexCount} vertices + ` +
      `${knotGeometry.indices.length + groundGeometry.indices.length} u16 indices, ` +
      `${(bytes / 1024 / 1024).toFixed(2)} MiB`,
  )

  // Render at device pixels so the knot's silhouette stays crisp on hi-DPI,
  // clamped to what the device can actually allocate.
  let targetSize = createMemo(() => {
    let size = windowSize()
    let scale = renderScale() || Math.max(env.displayScale, MIN_RENDER_SCALE)
    return {
      width: clamp(Math.round(size.width * scale), 1, limits.maxTextureSize),
      height: clamp(Math.round(size.height * scale), 1, limits.maxTextureSize),
    }
  })
  let initial = untrack(targetSize)

  // The backdrop is the scene pass's first entry (depth off, covering the
  // target), so there is no second texture layer and no resize plumbing of
  // its own - the ground's fade blends straight onto it.
  let scene = createScene(initial.width, initial.height, {
    label: "scene",
    background: BACKDROP_FRAGMENT,
  })
  scene.setCamera({ fov: (FOV * 180) / Math.PI, near: 0.1, far: 80 })
  orbit = createOrbitCamera(scene, {
    target: KNOT_CENTER,
    azimuth: 0.9,
    elevation: 0.34,
    distance: 5.4,
    minDistance: MIN_DISTANCE,
    maxDistance: MAX_DISTANCE,
    minElevation: -0.15,
    maxElevation: 1.35,
    orbitSpeed: (Math.PI * 2) / ORBIT_PERIOD,
  })
  // The rig: direction is the way each light SHINES (down and inward), which
  // the scene negates into the uLightDir the shaders read. A hemisphere light
  // supplies the ambient the fragments used to hard-code.
  add(
    scene.root,
    createHemisphereLight({ sky: [0.42, 0.48, 0.60], ground: [0.14, 0.15, 0.19], intensity: 1 }),
  )
  for (let i = 0; i < LIGHT_TINTS.length; i++) {
    let azimuth = LIGHT_PHASE + (i / LIGHT_TINTS.length) * Math.PI * 2
    let horizontal = Math.cos(LIGHT_ELEVATION)
    add(
      scene.root,
      createDirectionalLight({
        direction: [
          -horizontal * Math.cos(azimuth),
          -Math.sin(LIGHT_ELEVATION),
          -horizontal * Math.sin(azimuth),
        ],
        color: LIGHT_TINTS[i]!,
        intensity: LIGHT_INTENSITY,
      }),
    )
  }

  // The ground writes premultiplied alpha and fades to clear, so it is a
  // transparent material: the scene draws it after the opaque knot with
  // depth writes off, and the depth test keeps it behind the knot's pixels.
  // Both materials default to cull "back", so the ground quad would vanish if
  // the camera dipped below the floor plane - which the EYE_MIN_Y clamp in
  // the frame loop is there to prevent.
  let ground = createMesh(
    groundGeometry,
    shaderMaterial({
      vertex: LIT_VERTEX,
      fragment: GROUND_FRAGMENT,
      transparent: true,
      label: "ground",
    }),
  )
  let knot = createMesh(
    knotGeometry,
    shaderMaterial({ vertex: LIT_VERTEX, fragment: KNOT_FRAGMENT, label: "knot" }),
  )
  add(scene.root, ground)
  add(scene.root, knot)
  setTransform(ground, { rotation: [-Math.PI / 2, 0, 0] })
  setTransform(knot, { position: KNOT_CENTER })

  // The target follows the window; the id is stable across a resize, so the
  // <d-texture src> binding and the owner-scoped auto-free keep working.
  createEffect(targetSize, size => scene.setSize(size.width, size.height))

  let last = 0
  onFrame(tick => {
    let now = tick / 1000
    // Clamp both ends: the runtime's tick counter resets across a hot reload,
    // which makes exactly one frame's delta hugely negative.
    let dt = clamp(now - last, 0, 0.1)
    last = now
    // Glide the pending wheel zoom in: an exponential ease, framerate
    // independent because the step is 1 - e^(-rate*dt) rather than a fixed
    // fraction. It writes the pose, which orbit.update() then pushes once.
    if (zoomTarget !== null) {
      let distance = orbit.pose().distance
      let next = distance + (zoomTarget - distance) * (1 - Math.exp(-ZOOM_EASE * dt))
      if (Math.abs(zoomTarget - next) < ZOOM_EPSILON) {
        next = zoomTarget
        zoomTarget = null
      }
      orbit.set({ distance: next })
    }
    // Keep the eye above the floor: the lowest elevation that still clears
    // EYE_MIN_Y at this distance. It tightens as the zoom pulls out, so the
    // camera slides up along the limit instead of sinking through the ground.
    let pose = orbit.pose()
    let minElevation = Math.asin(clamp((EYE_MIN_Y - KNOT_CENTER[1]) / pose.distance, -1, 1))
    if (pose.elevation < minElevation) orbit.set({ elevation: minElevation })
    // The scene writes uViewProj/uCamPos itself on a pose change; there is
    // no per-mesh uniform left for the app to keep in step.
    orbit.update(dt)
  })

  return (
    <window
      fullscreen
      {...orbit.handlers}
      // Compounding from the pending distance, not the current one, so a
      // fast scroll accumulates its notches instead of each one restarting
      // the glide from wherever the last had reached.
      onWheel={e => {
        let from = zoomTarget ?? orbit.pose().distance
        zoomTarget = clamp(from * Math.exp(e.deltaY * WHEEL_ZOOM), MIN_DISTANCE, MAX_DISTANCE)
      }}
      // A pinch drives distance directly; drop the glide so the two do not
      // fight over the pose.
      onPointerDown={e => {
        zoomTarget = null
        tap = tap === null ? { id: e.pointerId, x: e.clientX, y: e.clientY, at: performance.now() } : null
        orbit.handlers.onPointerDown(e)
      }}
      onPointerUp={e => {
        orbit.handlers.onPointerUp(e)
        if (tap === null || tap.id !== e.pointerId) return
        let moved = Math.hypot(e.clientX - tap.x, e.clientY - tap.y)
        let held = performance.now() - tap.at
        tap = null
        if (moved < TAP_SLOP && held < TAP_MS) orbit.set({ orbiting: !orbit.orbiting() })
      }}
      onKeyDown={e => {
        if (e.code === "Space" || e.key === " ") orbit.set({ orbiting: !orbit.orbiting() })
      }}
    >
      <d-texture src={scene.texture} />
      <view
        flex={1}
        justifyContent="space-between"
        padding={28}
        paddingTop={safeArea().top + 28}
        paddingBottom={safeArea().bottom + 28}
      >
        <view gap={6}>
          <text color="#eef4ff" fontSize={30} fontWeight={700}>
            The Third Dimension
          </text>
          <text color="#a9bcd6" fontSize={16} fontWeight={600}>
            {`(${KNOT_P},${KNOT_Q}) torus knot - ${triangles.toLocaleString()} triangles - one GPU pass`}
          </text>
        </view>
          <text color="#a9bcd6" fontSize={16} fontWeight={600}>
          {orbit.orbiting()
            ? capabilities.touch
              ? "drag to orbit - pinch to zoom - tap to pause"
              : "drag to orbit - wheel to zoom - click or space to pause"
            : capabilities.touch
              ? "orbit paused - tap resumes"
              : "orbit paused - click or space resumes"}
        </text>
      </view>
    </window>
  )
}

// Debug commands for driving the app over MCP (list_debug / call_debug): park
// the camera at an exact pose, then snapshot. The pose reaches the scene on
// the next frame (onFrame updates the orbit every frame), and get_snapshot's
// own requested frame runs that callback first, so a park-then-snapshot
// sequence sees the parked pose. flush() applies the orbiting signal write
// before the command returns its result.
registerDebug("camera", (args?: Record<string, unknown>) => {
  if (typeof args?.distance === "number") zoomTarget = null
  orbit.set({
    azimuth: typeof args?.azimuth === "number" ? args.azimuth : undefined,
    elevation: typeof args?.elevation === "number" ? args.elevation : undefined,
    distance: typeof args?.distance === "number" ? args.distance : undefined,
    orbiting: typeof args?.orbiting === "boolean" ? args.orbiting : undefined,
  })
  flush()
  return { ...orbit.pose(), orbiting: orbit.orbiting() }
})

registerDebug("vertexLayout", () => ({ floatsPerVertex: STANDARD_FLOATS }))

// Pixels per logical unit the scene renders at (0 follows the display): the
// quality/throughput knob, and the A/B for "is this pass fill-bound?".
registerDebug("renderScale", (args?: Record<string, unknown>) => {
  if (typeof args?.scale === "number") setRenderScale(clamp(args.scale, 0, 4))
  flush()
  return { scale: renderScale() }
})

render(() => <App />)
