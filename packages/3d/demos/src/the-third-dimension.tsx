// The Third Dimension - a GPU demo on @solidrt/3d.
//
// One file: the scene, its GLSL and the debug commands. The shader
// sources are the "Shaders" section below.
//
// The scene is two meshes - a stock torusKnot() placed by transform and a
// stock plane() rotated flat - each drawn by its own shaderMaterial (custom
// GLSL), depth tested and back-face culled in the scene's own GPU pass,
// over a backdrop shader drawn as that pass's first entry. Three real
// directional lights (red, green and blue, evenly spaced in azimuth) plus a
// hemisphere ambient are scene light NODES: the custom shaders read them
// through @solidrt/3d's standard light uniforms, so nothing about the
// shading is a constant baked into the GLSL, and the lights themselves cost
// no pass. All three CAST (castShadow), which a scene allows up to
// MAX_SHADOWS = MAX_LIGHTS: each owns a depth texture drawn from its own
// orthographic frustum, and both custom fragments read the whole set back
// with SHADOW_SLOTS, SHADOW and SHADOW_LOOKUP from @solidrt/3d/glsl - the
// same three constants the lit material composes. A shadow is therefore
// the COMPLEMENT of the light it blocks - red's is cyan - where two cross
// only the third light survives and the colour goes pure, and where all
// three cross the floor falls to the ambient alone. The rig hangs off one
// Group that turns slowly, so the three shadows sweep and cross from a
// single setTransform per frame.
//
// The window is a split of THREE renderings of that one scene: the scene's
// own target taking the larger share, plus two scene.createView panels
// beside it - a side view, where the knot's weave reads because it lies
// flat, and straight down from above, where the three shadows cross. The
// split follows the window: the two panels stack down the right in
// landscape and sit side by side along the bottom in portrait. All three are draggable
// orbits with poses of their own. A view shares the scene's geometry, materials, lights and
// shadow maps; it is another target, another camera and one entry per mesh,
// and one core flush writes every entry's world matrix, so two more panels
// cost no per-frame JS. All three keep the scene's perspective projection.
// An orbit
// camera (createOrbitCamera) per panel owns its pose: drag to rotate, wheel
// or pinch to zoom, and an auto-orbit that pauses while dragging. A click or
// tap pauses the panel it lands on and only that one; space is the keyboard
// form and is global, the large view leading and the small ones taking its
// new state. The light rig follows the large view. Per-frame JS cost is
// constant no matter how many triangles the passes cover - one update(dt),
// one setTransform on the rig, and the scene's own shared camera writes
// when a pose changed.

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
import type { PointerEvent, WheelEvent } from "@solidrt/core"
import { glsl, limits } from "@solidrt/core/gpu"
import {
  add,
  createDirectionalLight,
  createGroup,
  createHemisphereLight,
  createMesh,
  createOrbitCamera,
  createScene,
  plane,
  setCastShadow,
  setLight,
  setTransform,
  shaderMaterial,
  torusKnot,
  STANDARD_FLOATS,
} from "@solidrt/3d"
import type { DirectionalLightNode, OrbitCamera, SceneNode, Vec3 } from "@solidrt/3d"
import { BLINN_SPECULAR, HEMISPHERE, LAMBERT, LIT_VERTEX, MAX_LIGHTS, SHADOW, SHADOW_LOOKUP, SHADOW_SLOTS } from "@solidrt/3d/glsl"
import { registerDebug } from "srt:dev"

const KNOT_P = 2
const KNOT_Q = 3
/** Where the knot stands, and the orbit camera's pivot. */
const KNOT_CENTER: Vec3 = [0, 1.4, 0]
const FLOOR_SIZE = 36 // world units across; GROUND_FRAGMENT interpolates it
const FOV = 0.85 // vertical field of view, radians (the scene camera speaks degrees)
const ORBIT_PERIOD = 68 // seconds for one full revolution
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
// How far back up its own ray each light NODE sits. A directional light
// needs no position to shine, but a casting one renders its shadow map from
// a camera placed at the node's world position, so it has to stand off the
// scene. They are one rig, so all three get the same placement.
const LIGHT_DISTANCE = 10
// Seconds for one full turn of the light rig - slower than the camera
// orbit, so the two motions stay legible as two.
const RIG_PERIOD = 120
// Every light casts. A scene takes MAX_SHADOWS = MAX_LIGHTS casting
// directional lights, so all three corners of the triangle get a map and a
// depth pass of their own, and each shadow is the COMPLEMENT of the light
// it blocks. The `shadow` debug command switches individual casters off to
// take the effect apart.
// The light frustum, world units either side of the light ray. The knot's
// bounding sphere is radius 2.18 (torusKnot's radius * 1.5, plus tube), and
// the frustum is a prism along the light direction, so a box that holds the
// caster also holds everywhere its shadow can land. Tight matters: the
// map's texels spread over this box, and 5.2 world units across 1024 of
// them is about 0.005 each.
const SHADOW_EXTENT = 2.6
const SHADOW_MAP = 1024
// Depth range along the light ray: the caster sits at LIGHT_DISTANCE and
// its shadow reaches the floor a few units past it.
const SHADOW_NEAR = 2
const SHADOW_FAR = 22
// The split: one large panel taking SPLIT of the window's long axis, the
// two others sharing what is left across the short one - stacked down the
// right in landscape, side by side along the bottom in portrait. They share
// edges with no gap: the rounding leftovers go to the two small panels, so
// the three tile the window exactly whichever way it turns.
const SPLIT = 0.66
// Where the lower-right panel opens: all but straight down (1.55 is the
// library's own pole guard), far enough back that the vertical FOV covers
// the whole shadow spread, about +-4 world units at the floor.
const TOP_DOWN_ELEVATION = 1.5
const TOP_DOWN_DISTANCE = 11.6
// The hero panel's opening azimuth. The side panel is placed RELATIVE to it
// rather than in world terms: every panel sweeps at the same rate, so what
// stays visible between two of them is how far apart they sit, never where
// either happens to point at t = 0.
const HERO_AZIMUTH = 0.9
// Where the upper-right panel opens, tuned on screen. The knot is a flat
// disc in the xz plane, so a side view catches it edge-on and its weave -
// which loop passes over which - reads there and nowhere else. The eye sits
// just BELOW the knot's centre (about y = 0.73 at this distance) looking
// slightly up at it, which stands the knot clear of the floor instead of
// laying it into the checker. Well inside the floor clamp: at this distance
// holdAboveFloor would not bite until about -0.124.
const SIDE_OFFSET = -3.096 // radians round from the hero
const SIDE_AZIMUTH = HERO_AZIMUTH + SIDE_OFFSET
const SIDE_ELEVATION = -0.079
const SIDE_DISTANCE = 8.5
// How far the two right-hand panels may be pushed and pulled.
const PANEL_MIN_DISTANCE = 3.5
const PANEL_MAX_DISTANCE = 30
// The extra views get no background entry of their own (a view mirrors the
// scene's meshes, not its backdrop), so this stands in for the backdrop
// shader's outer tone where the ground has faded away.
const VIEW_CLEAR: [number, number, number, number] = [0.02, 0.028, 0.048, 1]
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

  // The shadow set: one slot per directional light, all of it written by
  // the scene at target level. uShadowMap<i> is light i's depth map (a
  // white texel when it does not cast), uShadowMatrix[i] the light-space
  // view-projection that rendered it, uShadowCast[i] whether it casts, and
  // the two bias knobs are per light as well.
  ${SHADOW_SLOTS}

  ${HEMISPHERE}
  ${LAMBERT}
  ${BLINN_SPECULAR}
  ${SHADOW}
  // lightShadow(i, worldPos, n): light i's factor, 1 when it does not
  // cast - the same lookup the lit material composes.
  ${SHADOW_LOOKUP}
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
      // The knot draws into every map, so it also self-shadows: a stretch
      // of tube in the lee of another loses that light and keeps the rest,
      // which is where the colour separation gets its hardest edge.
      float s = lightShadow(i, vWorldPos, n);
      light += uLightColor[i] * lambert(n, l) * s;
      spec += uLightColor[i] * blinnSpecular(n, view, l, 64.0) * s;
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
 * Ground fragment stage: a distance-faded checkerboard, lit by the scene's
 * lights and the shadow map, over the plane's own UVs scaled to its world
 * footprint (FLOOR_SIZE, interpolated), so the mesh can be a stock plane()
 * under any transform. Output is premultiplied
 * alpha - the ground fades to fully transparent so the scene's background
 * entry shows through it. That fade is why the material is `transparent`.
 */
let GROUND_FRAGMENT = glsl`
  in vec2 vUv;
  in vec3 vNormal;
  in vec3 vWorldPos;

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
    // every directional term is flat across the whole plane and the three
    // tints sum back to neutral - until the maps take them away one at a
    // time. That is the picture here: three shadows, each keeping the two
    // lights it does not block, crossing into pure primaries where two
    // overlap and into the ambient alone where all three do. A flat plane
    // is the only place the rig's tints separate this cleanly.
    vec3 n = normalize(vNormal);
    vec3 light = hemisphere(n, uHemiSky, uHemiGround);
    for (int i = 0; i < ${MAX_LIGHTS}; i++) {
      if (i >= uLightCount) break;
      light += uLightColor[i] * lambert(n, uLightDir[i]) * lightShadow(i, vWorldPos, n);
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
// How many lights currently cast: the subtitle reads it, the `shadow`
// debug command writes it.
let [casters, setCasters] = createSignal(LIGHT_TINTS.length)
// Assigned in App; the debug commands below only run once the app is up.
// One orbit camera per panel: the large one on the left, then the two on
// the right. Only the left one auto-orbits.
let orbit!: OrbitCamera
let topOrbit!: OrbitCamera
let bottomOrbit!: OrbitCamera
let lights: DirectionalLightNode[] = []
// The light rig and where it has turned to. Module scope so the `rig` debug
// command can park it: the spin pauses with the orbit, so a parked pose and
// a parked rig together are one repeatable frame.
let rig!: SceneNode
let rigAngle = 0
// A panel as input and the frame loop see it: its camera, the distance range
// its wheel may steer within, and the eased zoom in flight (null when there
// is none). Assigned in App, in left-then-right order.
type PanelCamera = { cam: OrbitCamera; minDistance: number; maxDistance: number; zoom: number | null }
let cameras: PanelCamera[] = []

// A tap toggles the auto-orbit, so tablets have a pause too. A tap is one
// pointer that goes down and up within TAP_SLOP and TAP_MS without a second
// finger joining: crossing the slop is what makes an orbit drag, so the two
// never overlap. Mouse clicks qualify as taps as well.
const TAP_SLOP = 8
const TAP_MS = 300
let tap: { id: number; x: number; y: number; at: number } | null = null

let clamp = (v: number, lo: number, hi: number) => Math.min(hi, Math.max(lo, v))

// The keyboard's pause, and the only global one: the LARGE view leads and
// the two small ones are handed its new state, so one key puts the whole
// window in step and a paused scene is a still frame end to end. A click or
// tap is local to the panel it lands on (see panelInput). The large view's
// signal is what the hint text reads, and what the light rig follows.
let setAllOrbiting = (orbiting: boolean) => {
  orbit.set({ orbiting })
  topOrbit.set({ orbiting })
  bottomOrbit.set({ orbiting })
}

// Hold one camera's eye above the floor (see the EYE_MIN_Y note): the
// elevation floor is a function of the distance, so it has to be re-derived
// every frame rather than fixed once as a minElevation.
let holdAboveFloor = (cam: OrbitCamera) => {
  let pose = cam.pose()
  let minElevation = Math.asin(clamp((EYE_MIN_Y - KNOT_CENTER[1]) / pose.distance, -1, 1))
  if (pose.elevation < minElevation) cam.set({ elevation: minElevation })
}

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

  // The three panel boxes in logical units, tiling the window exactly: the
  // leftovers of the rounding go to the two small panels, so no seam ever
  // opens up at the far edge. `top` and `bottom` name the two VIEWS, not
  // where they land - in portrait they are the bottom-left and bottom-right
  // of the row under the hero panel.
  let panels = createMemo(() => {
    let size = windowSize()
    // x/y/w/h is the DETACHED box vocabulary (d-* only) - a d-texture takes
    // no layout width/height and would silently fall back to the inherited
    // window box, drawing all three panels on top of each other.
    if (size.height > size.width) {
      let hero = Math.round(size.height * SPLIT)
      let rest = size.height - hero
      let half = Math.round(size.width / 2)
      return {
        main: { x: 0, y: 0, w: size.width, h: hero },
        top: { x: 0, y: hero, w: half, h: rest },
        bottom: { x: half, y: hero, w: size.width - half, h: rest },
      }
    }
    let left = Math.round(size.width * SPLIT)
    let right = size.width - left
    let top = Math.round(size.height / 2)
    return {
      main: { x: 0, y: 0, w: left, h: size.height },
      top: { x: left, y: 0, w: right, h: top },
      bottom: { x: left, y: top, w: right, h: size.height - top },
    }
  })
  // Render at device pixels so the knot's silhouette stays crisp on hi-DPI,
  // clamped to what the device can actually allocate. Every panel is its own
  // target now, each sized from its own box - a small panel costs a small
  // texture, and the three together cover about the pixels one full-window
  // view used to.
  let targetSize = createMemo(() => {
    let box = panels()
    let scale = renderScale() || Math.max(env.displayScale, MIN_RENDER_SCALE)
    let pixels = (b: { w: number; h: number }) => ({
      width: clamp(Math.round(b.w * scale), 1, limits.maxTextureSize),
      height: clamp(Math.round(b.h * scale), 1, limits.maxTextureSize),
    })
    return { main: pixels(box.main), top: pixels(box.top), bottom: pixels(box.bottom) }
  })
  let initial = untrack(targetSize)

  // The backdrop is the scene pass's first entry (depth off, covering the
  // target), so there is no second texture layer and no resize plumbing of
  // its own - the ground's fade blends straight onto it.
  let scene = createScene(initial.main.width, initial.main.height, {
    label: "scene",
    background: BACKDROP_FRAGMENT,
  })
  scene.setCamera({ fov: (FOV * 180) / Math.PI, near: 0.1, far: 80 })
  orbit = createOrbitCamera(scene, {
    target: KNOT_CENTER,
    azimuth: HERO_AZIMUTH,
    elevation: 0.34,
    distance: 5.4,
    minDistance: MIN_DISTANCE,
    maxDistance: MAX_DISTANCE,
    minElevation: -0.15,
    maxElevation: 1.35,
    orbitSpeed: (Math.PI * 2) / ORBIT_PERIOD,
  })
  // The rig: direction is the way each light SHINES (down and inward), which
  // the scene negates into the uLightDir the shaders read. The hemisphere
  // light is the floor under all of it: with three shadows crossing, the
  // patch where all three lights are blocked would otherwise be pure black,
  // and this is what it falls to instead.
  add(
    scene.root,
    createHemisphereLight({ sky: [0.42, 0.48, 0.60], ground: [0.14, 0.15, 0.19], intensity: 1 }),
  )
  // The three lights hang off ONE Group, so the slow turn in the frame loop
  // stays a single setTransform however many corners the triangle grows. A
  // parent rotation carries both halves of a light: the direction it shines
  // and the position its shadow camera is placed at.
  rig = createGroup()
  add(scene.root, rig)
  lights = []
  for (let i = 0; i < LIGHT_TINTS.length; i++) {
    let azimuth = LIGHT_PHASE + (i / LIGHT_TINTS.length) * Math.PI * 2
    let horizontal = Math.cos(LIGHT_ELEVATION)
    let direction: Vec3 = [
      -horizontal * Math.cos(azimuth),
      -Math.sin(LIGHT_ELEVATION),
      -horizontal * Math.sin(azimuth),
    ]
    let light = createDirectionalLight({
      direction,
      color: LIGHT_TINTS[i]!,
      intensity: LIGHT_INTENSITY,
      castShadow: true,
      shadow: {
        mapSize: SHADOW_MAP,
        // Along the receiving surface's own normal, the knob to reach for
        // first. The depth pass culls FRONT faces, so a closed caster like
        // the knot needs no depth bias at all on top of it.
        normalBias: 0.02,
        camera: {
          left: -SHADOW_EXTENT,
          right: SHADOW_EXTENT,
          top: SHADOW_EXTENT,
          bottom: -SHADOW_EXTENT,
          near: SHADOW_NEAR,
          far: SHADOW_FAR,
        },
      },
    })
    // Each casting light's shadow camera sits AT its node's world position
    // (Three's rule), so back the light up its own ray from the knot: at
    // the origin it would render its map from inside the caster and shadow
    // nothing.
    setTransform(light, {
      position: [
        KNOT_CENTER[0] - direction[0] * LIGHT_DISTANCE,
        KNOT_CENTER[1] - direction[1] * LIGHT_DISTANCE,
        KNOT_CENTER[2] - direction[2] * LIGHT_DISTANCE,
      ],
    })
    add(rig, light)
    lights.push(light)
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
  // Only the knot casts. The ground is one single-sided quad and the depth
  // pass culls front faces, so it would draw nothing into the map anyway -
  // and its entry in the shadow view would cost a draw for nothing.
  setCastShadow(knot, true)

  // The three renderings of ONE scene. A view shares the scene's geometry,
  // materials, lights and shadow maps - it is another target, another camera
  // and one entry per mesh, and the core's flush writes every entry's world
  // matrix at once, so the extra panels cost the app nothing per frame.
  let topView = scene.createView({
    width: initial.top.width,
    height: initial.top.height,
    clearColor: VIEW_CLEAR,
    label: "top-right",
  })
  topView.setCamera({ fov: (FOV * 180) / Math.PI, near: 0.1, far: 80 })
  let bottomView = scene.createView({
    width: initial.bottom.width,
    height: initial.bottom.height,
    clearColor: VIEW_CLEAR,
    label: "bottom-right",
  })
  bottomView.setCamera({ fov: (FOV * 180) / Math.PI, near: 0.1, far: 80 })
  // A view has the same setCamera as a scene, which is all an orbit camera
  // ever touches, so each panel gets a real one: drag, pinch and wheel, and
  // a pose of its own. The upper panel opens from the side and the lower one
  // all but straight down - the two orthogonal readings a single orbiting
  // hero shot keeps sweeping past. All three auto-orbit at the same rate, so
  // they hold their phase offsets and read as three cameras on one
  // turntable. Each pauses on its own click; space stops all three.
  topOrbit = createOrbitCamera(topView, {
    target: KNOT_CENTER,
    azimuth: SIDE_AZIMUTH,
    elevation: SIDE_ELEVATION,
    distance: SIDE_DISTANCE,
    minDistance: PANEL_MIN_DISTANCE,
    maxDistance: PANEL_MAX_DISTANCE,
    minElevation: -0.15,
    maxElevation: 1.55,
    orbitSpeed: (Math.PI * 2) / ORBIT_PERIOD,
  })
  bottomOrbit = createOrbitCamera(bottomView, {
    target: KNOT_CENTER,
    azimuth: HERO_AZIMUTH,
    elevation: TOP_DOWN_ELEVATION,
    distance: TOP_DOWN_DISTANCE,
    minDistance: PANEL_MIN_DISTANCE,
    maxDistance: PANEL_MAX_DISTANCE,
    minElevation: -0.15,
    maxElevation: 1.55,
    orbitSpeed: (Math.PI * 2) / ORBIT_PERIOD,
  })
  cameras = [
    { cam: orbit, minDistance: MIN_DISTANCE, maxDistance: MAX_DISTANCE, zoom: null },
    { cam: topOrbit, minDistance: PANEL_MIN_DISTANCE, maxDistance: PANEL_MAX_DISTANCE, zoom: null },
    { cam: bottomOrbit, minDistance: PANEL_MIN_DISTANCE, maxDistance: PANEL_MAX_DISTANCE, zoom: null },
  ]

  // Every target follows its own panel; the ids are stable across a resize,
  // so the <d-texture src> bindings and the owner-scoped auto-free keep
  // working.
  createEffect(targetSize, size => {
    scene.setSize(size.main.width, size.main.height)
    topView.setSize(size.top.width, size.top.height)
    bottomView.setSize(size.bottom.width, size.bottom.height)
  })

  // One panel's input, spread onto its own texture leaf: that camera's drag
  // and pinch, an eased wheel, and the tap that toggles the global pause.
  // Which panel a gesture belongs to is the ENGINE's answer, not the app's -
  // the runtime freezes each pointer's hit path at the down and delivers
  // every later event along it, so a drag that runs off its panel keeps
  // arriving here with no capture to arrange, and two fingers on one panel
  // both land on it, which is what makes the pinch work.
  let panelInput = (panel: PanelCamera) => ({
    onPointerDown: (e: PointerEvent) => {
      // A pinch drives distance directly; drop this panel's glide so the two
      // do not fight over the pose.
      panel.zoom = null
      tap = tap === null ? { id: e.pointerId, x: e.clientX, y: e.clientY, at: performance.now() } : null
      panel.cam.handlers.onPointerDown(e)
    },
    onPointerMove: (e: PointerEvent) => panel.cam.handlers.onPointerMove(e),
    onPointerUp: (e: PointerEvent) => {
      panel.cam.handlers.onPointerUp(e)
      if (tap === null || tap.id !== e.pointerId) return
      let moved = Math.hypot(e.clientX - tap.x, e.clientY - tap.y)
      let held = performance.now() - tap.at
      tap = null
      // A tap pauses THIS panel and nothing else - the other two keep
      // sweeping, so one view can be held still to study while the rest of
      // the window carries on. Space is the global form.
      if (moved < TAP_SLOP && held < TAP_MS) panel.cam.set({ orbiting: !panel.cam.orbiting() })
    },
    // A notch retargets this panel's distance and the camera glides there
    // over the next few frames, so a scroll reads as one continuous push
    // instead of a staircase. Compounding from the PENDING distance rather
    // than the current one, so a fast scroll accumulates its notches instead
    // of each one restarting the glide from wherever the last had reached.
    onWheel: (e: WheelEvent) => {
      let from = panel.zoom ?? panel.cam.pose().distance
      panel.zoom = clamp(from * Math.exp(e.deltaY * WHEEL_ZOOM), panel.minDistance, panel.maxDistance)
    },
  })
  let input = cameras.map(panelInput)

  let last = 0
  onFrame(tick => {
    let now = tick / 1000
    // Clamp both ends: the runtime's tick counter resets across a hot reload,
    // which makes exactly one frame's delta hugely negative.
    let dt = clamp(now - last, 0, 0.1)
    last = now
    // Every panel in turn: glide its pending wheel zoom, then hold its eye
    // above the floor.
    for (let panel of cameras) {
      // The glide is an exponential ease, framerate independent because the
      // step is 1 - e^(-rate*dt) rather than a fixed fraction. It writes the
      // pose, which the camera's own update() then pushes once.
      if (panel.zoom !== null) {
        let distance = panel.cam.pose().distance
        let next = distance + (panel.zoom - distance) * (1 - Math.exp(-ZOOM_EASE * dt))
        if (Math.abs(panel.zoom - next) < ZOOM_EPSILON) {
          next = panel.zoom
          panel.zoom = null
        }
        panel.cam.set({ distance: next })
      }
      // Keep the eye above the floor: the lowest elevation that still clears
      // EYE_MIN_Y at this camera's distance. It tightens as the zoom pulls
      // out, so a camera slides up along the limit instead of sinking through
      // the ground - and the ground is one back-face-culled quad, so under it
      // the floor and its shadows simply vanish.
      holdAboveFloor(panel.cam)
    }
    // Turn the light rig, on the same pause as the orbit so a parked scene
    // is a still frame end to end and snapshots of one pose repeat. This
    // single write moves three lights, their direction slots and the shadow
    // camera; the map is then re-rendered for the frame, which is what
    // pausing buys back.
    if (orbit.orbiting()) {
      rigAngle = (rigAngle + (dt * Math.PI * 2) / RIG_PERIOD) % (Math.PI * 2)
      setTransform(rig, { rotation: [0, rigAngle, 0] })
    }
    // Each panel's target gets its own uViewProj/uCamPos write, and only
    // when that pose actually changed - update() reports it and skips.
    for (let panel of cameras) panel.cam.update(dt)
  })

  return (
    <window
      fullscreen
      onKeyDown={e => {
        // Keyboard-only, and global: the large view leads, the small ones
        // take whatever it just became.
        if (e.code === "Space" || e.key === " ") setAllOrbiting(!orbit.orbiting())
      }}
    >
      <d-texture src={scene.texture} {...panels().main} {...input[0]} />
      <d-texture src={topView.texture} {...panels().top} {...input[1]} />
      <d-texture src={bottomView.texture} {...panels().bottom} {...input[2]} />
      <view
        width={panels().main.w}
        height={panels().main.h}
        // Decoration only: it covers the whole hero panel, so without this a
        // drag anywhere over the title or the hint would hit the overlay and
        // never reach the texture leaf behind it.
        pointerEvents="none"
        justifyContent="space-between"
        padding={28}
        paddingTop={safeArea().top + 28}
        // The hero panel only reaches the window's bottom edge in landscape;
        // in portrait the two small panels sit under it and the inset there
        // belongs to them.
        paddingBottom={panels().main.h === windowSize().height ? safeArea().bottom + 28 : 28}
      >
        <view gap={6}>
          <text color="#eef4ff" fontSize={30} fontWeight={700}>
            The Third Dimension
          </text>
          <text color="#a9bcd6" fontSize={16} fontWeight={600}>
            {`(${KNOT_P},${KNOT_Q}) torus knot - ${triangles.toLocaleString()} triangles - ${
              casters() > 0 ? `${casters()} shadow map${casters() === 1 ? "" : "s"} - ` : ""
            }3 views of one scene`}
          </text>
        </view>
          <text color="#a9bcd6" fontSize={16} fontWeight={600}>
          {orbit.orbiting()
            ? capabilities.touch
              ? "drag a panel to orbit it - pinch to zoom - tap one to pause it"
              : "drag a panel to orbit it - wheel to zoom - click one to pause it, space for all"
            : capabilities.touch
              ? "paused - tap a panel to resume it"
              : "paused - click a panel to resume it, space for all"}
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
  // Which panel to park: "top" or "bottom" for the right-hand pair, the
  // large one otherwise. A pose is always one panel's; `orbiting` follows the
  // app's own split - named panel, that panel alone (a click), no panel, all
  // three (space). A parked distance also drops that panel's pending wheel
  // glide, which would otherwise slide it straight back off.
  let panel = args?.panel === "top" || args?.panel === "bottom" ? args.panel : "main"
  let cam = panel === "top" ? topOrbit : panel === "bottom" ? bottomOrbit : orbit
  if (typeof args?.distance === "number") {
    let entry = cameras.find(p => p.cam === cam)
    if (entry !== undefined) entry.zoom = null
  }
  if (typeof args?.orbiting === "boolean") {
    if (args.panel === undefined) setAllOrbiting(args.orbiting)
    else cam.set({ orbiting: args.orbiting })
  }
  cam.set({
    azimuth: typeof args?.azimuth === "number" ? args.azimuth : undefined,
    elevation: typeof args?.elevation === "number" ? args.elevation : undefined,
    distance: typeof args?.distance === "number" ? args.distance : undefined,
  })
  flush()
  return { panel, ...cam.pose(), orbiting: cam.orbiting() }
})

registerDebug("vertexLayout", () => ({ floatsPerVertex: STANDARD_FLOATS }))

// Where the light rig has turned to, radians. Its spin is paused with the
// orbit, so parking both is what makes a snapshot repeat.
registerDebug("rig", (args?: Record<string, unknown>) => {
  if (typeof args?.angle === "number") {
    rigAngle = args.angle % (Math.PI * 2)
    setTransform(rig, { rotation: [0, rigAngle, 0] })
  }
  flush()
  return { angle: rigAngle, period: RIG_PERIOD }
})

// Switch one light's shadow off or on: `{ light: 1, cast: false }`. Every
// light casts by default now that a scene takes MAX_SHADOWS = MAX_LIGHTS of
// them, so this is how the effect comes apart - leave one caster and its
// shadow is a plain complement, leave none and the floor is flat again.
registerDebug("shadow", (args?: Record<string, unknown>) => {
  if (typeof args?.light === "number" && typeof args?.cast === "boolean") {
    let i = clamp(Math.round(args.light), 0, lights.length - 1)
    setLight(lights[i]!, { castShadow: args.cast })
    setCasters(lights.filter(l => l.castShadow).length)
  }
  flush()
  return { casting: lights.map(l => l.castShadow), tints: LIGHT_TINTS }
})

// Pixels per logical unit the scene renders at (0 follows the display): the
// quality/throughput knob, and the A/B for "is this pass fill-bound?".
registerDebug("renderScale", (args?: Record<string, unknown>) => {
  if (typeof args?.scale === "number") setRenderScale(clamp(args.scale, 0, 4))
  flush()
  return { scale: renderScale() }
})

render(() => <App />)
