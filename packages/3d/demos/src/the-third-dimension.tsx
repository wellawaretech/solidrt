// The Third Dimension - a GPU demo on @solidrt/3d.
//
// One scene - a torus knot and a checkered ground, each drawn by its own
// shaderMaterial over a backdrop shader - rendered three times into three
// panels: a large orbiting hero shot, a side view where the knot's weave
// reads, and a top-down view where the shadows cross. The two side panels
// are <View3d>s sharing the scene's geometry, materials, lights and shadow
// maps, so they cost another target and one entry per mesh, not a second
// scene. Per-frame JS stays constant however much the passes cover.
//
// Four casting spot lights: a triangle of pure red, green and blue turning
// on one Group, plus a neutral overhead key. Each owns a tile of the
// scene's shadow atlas (one depth texture, one pass), so a shadow is the
// COMPLEMENT of the light it blocks - where two cross, the third survives
// alone, and where all cross the floor falls to the ambient.
//
// Drag a panel to orbit it, wheel or pinch to zoom, click or tap to pause
// that one, space to pause all three. The debug commands at the bottom
// park the camera and the rig, and switch casters off, over MCP.
import {
  capabilities,
  createEffect,
  createInputMap,
  createMemo,
  createPointerFeed,
  createSignal,
  env,
  flush,
  onFrame,
  render,
  safeArea,
  untrack,
  windowSize,
} from "@solidrt/core"
import type { PointerEvent, PointerFeed, TextureId } from "@solidrt/core"
import { createDrawTarget, glsl, limits, setTargetSize } from "@solidrt/core/gpu"
import {
  Group,
  HemisphereLight,
  Mesh,
  OrbitCamera,
  orbitActions,
  orbitBindings,
  phong,
  plane,
  Scene,
  setTransform,
  shaderMaterial,
  SpotLight,
  torusKnot,
  useScene,
  View3d,
  layoutStride,
  BASE_FLOATS,
} from "@solidrt/3d"
import type { CameraUpdate, OrbitCameraHandle, OrbitPoseState, SceneNode, SpotShadowOptions, Vec3 } from "@solidrt/3d"
import { FRESNEL, LIT_VERTEX, SCENE } from "@solidrt/3d/glsl"
import { registerDebug } from "srt:dev"

const KNOT_P = 2
const KNOT_Q = 3
/** Where the knot stands, and the orbit camera's pivot. */
const KNOT_CENTER: Vec3 = [0, 1.4, 0]
const FLOOR_SIZE = 36 // world units across; GROUND_SURFACE interpolates it
const FOV = 0.85 // vertical field of view, radians (the scene camera speaks degrees)
// What every panel projects with; near and far bracket the floor's visible disc.
const CAMERA: CameraUpdate = { fov: (FOV * 180) / Math.PI, near: 0.1, far: 80 }
const ORBIT_PERIOD = 68 // seconds for one full revolution
const ORBIT_SPEED = (Math.PI * 2) / ORBIT_PERIOD // radians per second, every panel
const MIN_DISTANCE = 2.6
const MAX_DISTANCE = 14
// Lowest the eye may sit, world units. Eye height is target.y + distance *
// sin(elevation), so an elevation that sits comfortably high up close digs
// under the floor at full zoom-out: hence a floor on the EYE, turned back
// into an elevation clamp against the distance of the moment (the orbit
// camera's clampPose, below). The ground is one back-face-culled quad, so
// under it the picture simply vanishes.
const EYE_MIN_Y = 0.35

// Three spot lights evenly spaced in azimuth and tilted down by the same
// angle: a triangular rig, so no face of the knot falls to ambient alone.
// These are scene light NODES, so setLight/setTransform on one re-shades.
const LIGHT_ELEVATION = 0.62 // radians above the horizon
// Pure primaries, one per corner. All three cones overlap on the centre of
// the floor, so the tints sum back toward neutral there.
const LIGHT_TINTS: Vec3[] = [
  [1, 0, 0],
  [0, 1, 0],
  [0, 0, 1],
]
// The strength each light lands on the knot with. A spot attenuates by
// inverse-square distance, so the node intensity is this times LIGHT_DISTANCE^2.
const LIGHT_INTENSITY = 0.72
const LIGHT_PHASE = 0.6 // where the triangle starts, so no light aims down the opening azimuth
// How far back up its own ray each light NODE sits: a spot shines, and
// renders its shadow map, from the node's world position.
const LIGHT_DISTANCE = 10
const RIG_PERIOD = 120 // seconds per turn of the rig; slower than the orbit, so the two read as two
// The cone's half-angle: wide enough that the pools overlap and each covers
// the other lights' cast shadows, narrow enough that the map's texels stay
// near the caster and fragments outside the frustum skip their taps.
const SPOT_ANGLE = 28
const SPOT_PENUMBRA = 0.25 // outer fraction of the cone that fades to the rim
const SPOT_DISTANCE = 22 // falloff cutoff, and the shadow camera's far plane
// The overhead key: neutral, straight above the knot, lifting its upper
// surfaces out of the tinted triangle. Kept well below the tints so the RGB
// separation stays the picture; its shadow is the knot's contact patch.
const KEY_INTENSITY = 0.35
const KEY_ANGLE = 22 // narrower than the rig's: the key only needs the knot and its contact shadow
// Per-caster map resolution. A spot's perspective map spreads its texels
// over the whole cone footprint, so this is what the shadow edge costs.
// Four casters tile a 4096x4096 atlas; scene-shadows downscales uniformly
// where maxTextureSize cannot hold it.
const SHADOW_MAP = 2048
const SHADOW_NEAR = 2 // the caster sits LIGHT_DISTANCE up its own ray
// Along the receiving surface's own normal, the knob to reach for first. The
// depth pass culls FRONT faces, so a closed caster needs no depth bias too.
const SHADOW_NORMAL_BIAS = 0.02
const SHADOW: SpotShadowOptions = { mapSize: SHADOW_MAP, normalBias: SHADOW_NORMAL_BIAS, near: SHADOW_NEAR }
// The hero panel's share of the window's long axis; the other two split what
// is left. The rounding leftovers go to the small panels, so the three tile
// the window exactly whichever way it turns.
const SPLIT = 0.66
// Where the lower-right panel opens: all but straight down (1.55 is the
// library's own pole guard), far enough back to cover the shadow spread.
const TOP_DOWN_ELEVATION = 1.5
const TOP_DOWN_DISTANCE = 11.6
// The hero panel's opening azimuth. The side panel is placed RELATIVE to it:
// every panel sweeps at the same rate, so what stays visible between two of
// them is how far apart they sit, never where either points at t = 0.
const HERO_AZIMUTH = 0.9
// Where the upper-right panel opens, tuned on screen. The knot is a flat disc
// in the xz plane, so a side view is the only place its weave reads. The eye
// sits just below the knot's centre looking slightly up, which stands the
// knot clear of the floor instead of laying it into the checker.
const SIDE_OFFSET = -3.096 // radians round from the hero
const SIDE_AZIMUTH = HERO_AZIMUTH + SIDE_OFFSET
const SIDE_ELEVATION = -0.079
const SIDE_DISTANCE = 8.5
// How far the two right-hand panels may be pushed and pulled.
const PANEL_MIN_DISTANCE = 3.5
const PANEL_MAX_DISTANCE = 30
// A view mirrors the scene's meshes, not its backdrop, so this stands in for
// the backdrop shader's outer tone where the ground has faded away.
const VIEW_CLEAR: [number, number, number, number] = [0.02, 0.028, 0.048, 1]
// Floor under the display's own scale: on a 1x display nothing softens the
// polygon edges (targets have no MSAA), so render the scene larger than the
// box it is drawn into and let the texture filter resolve it.
const MIN_RENDER_SCALE = 1.5

// ------ Shaders ------
//
// GLSL ES 3.00. None declares `#version`: the runtime injects its own
// preamble - `fragColor` and `iResolution` for the fragment stages, plus
// `vUV` for the fragment-only backdrop. Every other uniform is part of
// @solidrt/3d's standard set, opt-in by declare-and-use. The knot runs on
// the stock lit vertex stage (LIT_VERTEX, the one `phong` compiles for
// the ground) and takes its lighting from the scene's real light nodes
// through SCENE, so no light direction is baked into the GLSL and moving
// a light node re-shades everything.

// The knot's own material terms, and its rim: a view-dependent term of
// the shading model added after the scene's shade, not a stand-in light.
const KNOT_SPECULAR = 0.45
const KNOT_SHININESS = 64.0
const RIM_POWER = 3.5
const RIM_STRENGTH = 0.30

/**
 * Knot fragment stage: the scene's Blinn-Phong shade (hemisphere, every
 * light with its shadow, through shadeBlinn) over a cosine palette swept
 * along the knot, plus a fresnel rim, ending in the scene's output
 * (exposure, tone mapping, encode) like every stock material.
 */
let KNOT_FRAGMENT = glsl`
  in vec3 vWorldPos;
  in vec3 vNormal;
  in vec2 vUv;

  ${SCENE}
  ${FRESNEL}

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
    vec3 n = normalize(vNormal);

    vec3 base = palette(vUv.x + 0.05 * sin(vUv.y * TAU * 3.0));
    // Faint bands around the tube, so the surface reads as a swept ring.
    float band = smoothstep(0.30, 0.50, abs(fract(vUv.x * 64.0) - 0.5));
    base *= mix(0.82, 1.0, band);

    // The knot draws into every map, so it also self-shadows: a stretch
    // of tube in the lee of another loses that light and keeps the rest,
    // which is where the colour separation gets its hardest edge.
    Surface s = surfaceOf(vec4(base, 1.0), n);
    s.specular = ${KNOT_SPECULAR.toFixed(2)};
    s.shininess = ${KNOT_SHININESS.toFixed(1)};
    vec3 rgb = shadeBlinn(s, vWorldPos);

    // Fresnel rim tinted by the surface's own color, so grazing angles
    // brighten the silhouette instead of bleaching it toward white.
    vec3 view = normalize(uCamPos - vWorldPos);
    rgb += mix(vec3(0.25, 0.45, 0.85), base, 0.5) * fresnel(n, view, ${RIM_POWER.toFixed(1)}) * ${RIM_STRENGTH.toFixed(2)};

    fragColor = sceneOutput(rgb, 1.0, vWorldPos);
  }
`

/**
 * Ground: the stock phong material with a `surface` function - the
 * material describes the surface and the package shades it with the same
 * lights, shadows and fog as the knot. Output is premultiplied alpha, the
 * ground fading to fully transparent at the rim, which is why it is
 * `transparent`.
 */
let GROUND_PRELUDE = glsl`
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
`

let GROUND_SURFACE = glsl`
  void surface(inout Surface s) {
    vec2 p = (vUv - 0.5) * ${FLOOR_SIZE.toFixed(1)};
    float fade = 1.0 - smoothstep(6.0, 17.0, length(p));
    // Albedo, not final colour: the scene's shade multiplies it.
    vec3 tile = mix(vec3(0.16, 0.18, 0.22), vec3(0.52, 0.55, 0.60), checker(p / TILE));
    // Premultiplied, alpha being the distance fade: the floor is a solid
    // surface that dissolves into the background at the rim, so a shadow
    // simply darkens the tiles rather than contributing coverage of its own.
    s.base = vec4(tile * fade, fade);
  }
`


/**
 * Backdrop: a static radial gradient with a touch of hash grain so the ramp
 * does not band. Handed to <Scene> as `background`, so it is the scene
 * pass's FIRST entry - an attributeless fullscreen triangle with depth off,
 * redrawn with the pass rather than cached in a texture of its own.
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
// Which lights cast, the triangle's three tints then the overhead key: the
// `shadow` debug command writes it, each <SpotLight castShadow> and the
// subtitle's count read it.
const KEY_LIGHT = LIGHT_TINTS.length // the key's index in `casting`
let [casting, setCasting] = createSignal<boolean[]>([...LIGHT_TINTS.map(() => true), true])
// Handed out by the <OrbitCamera ref>s as the app mounts; the debug
// commands below only run once it is up. One orbit camera per panel: the
// large one on the left, then the two on the right.
let orbit!: OrbitCameraHandle
let topOrbit!: OrbitCameraHandle
let bottomOrbit!: OrbitCameraHandle
// The light rig and where it has turned to. Module scope so the `rig` debug
// command can park it: the spin pauses with the orbit, so a parked pose and
// a parked rig together are one repeatable frame.
let rig!: SceneNode
let rigAngle = 0
// A panel as input sees it: its camera. Left-then-right order. `cam` is a
// getter because the handles above arrive after this array exists; nothing
// reads it before the app is up.
type PanelCamera = { readonly cam: OrbitCameraHandle }
let cameras: PanelCamera[] = [{ get cam() { return orbit } }, { get cam() { return topOrbit } }, { get cam() { return bottomOrbit } }]

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
// tap is local to the panel it lands on (see panelLeaf). The large view's
// signal is what the hint text reads, and what the light rig follows.
let setAllOrbiting = (orbiting: boolean) => {
  orbit.set({ orbiting })
  topOrbit.set({ orbiting })
  bottomOrbit.set({ orbiting })
}

// Hold a camera's eye above the floor (see the EYE_MIN_Y note): the
// elevation floor is a function of the distance, so it is a clampPose hook
// rather than a fixed minElevation - re-derived on every write, a wheel
// glide's frames included.
let aboveFloor = (pose: OrbitPoseState) => {
  let minElevation = Math.asin(clamp((EYE_MIN_Y - pose.target[1]) / pose.distance, -1, 1))
  return pose.elevation < minElevation ? { elevation: minElevation } : undefined
}

// The i-th rig light's local frame: evenly spaced in azimuth, tilted down
// by LIGHT_ELEVATION, and standing LIGHT_DISTANCE back up its own ray from
// the knot. Each casting light's shadow camera sits AT its node's world
// position (Three's rule), so a light at the origin would render its map
// from inside the caster and shadow nothing.
let rigLight = (i: number): { direction: Vec3; position: Vec3 } => {
  let azimuth = LIGHT_PHASE + (i / LIGHT_TINTS.length) * Math.PI * 2
  let horizontal = Math.cos(LIGHT_ELEVATION)
  let direction: Vec3 = [-horizontal * Math.cos(azimuth), -Math.sin(LIGHT_ELEVATION), -horizontal * Math.sin(azimuth)]
  let position: Vec3 = [
    KNOT_CENTER[0] - direction[0] * LIGHT_DISTANCE,
    KNOT_CENTER[1] - direction[1] * LIGHT_DISTANCE,
    KNOT_CENTER[2] - direction[2] * LIGHT_DISTANCE,
  ]
  return { direction, position }
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
  let vertexCount = (knotGeometry.vertices.byteLength + groundGeometry.vertices.byteLength) / layoutStride("base")
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
  // clamped to what the device can actually allocate. The hero panel is its
  // own target; the two side panels are tiles of ONE atlas target, stacked
  // top above bottom, so they cost one render pass between them, not two.
  let targetSize = createMemo(() => {
    let box = panels()
    let scale = renderScale() || Math.max(env.displayScale, MIN_RENDER_SCALE)
    let pixels = (b: { w: number; h: number }) => ({
      width: clamp(Math.round(b.w * scale), 1, limits.maxTextureSize),
      height: clamp(Math.round(b.h * scale), 1, limits.maxTextureSize),
    })
    let top = pixels(box.top)
    let bottom = pixels(box.bottom)
    let atlas = {
      width: Math.max(top.width, bottom.width),
      height: Math.min(top.height + bottom.height, limits.maxTextureSize),
    }
    return { main: pixels(box.main), top, bottom, atlas }
  })
  let initial = untrack(targetSize)

  // Transparent, so the scene draws the ground after the opaque knot with
  // depth writes off. Made once, here, not in a JSX prop: a material is
  // a pipeline, and a prop expression is re-read. The surface function
  // replaces the base, so the color is moot; no highlight on the floor.
  let groundMaterial = phong({ transparent: true, prelude: GROUND_PRELUDE, surface: GROUND_SURFACE, specular: 0 })
  let knotMaterial = shaderMaterial({ vertex: LIT_VERTEX, fragment: KNOT_FRAGMENT, label: "knot" })

  // Both side views render `into` this one target as tiles: one pass for the
  // pair, each panel's <d-texture> showing its tile through srcX/srcY. The id
  // stays stable across a resize, so the <d-texture src> bindings and the
  // owner-scoped auto-free keep working.
  let atlas = createDrawTarget(initial.atlas.width, initial.atlas.height, null, {
    depth: true,
    clearColor: VIEW_CLEAR,
    label: "side-atlas",
  })
  createEffect(targetSize, size => setTargetSize(atlas, size.atlas.width, size.atlas.height))
  // How many lights cast, for the subtitle.
  let casters = createMemo(() => casting().filter(Boolean).length)

  // One pointer feed and one input map per panel, in the panels' order:
  // the panel's drag rotates and its pinch and wheel zoom through the
  // orbit's standard bindings (a wheel notch glides, the control's own
  // damping). The panel leaves are detached d-textures with no layout
  // box, so each feed is told its panel's box to normalize a drag by (one
  // panel height sweeps the same angle in the small panels as in the hero).
  let boxes = [() => panels().main, () => panels().top, () => panels().bottom]
  let feeds = boxes.map(box => createPointerFeed({ layout: () => ({ width: box().w, height: box().h }) }))
  let maps = feeds.map(pointer => {
    let input = createInputMap(orbitActions)
    input.bind(orbitBindings({ pointer }))
    return input
  })

  // One panel's leaf, a detached box over the window showing `tile`: the
  // pointer feed's drag, pinch and wheel, and the tap that pauses this
  // panel. Which panel a gesture belongs to is
  // the ENGINE's answer - the runtime freezes each pointer's hit path at the
  // down and delivers every later event along it, so a drag that runs off
  // its panel keeps arriving here with no capture to arrange, and two
  // fingers on one panel both land on it, which is what makes a pinch.
  let panelLeaf = (
    panel: PanelCamera,
    pointer: PointerFeed,
    box: () => { x: number; y: number; w: number; h: number },
    tile: () => { src: TextureId; srcX?: number; srcY?: number; srcW?: number; srcH?: number },
  ) => {
    let orbit = pointer.handlers
    return (
      <d-texture
        {...tile()}
        {...box()}
        onPointerDown={(e: PointerEvent) => {
          tap = tap === null ? { id: e.pointerId, x: e.clientX, y: e.clientY, at: performance.now() } : null
          orbit.onPointerDown(e)
        }}
        onPointerMove={orbit.onPointerMove}
        onPointerUp={(e: PointerEvent) => {
          orbit.onPointerUp(e)
          if (tap === null || tap.id !== e.pointerId) return
          let moved = Math.hypot(e.clientX - tap.x, e.clientY - tap.y)
          let held = performance.now() - tap.at
          tap = null
          // A tap pauses THIS panel and nothing else - the other two keep
          // sweeping, so one view can be held still to study while the rest
          // of the window carries on. Space is the global form.
          if (moved < TAP_SLOP && held < TAP_MS) panel.cam.set({ orbiting: !panel.cam.orbiting() })
        }}
        onWheel={orbit.onWheel}
      />
    )
  }

  let last = 0
  onFrame(tick => {
    let now = tick / 1000
    // Clamp both ends: the runtime's tick counter resets across a hot reload,
    // which makes exactly one frame's delta hugely negative.
    let dt = clamp(now - last, 0, 0.1)
    last = now
    // The cameras run themselves: each <OrbitCamera> mounts its own loop
    // while its panel orbits or a wheel glide is in flight.
    // Turn the light rig, on the same pause as the orbit so a parked scene
    // is a still frame end to end and snapshots of one pose repeat. This
    // single write moves three lights, their direction slots and the shadow
    // camera; the map is then re-rendered for the frame, which is what
    // pausing buys back.
    if (orbit.orbiting()) {
      rigAngle = (rigAngle + (dt * Math.PI * 2) / RIG_PERIOD) % (Math.PI * 2)
      setTransform(rig, { rotation: [0, rigAngle, 0] })
    }
  })

  return (
    <window
      onKeyDown={e => {
        // Keyboard-only, and global: the large view leads, the small ones
        // take whatever it just became.
        if (e.code === "Space" || e.key === " ") setAllOrbiting(!orbit.orbiting())
      }}
    >
      {/* The target is the hero panel at device pixels, shown by the
          detached leaf `output` returns. */}
      <Scene
        width={targetSize().main.width}
        height={targetSize().main.height}
        label="scene"
        background={BACKDROP_FRAGMENT}
        camera={CAMERA}
        // The hero leaf takes the SCENE's pointer feed: its <OrbitCamera>
        // sits directly under the Scene and drives from that feed's map.
        pointer={feeds[0]}
        output={texture => panelLeaf(cameras[0]!, useScene().pointer!, () => panels().main, () => ({ src: texture }))}
      >
        <OrbitCamera
          input={maps[0]}
          target={KNOT_CENTER}
          azimuth={HERO_AZIMUTH}
          elevation={0.34}
          distance={5.4}
          minDistance={MIN_DISTANCE}
          maxDistance={MAX_DISTANCE}
          minElevation={-0.15}
          maxElevation={1.35}
          clampPose={aboveFloor}
          orbitSpeed={ORBIT_SPEED}
          ref={o => (orbit = o)}
        />
        {/* The hemisphere light is the floor under the rig: with three
            shadows crossing, the patch where all three lights are blocked
            would otherwise be pure black, and this is what it falls to
            instead. Kept LOW under the spot rig - the dimmer it is, the
            harder the pools and the complement shadows read against it. */}
        <HemisphereLight sky={[0.42, 0.48, 0.6]} ground={[0.14, 0.15, 0.19]} intensity={0.4} />
        {/* The three lights hang off ONE Group, so the slow turn in the
            frame loop stays a single setTransform however many corners the
            triangle grows. A parent rotation carries both halves of a
            light: the direction it SHINES (down and inward, which the scene
            negates into the uLightDir the shaders read) and the position
            its shadow camera is placed at. */}
        <Group ref={n => (rig = n)}>
          {LIGHT_TINTS.map((tint, i) => {
            let { direction, position } = rigLight(i)
            return (
              <SpotLight
                direction={direction}
                position={position}
                color={tint}
                // Inverse-square compensation: what LIGHT_INTENSITY means
                // at the knot, LIGHT_DISTANCE away (see the constant).
                intensity={LIGHT_INTENSITY * LIGHT_DISTANCE * LIGHT_DISTANCE}
                angle={SPOT_ANGLE}
                penumbra={SPOT_PENUMBRA}
                distance={SPOT_DISTANCE}
                castShadow={casting()[i]}
                shadow={SHADOW}
              />
            )
          })}
        </Group>
        {/* The overhead key hangs off the scene root, not the rig: aimed
            straight down its default [0, -1, 0] from directly above the
            knot, the rig's turn has nothing to carry for it. */}
        <SpotLight
          position={[KNOT_CENTER[0], KNOT_CENTER[1] + LIGHT_DISTANCE, KNOT_CENTER[2]]}
          color={[1, 1, 1]}
          intensity={KEY_INTENSITY * LIGHT_DISTANCE * LIGHT_DISTANCE}
          angle={KEY_ANGLE}
          penumbra={SPOT_PENUMBRA}
          distance={SPOT_DISTANCE}
          castShadow={casting()[KEY_LIGHT]}
          shadow={SHADOW}
        />
        <Mesh geometry={groundGeometry} material={groundMaterial} rotation={[-Math.PI / 2, 0, 0]} />
        {/* Only the knot casts. The ground is one single-sided quad and the
            depth pass culls front faces, so it would draw nothing into the
            map anyway - and its entry in the shadow view would cost a draw
            for nothing. */}
        <Mesh geometry={knotGeometry} material={knotMaterial} position={KNOT_CENTER} castShadow />
        {/* A view has the same setCamera as a scene, which is all an orbit
            camera ever touches. All three sweep at the same rate, so they
            hold their phase offsets and read as three cameras on one
            turntable. */}
        <View3d
          width={targetSize().top.width}
          height={targetSize().top.height}
          clearColor={VIEW_CLEAR}
          label="top-right"
          camera={CAMERA}
          into={atlas}
          // Inside a <View3d>, useScene() hands out the VIEW's feed, so the
          // <OrbitCamera> inside hears this panel's drags and not the hero's.
          pointer={feeds[1]}
          output={() =>
            panelLeaf(cameras[1]!, useScene().pointer!, () => panels().top, () => ({
              src: atlas,
              srcX: 0,
              srcY: 0,
              srcW: targetSize().top.width,
              srcH: targetSize().top.height,
            }))
          }
        >
          <OrbitCamera
            input={maps[1]}
            target={KNOT_CENTER}
            azimuth={SIDE_AZIMUTH}
            elevation={SIDE_ELEVATION}
            distance={SIDE_DISTANCE}
            minDistance={PANEL_MIN_DISTANCE}
            maxDistance={PANEL_MAX_DISTANCE}
            minElevation={-0.15}
            maxElevation={1.55}
            clampPose={aboveFloor}
            orbitSpeed={ORBIT_SPEED}
            ref={o => (topOrbit = o)}
          />
        </View3d>
        <View3d
          width={targetSize().bottom.width}
          height={targetSize().bottom.height}
          clearColor={VIEW_CLEAR}
          label="bottom-right"
          camera={CAMERA}
          into={atlas}
          y={targetSize().top.height}
          pointer={feeds[2]}
          output={() =>
            panelLeaf(cameras[2]!, useScene().pointer!, () => panels().bottom, () => ({
              src: atlas,
              srcX: 0,
              srcY: targetSize().top.height,
              srcW: targetSize().bottom.width,
              srcH: targetSize().bottom.height,
            }))
          }
        >
          <OrbitCamera
            input={maps[2]}
            target={KNOT_CENTER}
            azimuth={HERO_AZIMUTH}
            elevation={TOP_DOWN_ELEVATION}
            distance={TOP_DOWN_DISTANCE}
            minDistance={PANEL_MIN_DISTANCE}
            maxDistance={PANEL_MAX_DISTANCE}
            minElevation={-0.15}
            maxElevation={1.55}
            clampPose={aboveFloor}
            orbitSpeed={ORBIT_SPEED}
            ref={o => (bottomOrbit = o)}
          />
        </View3d>
      </Scene>
      <view
        // Decoration only: without this a drag over the title would hit the
        // overlay and never reach the texture leaf behind it.
        pointerEvents="none"
        gap={6}
        padding={28}
        paddingTop={safeArea().top + 28}
      >
        <text color="#eef4ff" fontSize={30} fontWeight={700}>
          The Third Dimension
        </text>
        <text color="#a9bcd6" fontSize={16} fontWeight={600}>
          {`(${KNOT_P},${KNOT_Q}) torus knot - ${triangles.toLocaleString()} triangles - ${
            casters() > 0 ? `${casters()} shadow map${casters() === 1 ? "" : "s"} - ` : ""
          }3 views of one scene`}
        </text>
      </view>
      <view
        // The hint pins to the WINDOW's bottom-left corner, not the hero
        // panel's: in portrait the small panels sit under the hero, and the
        // hint stays at the screen edge below them.
        position="absolute"
        left={28}
        bottom={safeArea().bottom + 28}
        pointerEvents="none"
      >
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
// the camera at an exact pose, then snapshot. The handle's set() pushes the
// pose to the target at once, so a park-then-snapshot sequence sees the
// parked pose. flush() applies the orbiting signal write before the command
// returns its result.
registerDebug("camera", (args?: Record<string, unknown>) => {
  // Which panel to park: "top" or "bottom" for the right-hand pair, the
  // large one otherwise. A pose is always one panel's; `orbiting` follows the
  // app's own split - named panel, that panel alone (a click), no panel, all
  // three (space). A parked pose is a snap: set() drops a wheel glide in
  // flight, which would otherwise slide it straight back off.
  let panel = args?.panel === "top" || args?.panel === "bottom" ? args.panel : "main"
  let cam = panel === "top" ? topOrbit : panel === "bottom" ? bottomOrbit : orbit
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

registerDebug("vertexLayout", () => ({ floatsPerVertex: BASE_FLOATS }))

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
// light casts by default (the bound is the MAX_SHADOW_MAPS slot budget, one
// slot per spot), so this is how the effect comes apart - leave one caster
// and its shadow is a plain complement, leave none and the floor is flat again.
// One signal write: the light's <SpotLight castShadow> prop and the
// subtitle's count both follow it.
registerDebug("shadow", (args?: Record<string, unknown>) => {
  if (typeof args?.light === "number" && typeof args?.cast === "boolean") {
    let i = clamp(Math.round(args.light), 0, casting().length - 1)
    let cast = args.cast
    setCasting(casting().map((on, j) => (j === i ? cast : on)))
  }
  flush()
  return { casting: casting(), tints: LIGHT_TINTS }
})

// Pixels per logical unit the scene renders at (0 follows the display): the
// quality/throughput knob, and the A/B for "is this pass fill-bound?".
registerDebug("renderScale", (args?: Record<string, unknown>) => {
  if (typeof args?.scale === "number") setRenderScale(clamp(args.scale, 0, 4))
  flush()
  return { scale: renderScale() }
})

render(() => <App />)
