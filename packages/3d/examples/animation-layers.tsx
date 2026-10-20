// Animation layers: one meshless biped with clips authored in code and a
// mixer whose rig composes them in the spatial core, zero JS per frame:
//   - the base plays a walk-run BLEND SPACE (createBlendSpace) in a sync
//     group: the speed glides between the two every few seconds and the
//     cycles stay in phase, so the feet land together at any mix
//   - "aim" plays on an upper-body OVERRIDE layer masked to the spine's
//     subtree, its weight faded in and out in core every few seconds:
//     the torso turns to the ball while the legs keep walking
//   - "flinch" fires as a ONE-SHOT on an ADDITIVE layer (its deltas
//     against its first frame) and fades out before its end
//   - a crouch replaces the gait now and then through an INERTIAL
//     transition (the pose carries its velocity into the new clip)
//   - the foot plants are MARKERS on both gaits: onMarker counts them;
//     a CURVE on the gaits drives a lookAt solver's weight, so the head
//     turns to the ball through the middle of each cycle
import { pct, render } from "@solidrt/core"
import { add, capsule, createBlendSpace, createGroup, createIk, createMesh, createMixer, createModel, DirectionalLight, HemisphereLight, PerspectiveCamera, Scene, setTransform, sphere, standard } from "@solidrt/3d"
import type { ClipBlendSpace, Mixer, Model, ModelChannel, ModelClip, ModelData, ModelNode, SceneNode } from "@solidrt/3d"

const SIZE = 720
/** Keys per second the clips are baked at. */
const KEYS_PER_SECOND = 30
/** The clips' periods, seconds. */
const WALK_SECONDS = 1.2
const RUN_SECONDS = 0.7
const CROUCH_SECONDS = 1.2
const AIM_SECONDS = 2
const FLINCH_SECONDS = 0.5
/** Leg swing amplitudes, radians, and the hips' bob, units. */
const WALK_SWING = 0.5
const RUN_SWING = 0.9
const WALK_BOB = 0.03
const RUN_BOB = 0.06
/** The crouch: the hips' drop and the knees' bend. */
const CROUCH_DROP = 0.35
const CROUCH_BEND = 1.1
/** The aim: the spine's twist toward +x and the arm's raise. */
const AIM_TWIST = 0.9
const AIM_RAISE = 1.4
/** The flinch: the spine's backward bend at its peak. */
const FLINCH_BEND = 0.6
const FADE_MS = 300
const FLINCH_FADE_OUT_MS = 120
/** The blend space's positions (walk at 1.5, run at 4). */
const WALK_SPEED = 1.5
const RUN_SPEED = 4
/** The script: how often the speed, the aim, the flinch and the crouch
 * change, ms. */
const SPEED_EVERY_MS = 2500
const AIM_EVERY_MS = 4000
const FLINCH_EVERY_MS = 3300
const CROUCH_EVERY_MS = 9000
const CROUCH_FOR_MS = 3000

function xRotation(a: number): [number, number, number, number] {
  return [Math.sin(a / 2), 0, 0, Math.cos(a / 2)]
}
function yRotation(a: number): [number, number, number, number] {
  return [0, Math.sin(a / 2), 0, Math.cos(a / 2)]
}
function zRotation(a: number): [number, number, number, number] {
  return [0, 0, Math.sin(a / 2), Math.cos(a / 2)]
}

/** A baked channel over `seconds`: `f(t)` per key. */
function baked(node: number, path: ModelChannel["path"], seconds: number, f: (t: number) => ArrayLike<number>): ModelChannel {
  let keys = Math.round(seconds * KEYS_PER_SECOND) + 1
  let elements = path === "rotation" ? 4 : 3
  let times = new Float32Array(keys)
  let values = new Float32Array(keys * elements)
  for (let k = 0; k < keys; k++) {
    let t = (k / (keys - 1)) * seconds
    times[k] = t
    values.set(f(t), k * elements)
  }
  return { node, path, interpolation: "linear", times, values }
}

/** The rig: index 0 the root, parents before children. */
function rigData(): ModelData {
  let nodes: ModelNode[] = []
  let index: Record<string, number> = {}
  let at = (name: string, parent: number | null, position: [number, number, number]) => {
    nodes.push({ name, parent, position, rotation: [0, 0, 0, 1], scale: [1, 1, 1] })
    index[name] = nodes.length - 1
    return nodes.length - 1
  }
  let root = at("root", null, [0, 0, 0])
  let hips = at("hips", root, [0, 1, 0])
  let spine = at("spine", hips, [0, 0.3, 0])
  let neck = at("neck", spine, [0, 0.3, 0])
  at("head", neck, [0, 0.15, 0])
  let shoulder = at("shoulder", neck, [0.3, -0.05, 0])
  let elbow = at("elbow", shoulder, [0, -0.3, 0])
  at("wrist", elbow, [0, -0.3, 0])
  for (let side of ["l", "r"]) {
    let hip = at(side + "hip", hips, [side === "l" ? -0.2 : 0.2, 0, 0])
    let knee = at(side + "knee", hip, [0, -0.5, 0])
    at(side + "ankle", knee, [0, -0.5, 0])
  }
  return { nodes, parts: [], skins: [], clips: clips(index), materials: [], images: [], bounds: new Float32Array([-1, -1, -1, 1, 2, 1]) }
}

/** A gait cycle: legs swinging in opposition, the hips bobbing twice per
 * cycle, the arm counter-swinging; the plants marked where each leg
 * swings furthest back; the head's turn curve peaking mid-cycle. */
function gait(i: Record<string, number>, name: string, seconds: number, swing: number, bob: number): ModelClip {
  let phase = (t: number) => (t / seconds) * 2 * Math.PI
  return {
    name,
    duration: seconds,
    channels: [
      baked(i.lhip!, "rotation", seconds, (t) => xRotation(Math.sin(phase(t)) * swing)),
      baked(i.rhip!, "rotation", seconds, (t) => xRotation(-Math.sin(phase(t)) * swing)),
      baked(i.lknee!, "rotation", seconds, (t) => xRotation(Math.max(0, -Math.cos(phase(t))) * swing)),
      baked(i.rknee!, "rotation", seconds, (t) => xRotation(Math.max(0, Math.cos(phase(t))) * swing)),
      baked(i.hips!, "position", seconds, (t) => [0, 1 + Math.abs(Math.sin(phase(t))) * bob, 0]),
      baked(i.shoulder!, "rotation", seconds, (t) => xRotation(-Math.sin(phase(t)) * swing * 0.5)),
      baked(i.spine!, "rotation", seconds, () => [0, 0, 0, 1]),
      baked(i.neck!, "rotation", seconds, () => [0, 0, 0, 1]),
    ],
    markers: [
      { time: seconds * 0.25, name: "L" },
      { time: seconds * 0.75, name: "R" },
    ],
    curves: [{ name: "headTurn", times: Float32Array.from([0, seconds * 0.5, seconds]), values: Float32Array.from([0, 1, 0]) }],
  }
}

function clips(i: Record<string, number>): ModelClip[] {
  let crouch: ModelClip = {
    name: "crouch",
    duration: CROUCH_SECONDS,
    channels: [
      baked(i.hips!, "position", CROUCH_SECONDS, (t) => [0, 1 - CROUCH_DROP + Math.sin((t / CROUCH_SECONDS) * 2 * Math.PI) * 0.01, 0]),
      baked(i.lhip!, "rotation", CROUCH_SECONDS, () => xRotation(-CROUCH_BEND / 2)),
      baked(i.rhip!, "rotation", CROUCH_SECONDS, () => xRotation(-CROUCH_BEND / 2)),
      baked(i.lknee!, "rotation", CROUCH_SECONDS, () => xRotation(CROUCH_BEND)),
      baked(i.rknee!, "rotation", CROUCH_SECONDS, () => xRotation(CROUCH_BEND)),
      baked(i.shoulder!, "rotation", CROUCH_SECONDS, () => [0, 0, 0, 1]),
      baked(i.spine!, "rotation", CROUCH_SECONDS, () => xRotation(0.3)),
      baked(i.neck!, "rotation", CROUCH_SECONDS, () => [0, 0, 0, 1]),
    ],
  }
  // The aim keys the legs too: the layer's mask is what keeps them
  // walking.
  let aim: ModelClip = {
    name: "aim",
    duration: AIM_SECONDS,
    channels: [
      baked(i.spine!, "rotation", AIM_SECONDS, (t) => yRotation(-AIM_TWIST + Math.sin((t / AIM_SECONDS) * 2 * Math.PI) * 0.1)),
      baked(i.neck!, "rotation", AIM_SECONDS, () => yRotation(-0.2)),
      baked(i.shoulder!, "rotation", AIM_SECONDS, () => zRotation(-AIM_RAISE)),
      baked(i.elbow!, "rotation", AIM_SECONDS, () => [0, 0, 0, 1]),
      baked(i.lhip!, "rotation", AIM_SECONDS, () => [0, 0, 0, 1]),
      baked(i.rhip!, "rotation", AIM_SECONDS, () => [0, 0, 0, 1]),
    ],
  }
  // The flinch, authored from rest: the spine snaps back and recovers.
  let flinch: ModelClip = {
    name: "flinch",
    duration: FLINCH_SECONDS,
    channels: [
      baked(i.spine!, "rotation", FLINCH_SECONDS, (t) => xRotation(-Math.sin((t / FLINCH_SECONDS) * Math.PI) * FLINCH_BEND)),
      baked(i.neck!, "rotation", FLINCH_SECONDS, (t) => xRotation(-Math.sin((t / FLINCH_SECONDS) * Math.PI) * FLINCH_BEND * 0.5)),
    ],
  }
  return [gait(i, "walk", WALK_SECONDS, WALK_SWING, WALK_BOB), gait(i, "run", RUN_SECONDS, RUN_SWING, RUN_BOB), crouch, aim, flinch]
}

function App() {
  let model: Model = createModel(rigData(), { label: "layers-rig" })
  let j = (name: string): SceneNode => model.nodes.find((n) => n.name === name)!.node

  let bone = standard({ color: [0.85, 0.6, 0.3] })
  let mark = standard({ color: [0.3, 0.6, 0.85] })
  for (let { name, node } of model.nodes) {
    if (name === "root") continue
    add(node, createMesh(sphere({ radius: name === "head" ? 0.1 : 0.035 }), name === "head" ? mark : bone))
  }
  let nose = createMesh(sphere({ radius: 0.03 }), bone)
  setTransform(nose, { position: [0, 0, 0.1] })
  add(j("head"), nose)
  for (let name of ["shoulder", "elbow", "lhip", "lknee", "rhip", "rknee"]) {
    let len = name.endsWith("hip") || name.endsWith("knee") ? 0.5 : 0.3
    let mesh = createMesh(capsule({ radius: 0.025, height: len - 0.05 }), bone)
    setTransform(mesh, { position: [0, -len / 2, 0] })
    add(j(name), mesh)
  }
  for (let name of ["hips", "spine"]) {
    let mesh = createMesh(capsule({ radius: 0.03, height: 0.25 }), bone)
    setTransform(mesh, { position: [0, 0.15, 0] })
    add(j(name), mesh)
  }
  let ball = createMesh(sphere({ radius: 0.06 }), standard({ color: [0.9, 0.2, 0.2] }))
  setTransform(ball, { position: [1.2, 1.8, 0.8] })

  /** The script over the mixer: every change is a handful of core
   * writes, nothing runs per frame. */
  let script = (mixer: Mixer) => {
    let gaits = (): ClipBlendSpace =>
      createBlendSpace(mixer, {
        samples: [{ clip: "walk", position: WALK_SPEED }, { clip: "run", position: RUN_SPEED }],
        sync: "markers",
        position: WALK_SPEED,
        fadeMs: FADE_MS,
      })
    let space: ClipBlendSpace | null = gaits()
    let plants = 0
    mixer.onMarker = (e) => {
      if (e.phase === "instant" && e.weight > 0.5) plants++
    }
    let speedStep = 0
    setInterval(() => {
      // The speed glides walk -> run -> walk.
      speedStep = (speedStep + 1) % 4
      let t = [0, 0.5, 1, 0.5][speedStep]!
      space?.set(WALK_SPEED + (RUN_SPEED - WALK_SPEED) * t)
    }, SPEED_EVERY_MS)
    let aiming = false
    setInterval(() => {
      aiming = !aiming
      mixer.setLayerWeight("aim", aiming ? 1 : 0, { fadeMs: FADE_MS })
    }, AIM_EVERY_MS)
    setInterval(() => {
      mixer.play("flinch", { layer: "hit", loop: false, fadeOutMs: FLINCH_FADE_OUT_MS })
    }, FLINCH_EVERY_MS)
    setInterval(() => {
      space?.dispose()
      space = null
      mixer.play("crouch", { blend: "inertial", fadeMs: FADE_MS })
      setTimeout(() => {
        mixer.stop()
        mixer.inertialize(FADE_MS)
        space = gaits()
      }, CROUCH_FOR_MS)
    }, CROUCH_EVERY_MS)
    setInterval(() => console.log("foot plants so far:", plants, "head turn:", mixer.curve("headTurn")?.toFixed(2)), SPEED_EVERY_MS)
  }

  return (
    <window title="animation-layers">
      <view width={pct(100)} height={pct(100)} designSize={[SIZE, SIZE]}>
        <Scene
          width={SIZE}
          height={SIZE}
          clearColor={[0.08, 0.08, 0.1, 1]}
          samples={4}
          label="layers"
          ref={(scene) => {
            let world = createGroup()
            add(scene.root, world)
            add(world, model)
            add(world, ball)
            let mixer = createMixer(model, {
              layers: {
                aim: { mode: "override", weight: 0, mask: { include: [{ node: "spine", subtree: true }] } },
                hit: { mode: "additive" },
              },
            })
            mixer.play("aim", { layer: "aim" })
            let ik = createIk(model, {
              solvers: [{ kind: "lookAt", bones: [{ node: j("neck"), share: 0.4 }, { node: j("head"), share: 0.6 }], target: { node: ball }, limit: { yaw: 1.2, pitch: 0.8 }, weight: 0 }],
            })
            ik.setWeight(0, { mixer, curve: "headTurn" })
            script(mixer)
          }}
        >
          <PerspectiveCamera fov={40} position={[1.6, 1.6, 3.4]} lookAt={[0, 1, 0]} />
          <HemisphereLight sky={[1, 1, 1]} ground={[0.3, 0.3, 0.35]} intensity={0.6} />
          <DirectionalLight position={[4, 12, 8]} intensity={1.4} />
        </Scene>
      </view>
    </window>
  )
}

render(() => <App />)
