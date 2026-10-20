// Clip playback for models: `createMixer(model)` plays the model's baked
// animation clips (`model.clips`) through the spatial core's RIG - the
// core's animation state for one model (Unity's Animator, Godot's
// AnimationMixer, Unreal's AnimInstance): the joint table, the rest pose,
// ordered LAYERS, the players on them and the sync groups. Policy lives
// here at O(changes) - `play(name, { fadeMs })` crossfades by creating one
// player and writing fades on the others (the idle/walk/run/attack shape),
// a mask swap is one write, a blend space a few weight writes - while
// sampling, blending and the TRS writes are native, once per frame,
// BEFORE the frame's JS. So playback needs no frame loop at all, and your
// onFrame is the post-animation hook: it reads freshly posed joints
// (getTransform) and may overwrite them, last write wins, palettes and
// uModel following at the frame's flush. Playing requires the model to be
// IN A SCENE (the rig binds live arena nodes); removing it drops the rig
// and its players, and a re-added model plays again from play().
//
// The blend, per joint and per layer in order: the base layer's players
// blend over the REST pose (their weights normalized when they sum to one
// or more; below one the rest shows through for the remainder, so a
// stop-with-fade fades to rest, and a player lacking a channel another
// player on its layer animates contributes rest for it - no snap when
// clips of different coverage crossfade); each higher "override" layer
// composes its own blend OVER the result by its weight times its mask's
// share per joint (an aim on the upper body over a walk); each "additive"
// layer adds its players' deltas (a flinch over whatever plays), the
// clips turned into deltas against a reference once, in core. A joint no
// player on any layer animates keeps its pose. A transition is a
// crossfade (two players sampled, fading past each other) or INERTIAL
// (Unreal's inertialization: the new clip at full weight at once, the
// offset from the previous pose decayed with its velocity on a quintic -
// one sample per frame, no cycle averaging, any discontinuity smoothed).
//
// Root motion: a locomotion clip translates its root, which is what a game
// consumes to move the character and what sends a viewer's model through
// the lens. Every animation system grows the switch (Unity's
// applyRootMotion, Godot's root_motion_track); here it is `inPlace`, per
// play, automatic by default: a clip whose root position track ends
// somewhere else than it began (NET DRIFT, not how far it wanders - a
// taunt roams and returns and must not be pinned or the slide moves into
// the feet) plays in place. The strip is baked, not hooked: the in-place
// form of a clip is a second core clip whose root x/z sit at the first
// key and whose y is rebased onto the root's rest height (the bob
// survives), so it costs nothing per frame and blends like any clip. A
// mixer `rootHeight` rebases every clip the same way without the pin,
// for exports whose clips ride at another baseline than the rest pose.
// A game wants the opposite of a viewer: the travel kept, but moved from
// the root joint onto the character. `rootMotion` does that through the
// core: every clip plays fully pinned while the core samples the AUTHORED
// root track at the player's time and hands the per-advance delta on,
// weighted by the player's EFFECTIVE weight (its own times its layer's
// times the root's mask share: a legs-only base keeps the travel, an
// upper-body override contributes none, an additive player never does) -
// "apply" adds it to the model node itself (Unity's applyRootMotion),
// "report" leaves it in `rootDelta()` for a controller to spend.
//
// `sampleChannel` (./clip.ts) stays the pure JS sampling core (checks,
// custom drivers, bake-side resampling) - the native evaluator implements
// the same glTF contract.

import * as spatial from "flux:spatial"
import type { MaskId, NodeId, PlayerId, RigId } from "flux:spatial"
import { on } from "sol:events"
import type { ModelChannel, ModelClip, ModelMarker } from "./gltf.ts"
import { channelElements, sampleChannel } from "./clip.ts"
import type { Vec3 } from "./math.ts"
import type { Mesh } from "./mesh.ts"
import type { Model } from "./model.ts"
import type { CurveBinding, SceneNode } from "./node.ts"
import type { Scene } from "./scene.ts"

export type BlendKind = "crossfade" | "inertial"

/** A joint of the mask: a rig node, its name, or either with `subtree`
 * to take every rig joint under it as well (the spine-up mask is one
 * entry). */
export type MaskEntry = SceneNode | string | { node: SceneNode | string; subtree?: boolean }

/** A bone mask (Unity's AvatarMask, Godot's filters, Unreal's blend
 * masks): `include` names the joints the layer or player affects (the
 * others get 0), `exclude` the ones it leaves alone (the others get 1);
 * with both, the included minus the excluded. `soft` gives joints a
 * share between the two (a 0.5 on the shoulder so the mask does not
 * crease, Unreal's per-bone blend weight). */
export type MaskSpec = {
  include?: MaskEntry[]
  exclude?: MaskEntry[]
  soft?: { node: SceneNode | string; share: number }[]
}

export type LayerOptions = {
  /** 0..1, ramped by setLayerWeight; default 1. */
  weight?: number
  /** "override" (default) composes the layer's blend over what is under
   * it; "additive" adds its players' deltas. */
  mode?: "override" | "additive"
  /** The joints the layer affects; absent = every joint. */
  mask?: MaskSpec
}

/** What an additive clip's deltas are taken against: "first" (the
 * clip's own first frame, Three's referenceFrame 0, Unity's default),
 * "rest" (the model's rest pose, Godot's Add nodes), or another clip at
 * a time. */
export type AdditiveReferenceSpec = "first" | "rest" | { clip: string; time?: number }

export type MixerPlayOptions = {
  /** Repeat until something else plays. Defaults to true; false plays
   * once, holds the final pose and fires onFinish. */
  loop?: boolean
  /** Playback rate, 1 = as authored. */
  speed?: number
  /** The transition's length: with "crossfade" (the default blend) the
   * named clip fades in and every other clip on its layer fades out over
   * this window; with "inertial" the clip starts at full weight and the
   * pose's offset decays over it. 0 (the default) switches instantly. */
  fadeMs?: number
  /** How the transition blends; see fadeMs. Inertial is the one for
   * locomotion (no averaging of two cycles, no foot slide). */
  blend?: BlendKind
  /** Start fading out this many ms before a `loop: false` clip ends, in
   * core, so a one-shot returns to what is under it without a timer
   * (Godot's OneShot fadeout, Unreal's montage blend-out); onFinish still
   * fires at the end. */
  fadeOutMs?: number
  /** The layer to play on (declared by `layer()`); the base when absent. */
  layer?: string
  /** A mask for this player alone, on top of its layer's. */
  mask?: MaskSpec
  /** The subclip, seconds: the player's time stays within [start, end],
   * loops within it, finishes at `end` (Three's subclip, Godot's
   * play_section). */
  start?: number
  end?: number
  /** Where to start, seconds (clamped to the subclip). Default 0. */
  time?: number
  /** The initial weight, default 1. With `exclusive: false` a blend
   * space drives it afterwards through setWeight. */
  weight?: number
  /** Whether the other clips on the layer fade out (default true). false
   * adds the clip alongside them, for blend spaces and sync groups. */
  exclusive?: boolean
  /** On an additive layer: what the clip's deltas are taken against
   * (default "first"). Ignored on an override layer. */
  additiveReference?: AdditiveReferenceSpec
  /** Strip the root's travel so the model stays put (its x/z pinned at the
   * clip's first key, its height rebased onto the rest pose). Unset picks
   * per clip: a root track with net drift plays in place, one without
   * plays as authored. */
  inPlace?: boolean
}

export type RootMotionMode = "apply" | "report"

export type MixerOptions = {
  /** Rebase EVERY clip's root height so its first key lands here, pinned
   * or not - for an export whose clips ride at a different baseline than
   * its rest pose. Unset, only in-place clips rebase, onto the root's
   * rest position. */
  rootHeight?: number
  /** Keep the clips' root travel and move it off the root joint: every
   * clip plays in place (its root held, its yaw held) and the authored
   * root tracks' per-frame delta goes to the model - "apply" moves and
   * turns the model node, "report" accumulates it in `rootDelta()` for
   * the app to apply. The object form adds `up` (the root's parent-space
   * up axis, default +y) and `vertical: "pose"` to keep the height in
   * the pose (only the horizontal travel is delivered - for a controller
   * that owns gravity; Unity's bake-into-pose Y). Unset, travelling
   * clips are simply pinned (`inPlace`); `inPlace` is ignored while this
   * is set. */
  rootMotion?: RootMotionMode | { mode: RootMotionMode; up?: Vec3; vertical?: "travel" | "pose" }
  /** Layers above the base, in order, by name (`layer()` adds more). */
  layers?: Record<string, LayerOptions>
}

/** A rig over any scene nodes, for clips authored in code: `nodes` are
 * the slots the clips' channels index (`ModelChannel.node`), `root` the
 * node whose time scale the rig runs at and whose scene membership it
 * follows, `rest` the pose the layers compose over, declared on the
 * nodes (default: a node's declared rest, else its pose when the rig
 * is made). */
export type RigSpec = {
  root: SceneNode
  nodes: SceneNode[]
  clips: ModelClip[]
  rest?: { position: Vec3; quaternion: [number, number, number, number]; scale: Vec3 }[]
}

/** A time marker on a clip (Unity's AnimationEvent, Unreal's notify,
 * Godot's marker). With a `duration` it is a window (Unreal's notify
 * state): `begin` on entering, `end` on leaving, and on a stop inside. */
export type Marker = ModelMarker

export type MarkerEvent = {
  clip: string
  name: string
  phase: "instant" | "begin" | "end"
  /** The marker's authored time, seconds. */
  time: number
  data?: unknown
  /** The player's effective weight (its own times its layer's) at the
   * crossing: threshold a footstep at 0.01 yourself. */
  weight: number
  layer: string
}

export type Mixer = {
  /** (Re)start a clip by name; an unknown name throws listing what the
   * model has. The model must be in a scene. */
  play(name: string, opts?: MixerPlayOptions): void
  /** Fade every active clip (on `layer`, or everywhere) out over fadeMs
   * (instantly without), to what is under its layer - rest for the
   * base. */
  stop(opts?: { fadeMs?: number; layer?: string }): void
  /** Names of the clips currently playing or fading in (on `layer`, or
   * everywhere). */
  playing(layer?: string): string[]
  /** Whether a clip's root track drifts (what `inPlace` defaults to). */
  travels(name: string): boolean
  /** The root travel accumulated since the last call (zero without
   * `rootMotion`): the translation in the model's local frame (its
   * current facing) and the yaw in radians (a turn about the up axis).
   * A controller reads it once per frame and moves the character by it;
   * under "apply" it is what the core already applied to the model node. */
  rootDelta(): { position: [number, number, number]; yaw: number }
  /** Declare a layer above the existing ones (ordered by first
   * declaration), or change a declared one's mode and mask. */
  layer(name: string, opts?: LayerOptions): void
  /** Ramp a layer's weight over fadeMs (default 0, a snap) of the model's
   * time, in core: switch an aim on and off without per-frame JS. The
   * base layer's weight fades toward rest. */
  setLayerWeight(name: string, weight: number, opts?: { fadeMs?: number }): void
  /** Drop a layer and whatever plays on it. */
  removeLayer(name: string): void
  /** Make the named clips a sync group (Unreal's): whenever they play
   * together the heaviest leads and the others follow its phase, so a
   * walk-to-run blend does not foot-slide. `sync: "markers"` aligns the
   * markers common to every clip (left and right foot plants) instead of
   * normalized time. One group per clip. */
  group(name: string, clips: string[], opts?: { sync?: "time" | "markers" }): void
  /** Dissolve a group. */
  ungroup(name: string): void
  /** Write a playing clip's weight (ramped over fadeMs, default 0): the
   * blend-space channel. */
  setWeight(name: string, weight: number, opts?: { fadeMs?: number }): void
  /** Replace a clip's markers (the file's or set here). */
  markers(name: string, markers: Marker[]): void
  /** Whether the playing clip is inside the named window now. */
  inside(name: string, marker: string): boolean
  /** A curve's value as of this frame's advance (the clips' float tracks,
   * blended like the pose), or undefined when nothing playing carries
   * it. */
  curve(name: string): number | undefined
  /** Route a curve into a scalar param of a mesh's material (every entry
   * the mesh draws with it: a clip drives an opacity or a glow with no
   * JS per frame), or into a scene-wide param (`scene.setParams`'s,
   * uExposure among them) with a Scene instead of a mesh. Written at
   * the flush whenever the value changed; kept across scene re-enters.
   * `unbindCurve` removes it. */
  bindCurve(name: string, target: Mesh | Scene, param: string): void
  unbindCurve(name: string, target: Mesh | Scene, param: string): void
  /** A playing clip's time, seconds (undefined when it is not playing). */
  time(name: string): number | undefined
  /** Scrub a playing clip to `time` (clamped to its subclip). Markers it
   * jumps over do not fire; a window it leaves ends, one it lands in
   * begins. Pair it with `inertialize` to hide the jump. */
  seek(name: string, time: number): void
  /** Change a playing clip's rate (Three's action.timeScale). */
  setSpeed(name: string, speed: number): void
  /** Start an inertial transition now over `ms`: whatever the pose does
   * next frame (a seek, a layer switched on, a group change) is
   * smoothed from where it is. play() with blend "inertial" does this
   * for you. */
  inertialize(ms: number): void
  /** Fired once when a `loop: false` clip reaches its end; the pose
   * holds until another play(). A plain field, like node pointer
   * handlers. */
  onFinish?: (name: string, layer: string) => void
  /** Fired on every wrap of a looping clip. */
  onLoop?: (name: string, layer: string) => void
  /** Fired per marker crossing, in time order, in the frame's JS after
   * the advance (like onFinish). */
  onMarker?: (event: MarkerEvent) => void
  /** Stop everything, drop the rig and stop following the model's scene
   * membership; a RigSpec mixer also frees the core clips it registered
   * (a model's are freed by its dispose). */
  dispose(): void
  /** The model's clip names, in file order. */
  clips: string[]
  /** @internal The core rig while the model is in a scene. */
  _rig: RigId | null
}

// The packed-clip codes of flux:spatial's createClip layout.
const PATH_CODE = { position: 0, rotation: 1, scale: 2, weights: 3, curve: 4 } as const
const INTERP_CODE = { step: 0, linear: 1, cubic: 2 } as const
// Meta words per channel in that layout.
const CLIP_META_WORDS = 5
// The base layer's name and index.
const BASE_LAYER = "base"

// Net root x/z drift, as a fraction of the model's rest height, above
// which a clip counts as travelling. Measured on a stock rig: run cycles
// net 3-4 heights, every idle and taunt under 0.03 - the two sit far apart
// on either side of this.
const ROOT_DRIFT_RATIO = 0.25
// Keys per second a cubic rotation track is resampled to before its yaw
// is held: a per-key twist is not a constant the Hermite tangents could
// carry, so the curve is flattened to linear keys at this rate first.
const YAW_HOLD_RESAMPLE_HZ = 60
// The default up axis: the turn axis and the height axis of a root.
const UP_Y: Vec3 = [0, 1, 0]

// Core events (emitted before each frame's JS) routed back to the owning
// mixer's action by player id.
const PLAYER_ENDS = new Map<number, (reason: string) => void>()
on("spatialClipEnd", (event: { player: number; reason: string }) => {
  PLAYER_ENDS.get(event.player)?.(event.reason)
})
const PLAYER_LOOPS = new Map<number, () => void>()
on("spatialClipLoop", (event: { player: number }) => {
  PLAYER_LOOPS.get(event.player)?.()
})
const PLAYER_MARKERS = new Map<number, (marker: number, phase: MarkerEvent["phase"], weight: number) => void>()
on("spatialClipMarker", (event: { player: number; marker: number; phase: MarkerEvent["phase"]; weight: number; layer: number }) => {
  PLAYER_MARKERS.get(event.player)?.(event.marker, event.phase, event.weight)
})
const PLAYER_ROOTS = new Map<number, (x: number, y: number, z: number, yaw: number) => void>()
on("spatialRootMotion", (event: { player: number; x: number; y: number; z: number; yaw: number }) => {
  PLAYER_ROOTS.get(event.player)?.(event.x, event.y, event.z, event.yaw)
})
const RIG_ENDS = new Map<number, () => void>()
on("spatialRigEnd", (event: { rig: number }) => {
  RIG_ENDS.get(event.rig)?.()
})

/** Pack channels and curves into the core's createClip layout. */
function registerClip(clip: ModelClip, channels: ModelChannel[]): number {
  let curves = clip.curves ?? []
  let count = channels.length + curves.length
  let meta = new Uint32Array(count * CLIP_META_WORDS)
  let timeCount = 0
  let valueCount = 0
  channels.forEach((c, i) => {
    // Target slot = the channel's node index: the rig's joint table is
    // the model's node table in order.
    let at = i * CLIP_META_WORDS
    meta[at] = c.node
    meta[at + 1] = PATH_CODE[c.path]
    meta[at + 2] = INTERP_CODE[c.interpolation]
    meta[at + 3] = c.times.length
    meta[at + 4] = channelElements(c)
    timeCount += c.times.length
    valueCount += c.values.length
  })
  curves.forEach((c, k) => {
    let at = (channels.length + k) * CLIP_META_WORDS
    meta[at] = k
    meta[at + 1] = PATH_CODE.curve
    meta[at + 2] = INTERP_CODE[c.interpolation ?? "linear"]
    meta[at + 3] = c.times.length
    meta[at + 4] = 1
    timeCount += c.times.length
    valueCount += c.values.length
  })
  let times = new Float32Array(timeCount)
  let values = new Float32Array(valueCount)
  let t = 0
  let v = 0
  for (let c of [...channels, ...curves]) {
    times.set(c.times, t)
    t += c.times.length
    values.set(c.values, v)
    v += c.values.length
  }
  let id = spatial.createClip(clip.duration, meta, times, values, curves.map((c) => c.name))
  applyMarkers(id, clip.markers ?? [])
  return id
}

/** Write a marker list to a core clip. */
function applyMarkers(id: number, markers: Marker[]): void {
  let sorted = sortedMarkers(markers)
  spatial.setClipMarkers(
    id,
    Float32Array.from(sorted, (m) => m.time),
    Float32Array.from(sorted, (m) => m.duration ?? 0),
    sorted.map((m) => m.name),
  )
}

/** Markers in the core's order (ascending time, stable), so an event's
 * index names the same marker here. */
function sortedMarkers(markers: Marker[]): Marker[] {
  return markers.slice().sort((a, b) => a.time - b.time)
}

/** Register the clip's baked channels with the core once; the id is
 * cached on the clip (shared by every mixer) and freed by model.dispose. */
function coreClip(clip: ModelClip): number {
  if (clip._core !== undefined) return clip._core
  clip._core = registerClip(clip, clip.channels)
  return clip._core
}

/** A derived core clip of `clip` under `key`, made once by `make`. */
function variant(clip: ModelClip, key: string, make: () => number): number {
  let cached = clip._coreVariants?.get(key)
  if (cached !== undefined) return cached
  let id = make()
  ;(clip._coreVariants ??= new Map()).set(key, id)
  return id
}

/** The clip's root position channel: the one on the topmost node any
 * position channel targets (the table is pre-order, so lowest index is
 * topmost), provided the clip animates something BENEATH that node too -
 * a clip translating one lone joint has no root, so a sliding prop or a
 * test track is never pinned as travel. Null for a clip with no root. */
function rootChannel(clip: ModelClip, entries: { node: SceneNode }[]): ModelChannel | null {
  let root: ModelChannel | null = null
  for (let c of clip.channels) {
    if (c.path === "position" && (root === null || c.node < root.node)) root = c
  }
  if (root === null) return null
  let top = entries[root.node]?.node
  if (top === undefined) return root
  let under = (n: SceneNode | undefined): boolean => {
    for (let p = n?.parent ?? null; p !== null; p = p.parent) if (p === top) return true
    return false
  }
  return clip.channels.some((c) => c.node !== root.node && under(entries[c.node]?.node)) ? root : null
}

/** The root's rotation channel, if the clip turns the root at all. */
function rootRotation(clip: ModelClip, root: ModelChannel): ModelChannel | null {
  return clip.channels.find((c) => c.node === root.node && c.path === "rotation") ?? null
}

/** A quaternion's twist about the unit axis `up`, in radians: the
 * swing-twist split, exact under any lean. */
function twistOf(x: number, y: number, z: number, w: number, up: Vec3): number {
  let along = x * up[0] + y * up[1] + z * up[2]
  let a = 2 * Math.atan2(along, w)
  if (a > Math.PI) a -= 2 * Math.PI
  else if (a < -Math.PI) a += 2 * Math.PI
  return a
}

/** The rotation of `angle` radians about the unit axis `up`. */
function axisQuat(up: Vec3, angle: number): [number, number, number, number] {
  let s = Math.sin(angle / 2)
  return [up[0] * s, up[1] * s, up[2] * s, Math.cos(angle / 2)]
}

/** A cubic channel flattened to linear keys at YAW_HOLD_RESAMPLE_HZ (a
 * linear or step channel is returned as is). */
function linearized(c: ModelChannel, duration: number): ModelChannel {
  if (c.interpolation !== "cubic") return c
  let elements = channelElements(c)
  let keys = Math.max(2, Math.ceil(duration * YAW_HOLD_RESAMPLE_HZ) + 1)
  let times = new Float32Array(keys)
  let values = new Float32Array(keys * elements)
  let out: number[] = []
  for (let k = 0; k < keys; k++) {
    let t = (k / (keys - 1)) * duration
    times[k] = t
    sampleChannel(c, t, out)
    for (let e = 0; e < elements; e++) values[k * elements + e] = out[e]!
  }
  return { ...c, interpolation: "linear", times, values }
}

/** The rotation channel with its twist about `up` held at the first
 * key's: each key is pre-multiplied by the twist that undoes its own
 * turn, so the lean and pitch of the pose survive and only the turn is
 * gone. Cubic tracks are linearized first. */
function holdYaw(rotation: ModelChannel, duration: number, up: Vec3): ModelChannel {
  let c = linearized(rotation, duration)
  let src = c.values
  let values = Float32Array.from(src)
  let yaw0 = twistOf(src[0]!, src[1]!, src[2]!, src[3]!, up)
  for (let k = 0; k < values.length; k += 4) {
    let x = src[k]!, y = src[k + 1]!, z = src[k + 2]!, w = src[k + 3]!
    let [ax, ay, az, aw] = axisQuat(up, yaw0 - twistOf(x, y, z, w, up))
    // a * q
    values[k] = aw * x + ax * w + ay * z - az * y
    values[k + 1] = aw * y - ax * z + ay * w + az * x
    values[k + 2] = aw * z + ax * y - ay * x + az * w
    values[k + 3] = aw * w - ax * x - ay * y - az * z
  }
  return { ...c, values }
}

// Floats per key and the value's offset within a key: a key's value sits
// at [in, value, out] for cubic keys; rotations are four floats.
function keyLayout(c: ModelChannel): { stride: number; mid: number } {
  let elements = channelElements(c)
  let cubic = c.interpolation === "cubic"
  return { stride: cubic ? elements * 3 : elements, mid: cubic ? elements : 0 }
}

/** Net horizontal travel of the root track (last key minus first, the
 * component along `up` removed). */
function rootDrift(c: ModelChannel, up: Vec3): number {
  let { stride, mid } = keyLayout(c)
  let v = c.values
  let last = v.length - stride + mid
  let d = [v[last]! - v[mid]!, v[last + 1]! - v[mid + 1]!, v[last + 2]! - v[mid + 2]!]
  let rise = d[0]! * up[0] + d[1]! * up[1] + d[2]! * up[2]
  return Math.hypot(d[0]! - rise * up[0], d[1]! - rise * up[1], d[2]! - rise * up[2])
}

// How a root variant treats the root's travel: "free" keeps it (height
// rebase only), "horizontal" holds the travel across `up` at the first
// key (the bob survives), "all" holds every axis; `yaw` holds the root's
// twist about `up` too (the rootMotion forms: the travel goes to the
// model).
type RootPin = { axes: "free" | "horizontal" | "all"; yaw: boolean }

/** A root-corrected form of the clip: the root position channel with its
 * height rebased so the first key lands on `height` along `up` and, per
 * `pin`, its travel held at the first key (cubic tangents projected the
 * same way); the root rotation channel with its yaw held when asked;
 * every other channel shared as is. Registered once per (pin, height,
 * up) and cached on the clip. */
function rootVariant(clip: ModelClip, root: ModelChannel, pin: RootPin, height: number, up: Vec3): number {
  let key = "root:" + pin.axes + (pin.yaw ? "+yaw" : "") + ":" + height + ":" + up.join(",")
  return variant(clip, key, () => {
    let { stride, mid } = keyLayout(root)
    let src = root.values
    let values = Float32Array.from(src)
    let first = [src[mid]!, src[mid + 1]!, src[mid + 2]!]
    let firstRise = first[0]! * up[0] + first[1]! * up[1] + first[2]! * up[2]
    let shift = height - firstRise
    // The part of a travel vector a pin keeps: all of it, only its rise
    // along up, or none.
    let kept = (d: number[]): number[] => {
      if (pin.axes === "free") return d
      if (pin.axes === "all") return [0, 0, 0]
      let rise = d[0]! * up[0] + d[1]! * up[1] + d[2]! * up[2]
      return [rise * up[0], rise * up[1], rise * up[2]]
    }
    for (let k = 0; k < values.length; k += stride) {
      let travel = kept([src[k + mid]! - first[0]!, src[k + mid + 1]! - first[1]!, src[k + mid + 2]! - first[2]!])
      for (let e = 0; e < 3; e++) values[k + mid + e] = first[e]! + travel[e]! + shift * up[e]!
      if (stride === 9) {
        let tin = kept([src[k]!, src[k + 1]!, src[k + 2]!])
        let tout = kept([src[k + 6]!, src[k + 7]!, src[k + 8]!])
        for (let e = 0; e < 3; e++) {
          values[k + e] = tin[e]!
          values[k + 6 + e] = tout[e]!
        }
      }
    }
    let rotation = pin.yaw ? rootRotation(clip, root) : null
    let channels = clip.channels.map((c) => (c === root ? { ...c, values } : c === rotation ? holdYaw(c, clip.duration, up) : c))
    return registerClip(clip, channels)
  })
}

// One playing clip: the JS bookkeeping over a core player.
type Action = {
  name: string
  clip: ModelClip
  layer: string
  player: PlayerId
  /** A per-play mask, freed with the player. */
  mask: MaskId | null
  fadingOut: boolean
  finished: boolean
}

type LayerState = {
  name: string
  mode: "override" | "additive"
  mask: MaskSpec | null
  weight: number
  /** The core layer index while bound. */
  index: number
  coreMask: MaskId | null
}

type GroupState = {
  name: string
  clips: Set<string>
  sync: "time" | "markers"
  /** The core group while members play. */
  id: number | null
}

/** What a mixer animates: a model, or a rig over any nodes. */
type Subject = {
  root: SceneNode
  entries: { name: string; node: SceneNode }[]
  clips: ModelClip[]
  /** The extent along up the travel classification measures against (0:
   * any net drift travels). */
  height: (up: Vec3) => number
  /** Whether the mixer frees the clips' core ids on dispose (a bare
   * rig's); a model's dispose frees its own. */
  ownsClips: boolean
}

function subjectOf(target: Model | RigSpec): Subject {
  if ("nodes" in target && "clips" in target && !("kind" in target)) {
    let spec = target as RigSpec
    if (spec.nodes.length === 0) throw new Error("createMixer: a rig needs at least one node")
    // A given rest pose becomes the nodes' declared rest (published at
    // every scene enter; now, when they are in one).
    spec.nodes.forEach((n, i) => {
      let r = spec.rest?.[i]
      if (r === undefined) return
      n._rest = Float32Array.from([...r.position, ...r.quaternion, ...r.scale])
      if (n._node !== null) spatial.setRest(n._node, n._rest)
    })
    return {
      root: spec.root,
      entries: spec.nodes.map((node, i) => ({ name: "node" + i, node })),
      clips: spec.clips,
      height: () => 0,
      ownsClips: true,
    }
  }
  let model = target as Model
  let extent = [model.bounds[3]! - model.bounds[0]!, model.bounds[4]! - model.bounds[1]!, model.bounds[5]! - model.bounds[2]!]
  return {
    root: model,
    entries: model.nodes,
    clips: model.clips,
    height: (up) => Math.abs(extent[0]! * up[0] + extent[1]! * up[1] + extent[2]! * up[2]),
    ownsClips: false,
  }
}

/**
 * A mixer over a model's clips (or over any nodes, with clips authored
 * in code: the RigSpec form). One mixer per model; make it where you
 * made the model. Playback is core-driven - nothing to call per frame.
 */
export function createMixer(target: Model | RigSpec, mixerOpts: MixerOptions = {}): Mixer {
  let subject = subjectOf(target)
  let rm = mixerOpts.rootMotion
  let rootMotion = rm === undefined ? null : typeof rm === "string" ? { mode: rm, up: UP_Y, vertical: "travel" as const } : { mode: rm.mode, up: rm.up ?? UP_Y, vertical: rm.vertical ?? ("travel" as const) }
  if (rootMotion !== null) {
    let len = Math.hypot(rootMotion.up[0], rootMotion.up[1], rootMotion.up[2])
    if (!(len > 0)) throw new Error("createMixer: rootMotion.up must not be zero")
    rootMotion.up = [rootMotion.up[0] / len, rootMotion.up[1] / len, rootMotion.up[2] / len]
  }
  let up = rootMotion?.up ?? UP_Y
  // Travelling is a property of the clip's baked data, classified once:
  // the horizontal drift against the model's extent along up.
  let driftLimit = ROOT_DRIFT_RATIO * subject.height(up)
  let travelling = new Map<ModelClip, boolean>()
  let travels = (clip: ModelClip): boolean => {
    let known = travelling.get(clip)
    if (known !== undefined) return known
    let root = rootChannel(clip, subject.entries)
    let result = root !== null && rootDrift(root, up) > driftLimit
    travelling.set(clip, result)
    return result
  }
  let clipNamed = (name: string, what: string): ModelClip => {
    let clip = subject.clips.find((c) => c.name === name)
    if (clip === undefined) {
      throw new Error(what + ": no clip '" + name + "' (the model has: " + (subject.clips.map((c) => c.name).join(", ") || "none") + ")")
    }
    return clip
  }
  // The core clip a play uses: as authored unless the clip is pinned
  // (asked for, or it travels) or the mixer names a root height; a pin
  // rebases onto the rest height when no rootHeight is given, a
  // rootHeight rebases every clip whether pinned or not. Under rootMotion
  // the root and its yaw are held - the travel goes to the model - with
  // the height held too unless it stays in the pose.
  let rootCorrected = (clip: ModelClip, opts: MixerPlayOptions, additive: boolean): number => {
    let root = rootChannel(clip, subject.entries)
    if (root === null) return coreClip(clip)
    // An additive clip's deltas carry no travel by construction (a hit
    // reaction's hop stays in the pose), so it is never pinned unless
    // asked; under rootMotion its root is held like every clip's.
    let pin: RootPin =
      rootMotion !== null
        ? { axes: rootMotion.vertical === "pose" ? "horizontal" : "all", yaw: true }
        : { axes: (opts.inPlace ?? (additive ? false : travels(clip))) ? "horizontal" : "free", yaw: false }
    if (pin.axes === "free" && mixerOpts.rootHeight === undefined) return coreClip(clip)
    let entry = subject.entries[root.node]
    if (entry === undefined) throw new Error("createMixer: clip '" + clip.name + "' targets a missing node " + root.node)
    let rest = entry.node.position
    let restHeight = rest[0] * up[0] + rest[1] * up[1] + rest[2] * up[2]
    return rootVariant(clip, root, pin, mixerOpts.rootHeight ?? restHeight, up)
  }
  // On an additive layer the clip plays as deltas against its reference,
  // derived once in core and cached beside the root variants.
  let additiveForm = (clip: ModelClip, base: number, reference: AdditiveReferenceSpec): number => {
    let rig = mixer._rig
    if (reference === "first") return variant(clip, "additive:first:" + base, () => spatial.deriveAdditiveClip(base, { kind: "first" }))
    if (reference === "rest") {
      if (rig === null) throw new Error("play: the model must be in a scene before a rest-referenced additive clip can be derived")
      return variant(clip, "additive:rest:" + base, () => spatial.deriveAdditiveClip(base, { kind: "rest", rig }))
    }
    let refClip = clipNamed(reference.clip, "play (additiveReference)")
    let time = reference.time ?? 0
    return variant(clip, "additive:clip:" + base + ":" + reference.clip + ":" + time, () => spatial.deriveAdditiveClip(base, { kind: "clip", clip: coreClip(refClip), time }))
  }
  // Root motion, accumulated from the core's per-advance reports until
  // the app takes it.
  let rootAcc = { position: [0, 0, 0] as [number, number, number], yaw: 0 }

  // Layers, by name, in order; the base first.
  let layers: LayerState[] = [{ name: BASE_LAYER, mode: "override", mask: null, weight: 1, index: 0, coreMask: null }]
  let layerNamed = (name: string, what: string): LayerState => {
    let layer = layers.find((l) => l.name === name)
    if (layer === undefined) throw new Error(what + ": no layer '" + name + "' (declared: " + layers.map((l) => l.name).join(", ") + ")")
    return layer
  }
  let groups: GroupState[] = []
  // Curves routed into params: a mesh's through its scene (which applies
  // them to the mesh's entries), a scene's own as a target param. The
  // scene-level ones re-apply at every bind (the rig is new each time).
  type CurveRoute = { binding: CurveBinding; target: Mesh | Scene }
  let curveRoutes: CurveRoute[] = []
  let isMesh = (target: Mesh | Scene): target is Mesh => "geometry" in target
  let applySceneCurve = (route: CurveRoute, on: boolean): void => {
    let rig = mixer._rig
    if (rig === null || isMesh(route.target)) return
    if (on) spatial.bindCurveParam(rig, route.binding.curve, route.target.texture, route.binding.param)
    else spatial.unbindCurveParam(rig, route.binding.curve, route.target.texture)
  }
  let groupOf = (clip: string): GroupState | undefined => groups.find((g) => g.clips.has(clip))
  let actions: Action[] = []

  // Masks: names and subtrees resolved to the rig's joints once per
  // declaration, to per-node shares the core keys on node ids.
  let nodeNamed = (ref: SceneNode | string, what: string): SceneNode => {
    if (typeof ref !== "string") return ref
    let entry = subject.entries.find((e) => e.name === ref)
    if (entry === undefined) throw new Error(what + ": no node '" + ref + "' in the model")
    return entry.node
  }
  let joints = new Set<SceneNode>(subject.entries.map((e) => e.node))
  let collect = (entry: MaskEntry, what: string, into: Set<SceneNode>): void => {
    let ref = typeof entry === "object" && entry !== null && "node" in entry && !("kind" in entry) ? (entry as { node: SceneNode | string; subtree?: boolean }) : { node: entry as SceneNode | string, subtree: false }
    let node = nodeNamed(ref.node, what)
    if (!joints.has(node)) throw new Error(what + ": the node is not a joint of this mixer")
    into.add(node)
    if (ref.subtree) {
      let walk = (n: SceneNode): void => {
        for (let c of n.children) {
          if (joints.has(c)) into.add(c)
          walk(c)
        }
      }
      walk(node)
    }
  }
  let resolveMask = (spec: MaskSpec, what: string): { default: number; shares: Map<SceneNode, number> } => {
    let shares = new Map<SceneNode, number>()
    let include = new Set<SceneNode>()
    let exclude = new Set<SceneNode>()
    for (let e of spec.include ?? []) collect(e, what + " include", include)
    for (let e of spec.exclude ?? []) collect(e, what + " exclude", exclude)
    let def = spec.include === undefined ? 1 : 0
    for (let n of include) shares.set(n, 1)
    for (let n of exclude) shares.set(n, 0)
    for (let s of spec.soft ?? []) {
      if (!(s.share >= 0 && s.share <= 1)) throw new Error(what + " soft: a share must be within 0..1")
      let node = nodeNamed(s.node, what + " soft")
      if (!joints.has(node)) throw new Error(what + " soft: the node is not a joint of this mixer")
      shares.set(node, s.share)
    }
    return { default: def, shares }
  }
  let bindMask = (spec: MaskSpec, what: string): MaskId => {
    let { default: def, shares } = resolveMask(spec, what)
    let nodes: NodeId[] = []
    let values: number[] = []
    for (let [node, share] of shares) {
      if (node._node === null) throw new Error(what + ": a masked joint is not in the scene")
      nodes.push(node._node)
      values.push(share)
    }
    return spatial.createMask(def, nodes, Float32Array.from(values))
  }

  // The rig: made when the model enters a scene (or at creation when it
  // is in one), dropped when it leaves; the layers and their masks are
  // re-declared with it, the players are not (a re-added model plays
  // again from play()).
  let mixer: Mixer
  let bind = (): void => {
    let clock = subject.root._node
    if (clock === null) return
    let ids: NodeId[] = []
    for (let e of subject.entries) {
      if (e.node._node === null) {
        console.warn("createMixer: a joint is no longer in the scene; the model does not animate")
        return
      }
      // The core poses the joint: its JS mirror is stale from here on.
      e.node._native = true
      ids.push(e.node._node)
    }
    let rig = spatial.createRig(clock, ids)
    mixer._rig = rig
    RIG_ENDS.set(rig, () => {
      // The clock or a joint died under the core: every player is gone.
      for (let action of actions.slice()) dropAction(action)
      unbind()
    })
    layers.forEach((layer, i) => {
      layer.coreMask = layer.mask === null ? null : bindMask(layer.mask, "layer '" + layer.name + "' mask")
      if (i === 0) {
        if (layer.weight !== 1) spatial.setLayer(rig, 0, { weight: layer.weight })
        if (layer.coreMask !== null) spatial.setLayer(rig, 0, { mask: layer.coreMask })
      } else {
        layer.index = spatial.createLayer(rig, { weight: layer.weight, mode: layer.mode, mask: layer.coreMask ?? undefined })
      }
    })
    for (let route of curveRoutes) {
      if (isMesh(route.target)) route.target._scene?._bindCurve(route.target, route.binding)
      else applySceneCurve(route, true)
    }
  }
  let unbind = (): void => {
    let rig = mixer._rig
    if (rig === null) return
    RIG_ENDS.delete(rig)
    mixer._rig = null
    for (let action of actions.slice()) dropAction(action)
    for (let g of groups) g.id = null
    for (let layer of layers) layer.coreMask = null
  }
  let binding = {
    enter: bind,
    leave() {
      let rig = mixer._rig
      if (rig !== null) {
        // The model leaves the scene: every player goes with the rig,
        // its open windows ending now.
        for (let action of actions.slice()) destroyNow(action)
        spatial.destroyRig(rig)
      }
      unbind()
    },
  }
  if (subject.root._bindings === null) subject.root._bindings = []
  subject.root._bindings.push(binding)

  /** Report a marker crossing of an action's clip to onMarker. */
  let markerEvent = (action: Action, index: number, phase: MarkerEvent["phase"], weight: number): void => {
    let marker = sortedMarkers(action.clip.markers ?? [])[index]
    if (marker === undefined) return
    let event: MarkerEvent = { clip: action.name, name: marker.name, phase, time: marker.time, weight, layer: action.layer }
    if (marker.data !== undefined) event.data = marker.data
    mixer.onMarker?.(event)
  }
  /** The window changes a setPlayer returned (`[marker, phase, ...]`),
   * reported at once. */
  const PHASES: MarkerEvent["phase"][] = ["instant", "begin", "end"]
  let reportChanges = (action: Action, changes: number[]): void => {
    if (changes.length === 0) return
    let weight = spatial.readPlayer(action.player)[1]! * (layers.find((l) => l.name === action.layer)?.weight ?? 1)
    for (let k = 0; k + 1 < changes.length; k += 2) markerEvent(action, changes[k]!, PHASES[changes[k + 1]!] ?? "instant", weight)
  }
  /** Destroy an action's player at once; the windows it was inside end
   * now. */
  let destroyNow = (action: Action): void => {
    let [, weight] = spatial.readPlayer(action.player)
    let layerWeight = layers.find((l) => l.name === action.layer)?.weight ?? 1
    let ended = spatial.destroyPlayer(action.player)
    for (let m of ended) markerEvent(action, m, "end", weight! * layerWeight)
    dropAction(action)
  }
  let dropAction = (action: Action): void => {
    PLAYER_ENDS.delete(action.player)
    PLAYER_LOOPS.delete(action.player)
    PLAYER_MARKERS.delete(action.player)
    PLAYER_ROOTS.delete(action.player)
    if (action.mask !== null && mixer._rig !== null) spatial.destroyMask(action.mask)
    action.mask = null
    let i = actions.indexOf(action)
    if (i >= 0) actions.splice(i, 1)
    syncGroup(groupOf(action.name))
  }
  // A group's core half follows the players of its clips that are live.
  let syncGroup = (group: GroupState | undefined): void => {
    if (group === undefined) return
    let rig = mixer._rig
    if (rig === null) return
    let members = actions.filter((a) => group.clips.has(a.name) && !a.fadingOut).map((a) => a.player)
    if (members.length === 0) {
      if (group.id !== null) spatial.destroyGroup(group.id)
      group.id = null
    } else if (group.id === null) {
      group.id = spatial.createGroup(rig, members, group.sync)
    } else {
      spatial.setGroup(group.id, members)
    }
  }
  let actionNamed = (name: string, what: string): Action => {
    let action = actions.find((a) => a.name === name && !a.fadingOut)
    if (action === undefined) throw new Error(what + ": clip '" + name + "' is not playing")
    return action
  }
  let requireRig = (what: string): RigId => {
    if (subject.root._node === null || mixer._rig === null) throw new Error(what + ": the model must be in a scene (add() it first) before clips can play")
    return mixer._rig
  }
  let weightsFor = (clip: ModelClip): void => {
    // A weights channel targets the node's weights register; its key
    // width must be the node's target count.
    for (let c of clip.channels) {
      if (c.path !== "weights") continue
      let entry = subject.entries[c.node]
      if (entry === undefined) throw new Error("play: clip '" + clip.name + "' targets a missing node " + c.node)
      let targets = entry.node._morph?.names.length ?? 0
      if (channelElements(c) !== targets) {
        throw new Error("play: clip '" + clip.name + "' has " + channelElements(c) + " weights per key for node '" + entry.name + "' with " + targets + " morph targets")
      }
    }
  }
  let fadeOthers = (layer: string, except: Action | null, fadeMs: number): void => {
    for (let action of actions.slice()) {
      if (action.layer !== layer || action === except) continue
      if (fadeMs > 0) {
        if (action.fadingOut) continue
        action.fadingOut = true
        spatial.setPlayer(action.player, { fade: -1000 / fadeMs })
        syncGroup(groupOf(action.name))
      } else destroyNow(action)
    }
  }
  let start = (clip: ModelClip, layer: LayerState, opts: MixerPlayOptions, weight: number, fade: number): Action => {
    let rig = requireRig("play")
    weightsFor(clip)
    let base = rootCorrected(clip, opts, layer.mode === "additive")
    let core = layer.mode === "additive" ? additiveForm(clip, base, opts.additiveReference ?? "first") : base
    let mask = opts.mask === undefined ? null : bindMask(opts.mask, "play '" + clip.name + "' mask")
    let window: [number, number] | undefined = undefined
    if (opts.start !== undefined || opts.end !== undefined) {
      let start = opts.start ?? 0
      let end = opts.end ?? clip.duration
      if (!(start >= 0 && end > start && end <= clip.duration)) throw new Error("play: '" + clip.name + "' start/end must satisfy 0 <= start < end <= " + clip.duration)
      window = [start, end]
    }
    if (opts.fadeOutMs !== undefined && !(opts.fadeOutMs >= 0)) throw new Error("play: fadeOutMs must be >= 0")
    let player = spatial.createPlayer(rig, core, {
      layer: layer.index,
      speed: opts.speed,
      loop: opts.loop,
      weight,
      fade,
      mask: mask ?? undefined,
      window,
      fadeOut: opts.fadeOutMs === undefined ? undefined : opts.fadeOutMs / 1000,
      time: opts.time,
    })
    let action: Action = { name: clip.name, clip, layer: layer.name, player, mask, fadingOut: false, finished: false }
    PLAYER_ENDS.set(player, (reason) => {
      if (reason === "finished") {
        action.finished = true
        mixer.onFinish?.(action.name, action.layer)
      } else {
        // Faded out, or the model left the scene / was disposed.
        dropAction(action)
      }
    })
    PLAYER_LOOPS.set(player, () => mixer.onLoop?.(action.name, action.layer))
    PLAYER_MARKERS.set(player, (index, phase, weight) => markerEvent(action, index, phase, weight))
    actions.push(action)
    let root = rootMotion !== null && layer.mode !== "additive" ? rootChannel(clip, subject.entries) : null
    if (root !== null && rootMotion !== null) {
      let anchor = rootMotion.mode === "apply" ? subject.root._node : null
      if (anchor !== null) subject.root._native = true
      let rotation = rootRotation(clip, root)
      let rotationIndex = rotation === null ? undefined : clip.channels.indexOf(rotation)
      spatial.bindRootMotion(player, coreClip(clip), clip.channels.indexOf(root), rotationIndex, anchor ?? undefined, {
        up: rootMotion.up,
        vertical: rootMotion.vertical === "travel",
      })
      PLAYER_ROOTS.set(player, (x, y, z, yaw) => {
        rootAcc.position[0] += x
        rootAcc.position[1] += y
        rootAcc.position[2] += z
        rootAcc.yaw += yaw
      })
    }
    syncGroup(groupOf(clip.name))
    return action
  }

  mixer = {
    _rig: null,
    clips: subject.clips.map((c) => c.name),
    play(name, opts = {}) {
      let clip = clipNamed(name, "play")
      let layer = layerNamed(opts.layer ?? BASE_LAYER, "play")
      let fadeMs = opts.fadeMs ?? 0
      let weight = opts.weight ?? 1
      if (!(weight >= 0 && weight <= 1)) throw new Error("play: weight must be within 0..1")
      let exclusive = opts.exclusive ?? true
      if ((opts.blend ?? "crossfade") === "inertial") {
        if (exclusive) fadeOthers(layer.name, null, 0)
        start(clip, layer, opts, weight, 0)
        if (fadeMs > 0) spatial.inertialize(requireRig("play"), fadeMs)
      } else if (fadeMs > 0) {
        if (exclusive) fadeOthers(layer.name, null, fadeMs)
        start(clip, layer, opts, 0, (weight * 1000) / fadeMs)
      } else {
        if (exclusive) fadeOthers(layer.name, null, 0)
        start(clip, layer, opts, weight, 0)
      }
    },
    stop(opts = {}) {
      let fadeMs = opts.fadeMs ?? 0
      let names = opts.layer === undefined ? layers.map((l) => l.name) : [layerNamed(opts.layer, "stop").name]
      for (let layer of names) fadeOthers(layer, null, fadeMs)
    },
    playing(layer) {
      return actions.filter((a) => !a.fadingOut && (layer === undefined || a.layer === layer)).map((a) => a.name)
    },
    rootDelta() {
      let out = rootAcc
      rootAcc = { position: [0, 0, 0], yaw: 0 }
      return out
    },
    travels(name) {
      return travels(clipNamed(name, "travels"))
    },
    layer(name, opts = {}) {
      let weight = opts.weight ?? 1
      if (!(weight >= 0 && weight <= 1)) throw new Error("layer: weight must be within 0..1")
      let existing = layers.find((l) => l.name === name)
      let mode = opts.mode ?? existing?.mode ?? "override"
      let mask = opts.mask === undefined ? (existing?.mask ?? null) : opts.mask
      if (existing !== undefined) {
        if (existing.name === BASE_LAYER && mode !== "override") throw new Error("layer: the base layer is an override layer")
        if (existing.mode !== mode) throw new Error("layer: '" + name + "' is " + existing.mode + "; remove it to change its mode")
        existing.mask = mask
        existing.weight = weight
        let rig = mixer._rig
        if (rig !== null) {
          let old = existing.coreMask
          existing.coreMask = mask === null ? null : bindMask(mask, "layer '" + name + "' mask")
          spatial.setLayer(rig, existing.index, { weight, mask: existing.coreMask })
          if (old !== null) spatial.destroyMask(old)
        }
        return
      }
      if (mode !== "override" && mode !== "additive") throw new Error("layer: mode must be override or additive")
      let layer: LayerState = { name, mode, mask, weight, index: layers.length, coreMask: null }
      layers.push(layer)
      let rig = mixer._rig
      if (rig !== null) {
        layer.coreMask = mask === null ? null : bindMask(mask, "layer '" + name + "' mask")
        layer.index = spatial.createLayer(rig, { weight, mode, mask: layer.coreMask ?? undefined })
      }
    },
    setLayerWeight(name, weight, opts = {}) {
      if (!(weight >= 0 && weight <= 1)) throw new Error("setLayerWeight: weight must be within 0..1")
      let layer = layerNamed(name, "setLayerWeight")
      layer.weight = weight
      let rig = mixer._rig
      if (rig !== null) spatial.setLayer(rig, layer.index, { weight, fadeMs: opts.fadeMs ?? 0 })
    },
    removeLayer(name) {
      if (name === BASE_LAYER) throw new Error("removeLayer: the base layer stays")
      let layer = layerNamed(name, "removeLayer")
      for (let action of actions.slice()) if (action.layer === name) dropAction(action)
      let rig = mixer._rig
      if (rig !== null) {
        spatial.destroyLayer(rig, layer.index)
        if (layer.coreMask !== null) spatial.destroyMask(layer.coreMask)
      }
      let at = layers.indexOf(layer)
      layers.splice(at, 1)
      for (let l of layers) if (l.index > layer.index) l.index -= 1
    },
    group(name, clips, opts = {}) {
      for (let c of clips) clipNamed(c, "group")
      let sync = opts.sync ?? "time"
      let existing = groups.find((g) => g.name === name)
      for (let g of groups) if (g !== existing) for (let c of clips) if (g.clips.has(c)) throw new Error("group: clip '" + c + "' is already in group '" + g.name + "'")
      if (existing !== undefined) {
        if (existing.id !== null) spatial.destroyGroup(existing.id)
        existing.id = null
        existing.clips = new Set(clips)
        existing.sync = sync
        syncGroup(existing)
        return
      }
      let group: GroupState = { name, clips: new Set(clips), sync, id: null }
      groups.push(group)
      syncGroup(group)
    },
    ungroup(name) {
      let at = groups.findIndex((g) => g.name === name)
      if (at < 0) throw new Error("ungroup: no group '" + name + "'")
      let group = groups[at]!
      if (group.id !== null) spatial.destroyGroup(group.id)
      groups.splice(at, 1)
    },
    setWeight(name, weight, opts = {}) {
      if (!(weight >= 0 && weight <= 1)) throw new Error("setWeight: weight must be within 0..1")
      let action = actionNamed(name, "setWeight")
      let fadeMs = opts.fadeMs ?? 0
      if (fadeMs > 0) {
        let current = spatial.readPlayer(action.player)[1]!
        spatial.setPlayer(action.player, { fade: ((weight - current) * 1000) / fadeMs })
      } else spatial.setPlayer(action.player, { weight, fade: 0 })
    },
    markers(name, markers) {
      let clip = clipNamed(name, "markers")
      for (let m of markers) {
        if (!(m.time >= 0 && Number.isFinite(m.time)) || (m.duration !== undefined && !(m.duration >= 0))) throw new Error("markers: '" + m.name + "' needs a finite time >= 0 and a duration >= 0")
      }
      clip.markers = markers.slice()
      if (clip._core !== undefined) applyMarkers(clip._core, clip.markers)
      if (clip._coreVariants !== undefined) for (let id of clip._coreVariants.values()) applyMarkers(id, clip.markers)
    },
    inside(name, marker) {
      let action = actionNamed(name, "inside")
      let sorted = sortedMarkers(action.clip.markers ?? [])
      let at = sorted.findIndex((m) => m.name === marker && (m.duration ?? 0) > 0)
      if (at < 0) throw new Error("inside: clip '" + name + "' has no window '" + marker + "'")
      return spatial.markerInside(action.player, at)
    },
    curve(name) {
      let rig = mixer._rig
      return rig === null ? undefined : spatial.readCurve(rig, name)
    },
    bindCurve(name, target, param) {
      if (curveRoutes.some((r) => r.target === target && r.binding.curve === name && r.binding.param === param)) return
      let route: CurveRoute = { binding: { mixer, curve: name, param }, target }
      curveRoutes.push(route)
      if (isMesh(target)) target._scene?._bindCurve(target, route.binding)
      else applySceneCurve(route, true)
    },
    unbindCurve(name, target, param) {
      let at = curveRoutes.findIndex((r) => r.target === target && r.binding.curve === name && r.binding.param === param)
      if (at < 0) return
      let route = curveRoutes[at]!
      curveRoutes.splice(at, 1)
      if (isMesh(target)) target._scene?._unbindCurve(target, route.binding)
      else applySceneCurve(route, false)
    },
    time(name) {
      let action = actions.find((a) => a.name === name && !a.fadingOut)
      return action === undefined ? undefined : spatial.readPlayer(action.player)[0]
    },
    seek(name, time) {
      let action = actionNamed(name, "seek")
      reportChanges(action, spatial.setPlayer(action.player, { time }))
    },
    setSpeed(name, speed) {
      spatial.setPlayer(actionNamed(name, "setSpeed").player, { speed })
    },
    inertialize(ms) {
      spatial.inertialize(requireRig("inertialize"), ms)
    },
    dispose() {
      binding.leave()
      let list = subject.root._bindings
      if (list !== null) {
        let i = list.indexOf(binding)
        if (i >= 0) list.splice(i, 1)
      }
      if (subject.ownsClips) {
        for (let clip of subject.clips) {
          if (clip._core !== undefined) spatial.destroyClip(clip._core)
          clip._core = undefined
          if (clip._coreVariants !== undefined) for (let id of clip._coreVariants.values()) spatial.destroyClip(id)
          clip._coreVariants = undefined
        }
      }
    },
  }
  for (let [name, opts] of Object.entries(mixerOpts.layers ?? {})) mixer.layer(name, opts)
  bind()
  return mixer
}
