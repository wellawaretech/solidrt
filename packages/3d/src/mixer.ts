// Clip playback for models: `createMixer(model)` plays the model's baked
// animation clips (`model.clips`) through the spatial core's clip
// players. Policy lives here at O(changes) - `play(name, { fadeMs })`
// crossfades by creating one player and writing fades on the others (the
// idle/walk/run/attack shape) - while sampling, blending and the TRS
// writes are native, once per frame, BEFORE the frame's JS. So playback
// needs no frame loop at all (there is no update() to call), and your
// onFrame is the post-animation hook: it reads freshly posed joints
// (getTransform) and may overwrite them (root-motion strips, skeleton
// copies), last write wins, palettes and uModel following at the frame's
// flush. A channel the active clips do not animate keeps the node's
// current pose. Playing requires the model to be IN A SCENE (players
// bind live arena nodes); removing it drops the players, and a re-added
// model plays again from play().
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
//
// A game wants the opposite of a viewer: the travel kept, but moved from
// the root joint onto the character. `rootMotion` does that through the
// core: every clip plays fully pinned (the root held at its first key on
// all three axes) while the core samples the AUTHORED root track at the
// player's time and hands the per-advance delta on - "apply" adds it to
// the model node itself (Unity's applyRootMotion), "report" leaves it in
// `rootDelta()` for a controller to spend (Godot's
// get_root_motion_position, the collision tier's move-and-slide).
//
// `sampleChannel` (./clip.ts) stays the pure JS sampling core (checks,
// custom drivers, bake-side resampling) - the native evaluator implements
// the same glTF contract.

import * as spatial from "flux:spatial"
import type { NodeId } from "flux:spatial"
import { on } from "srt:events"
import type { ModelChannel, ModelClip } from "./gltf.ts"
import { sampleChannel } from "./clip.ts"
import type { Vec3 } from "./math.ts"
import type { Model } from "./model.ts"

export type MixerPlayOptions = {
  /** Repeat until something else plays. Defaults to true; false plays
   * once, holds the final pose and fires onFinish. */
  loop?: boolean
  /** Playback rate, 1 = as authored. */
  speed?: number
  /** Crossfade: this clip fades in and every other active clip fades out
   * over this window. 0 (the default) switches instantly. */
  fadeMs?: number
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
}

export type Mixer = {
  /** (Re)start a clip by name from its beginning; an unknown name throws
   * listing what the model has. The model must be in a scene. */
  play(name: string, opts?: MixerPlayOptions): void
  /** Fade every active clip out (instantly with no fadeMs); the nodes
   * keep the last written pose. */
  stop(opts?: { fadeMs?: number }): void
  /** Names of the clips currently playing or fading in. */
  playing(): string[]
  /** Whether a clip's root track drifts (what `inPlace` defaults to). */
  travels(name: string): boolean
  /** The root travel accumulated since the last call (zero without
   * `rootMotion`): the translation in the model's local frame (its
   * current facing) and the yaw in radians (a turn about the up axis).
   * A controller reads it once per frame and moves the character by it;
   * under "apply" it is what the core already applied to the model node. */
  rootDelta(): { position: [number, number, number]; yaw: number }
  /** Fired once when a `loop: false` clip reaches its end; the pose
   * holds until another play(). A plain field, like node pointer
   * handlers. */
  onFinish?: (name: string) => void
  /** The model's clip names, in file order. */
  clips: string[]
}

// The packed-clip codes of flux:spatial's createClip layout.
const PATH_CODE = { position: 0, rotation: 1, scale: 2 } as const
const INTERP_CODE = { step: 0, linear: 1, cubic: 2 } as const

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

// Player end reports ("spatialClipEnd", emitted before each frame's JS)
// routed back to the owning mixer's action.
const PLAYER_ENDS = new Map<number, (reason: string) => void>()
on("spatialClipEnd", (event: { player: number; reason: string }) => {
  PLAYER_ENDS.get(event.player)?.(event.reason)
})
// Root-motion deltas ("spatialRootMotion", one per bound player per
// advance) routed to the owning mixer's accumulator.
const PLAYER_ROOTS = new Map<number, (x: number, y: number, z: number, yaw: number) => void>()
on("spatialRootMotion", (event: { player: number; x: number; y: number; z: number; yaw: number }) => {
  PLAYER_ROOTS.get(event.player)?.(event.x, event.y, event.z, event.yaw)
})

/** Pack channels into the core's createClip layout. */
function registerClip(duration: number, channels: ModelChannel[]): number {
  let meta = new Uint32Array(channels.length * 4)
  let timeCount = 0
  let valueCount = 0
  channels.forEach((c, i) => {
    // Target slot = channel index: the player's target table below is
    // built in the same order.
    meta[i * 4] = i
    meta[i * 4 + 1] = PATH_CODE[c.path]
    meta[i * 4 + 2] = INTERP_CODE[c.interpolation]
    meta[i * 4 + 3] = c.times.length
    timeCount += c.times.length
    valueCount += c.values.length
  })
  let times = new Float32Array(timeCount)
  let values = new Float32Array(valueCount)
  let t = 0
  let v = 0
  for (let c of channels) {
    times.set(c.times, t)
    t += c.times.length
    values.set(c.values, v)
    v += c.values.length
  }
  return spatial.createClip(duration, meta, times, values)
}

/** Register the clip's baked channels with the core once; the id is
 * cached on the clip (shared by every mixer) and freed by model.dispose. */
function coreClip(clip: ModelClip): number {
  if (clip._core !== undefined) return clip._core
  clip._core = registerClip(clip.duration, clip.channels)
  return clip._core
}

/** The clip's root position channel: the one on the topmost node any
 * position channel targets (the table is pre-order, so lowest index is
 * topmost), or null for a clip that moves no translation at all. */
function rootChannel(clip: ModelClip): ModelChannel | null {
  let root: ModelChannel | null = null
  for (let c of clip.channels) {
    if (c.path === "position" && (root === null || c.node < root.node)) root = c
  }
  return root
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
  let elements = c.path === "rotation" ? 4 : 3
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
  let elements = c.path === "rotation" ? 4 : 3
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
  let key = pin.axes + (pin.yaw ? "+yaw" : "") + ":" + height + ":" + up.join(",")
  let cached = clip._coreVariants?.get(key)
  if (cached !== undefined) return cached
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
  let id = registerClip(clip.duration, channels)
  ;(clip._coreVariants ??= new Map()).set(key, id)
  return id
}

// One playing clip: the JS bookkeeping over a core player.
type Action = { name: string; player: number; fadingOut: boolean; finished: boolean }

/**
 * A mixer over a model's clips. One mixer per model; make it where you
 * made the model. Playback is core-driven - nothing to call per frame.
 */
export function createMixer(model: Model, mixerOpts: MixerOptions = {}): Mixer {
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
  let extent = [model.bounds[3]! - model.bounds[0]!, model.bounds[4]! - model.bounds[1]!, model.bounds[5]! - model.bounds[2]!]
  let driftLimit = ROOT_DRIFT_RATIO * Math.abs(extent[0]! * up[0] + extent[1]! * up[1] + extent[2]! * up[2])
  let travelling = new Map<ModelClip, boolean>()
  let travels = (clip: ModelClip): boolean => {
    let known = travelling.get(clip)
    if (known !== undefined) return known
    let root = rootChannel(clip)
    let result = root !== null && rootDrift(root, up) > driftLimit
    travelling.set(clip, result)
    return result
  }
  // The core clip a play uses: as authored unless the clip is pinned
  // (asked for, or it travels) or the mixer names a root height; a pin
  // rebases onto the rest height when no rootHeight is given, a
  // rootHeight rebases every clip whether pinned or not. Under rootMotion
  // the root and its yaw are held - the travel goes to the model - with
  // the height held too unless it stays in the pose.
  let clipFor = (clip: ModelClip, opts: MixerPlayOptions): number => {
    let root = rootChannel(clip)
    if (root === null) return coreClip(clip)
    let pin: RootPin =
      rootMotion !== null
        ? { axes: rootMotion.vertical === "pose" ? "horizontal" : "all", yaw: true }
        : { axes: (opts.inPlace ?? travels(clip)) ? "horizontal" : "free", yaw: false }
    if (pin.axes === "free" && mixerOpts.rootHeight === undefined) return coreClip(clip)
    let entry = model.nodes[root.node]
    if (entry === undefined) throw new Error("createMixer: clip '" + clip.name + "' targets a missing node " + root.node)
    let rest = entry.node.position
    let restHeight = rest[0] * up[0] + rest[1] * up[1] + rest[2] * up[2]
    return rootVariant(clip, root, pin, mixerOpts.rootHeight ?? restHeight, up)
  }
  // Root motion, accumulated from the core's per-advance reports until
  // the app takes it.
  let rootAcc = { position: [0, 0, 0] as [number, number, number], yaw: 0 }
  // Channel targets resolve at play: clips index the model's node table,
  // and the arena ids are per scene-entry, so a re-added model binds
  // fresh ones.
  let targetsFor = (clip: ModelClip): NodeId[] =>
    clip.channels.map((c) => {
      let entry = model.nodes[c.node]
      if (entry === undefined) throw new Error("createMixer: clip '" + clip.name + "' targets a missing node " + c.node)
      let id = entry.node._node
      if (id === null) throw new Error("play: the model must be in a scene (add() it first) before clips can play")
      entry.node._native = true
      return id
    })

  let actions: Action[] = []
  let dropAction = (action: Action) => {
    PLAYER_ENDS.delete(action.player)
    PLAYER_ROOTS.delete(action.player)
    let i = actions.indexOf(action)
    if (i >= 0) actions.splice(i, 1)
  }
  let start = (clip: ModelClip, opts: MixerPlayOptions, weight: number, fade: number): void => {
    let player = spatial.createPlayer(clipFor(clip, opts), targetsFor(clip), opts.speed ?? 1, opts.loop ?? true, weight, fade)
    let action: Action = { name: clip.name, player, fadingOut: false, finished: false }
    PLAYER_ENDS.set(player, (reason) => {
      if (reason === "finished") {
        action.finished = true
        mixer.onFinish?.(action.name)
      } else {
        // Faded out, or the model left the scene / was disposed.
        dropAction(action)
      }
    })
    actions.push(action)
    let root = rootMotion !== null ? rootChannel(clip) : null
    if (root !== null && rootMotion !== null) {
      let anchor = rootMotion.mode === "apply" ? model._node : null
      if (anchor === null && rootMotion.mode === "apply") throw new Error("play: the model must be in a scene before root motion can move it")
      if (anchor !== null) model._native = true
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
  }

  let mixer: Mixer = {
    clips: model.clips.map((c) => c.name),
    play(name, opts = {}) {
      let clip = model.clips.find((c) => c.name === name)
      if (clip === undefined) {
        throw new Error("play: no clip '" + name + "' (the model has: " + (mixer.clips.join(", ") || "none") + ")")
      }
      let fadeMs = opts.fadeMs ?? 0
      if (fadeMs > 0) {
        for (let action of actions) {
          action.fadingOut = true
          spatial.setPlayer(action.player, { fade: -1000 / fadeMs })
        }
        start(clip, opts, 0, 1000 / fadeMs)
      } else {
        for (let action of [...actions]) {
          spatial.destroyPlayer(action.player)
          dropAction(action)
        }
        start(clip, opts, 1, 0)
      }
    },
    stop(opts = {}) {
      let fadeMs = opts.fadeMs ?? 0
      if (fadeMs > 0) {
        for (let action of actions) {
          action.fadingOut = true
          spatial.setPlayer(action.player, { fade: -1000 / fadeMs })
        }
      } else {
        for (let action of [...actions]) {
          spatial.destroyPlayer(action.player)
          dropAction(action)
        }
      }
    },
    playing() {
      return actions.filter((a) => !a.fadingOut).map((a) => a.name)
    },
    rootDelta() {
      let out = rootAcc
      rootAcc = { position: [0, 0, 0], yaw: 0 }
      return out
    },
    travels(name) {
      let clip = model.clips.find((c) => c.name === name)
      if (clip === undefined) {
        throw new Error("travels: no clip '" + name + "' (the model has: " + (mixer.clips.join(", ") || "none") + ")")
      }
      return travels(clip)
    },
  }
  return mixer
}
