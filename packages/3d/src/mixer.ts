// Clip playback for models: `createMixer(model)` plays the model's baked
// animation clips (`model.clips`) by writing node TRS through
// setTransform. The orbit-camera pattern: the mixer registers no frame
// loop - the app calls `mixer.update(dt)` from its own onFrame and uses
// the boolean return to gate dependents. `play(name, { fadeMs })`
// crossfades: the named clip fades in while every other active clip fades
// out over the same window, which is the idle/walk/run/attack shape.
//
// This is the JS tier of animation (okf/backlog/animation-core.md is the
// native evaluator that replaces these internals, not this API): sampling
// is O(animated channels) interpreted work per update - fine for a
// handful of characters, not for a crowd. A channel the active clips do
// not animate keeps the node's current pose (the file's rest pose, or
// whatever the app wrote); the mixer's writes and the app's setTransform
// go through the same path, last write wins.
//
// `sampleChannel` (./clip.ts) is the pure core (no scene, runs under
// bun), exported from the package root for checks and custom drivers.

import { quatSlerp } from "./math.ts"
import type { Quat, Vec3 } from "./math.ts"
import type { ModelClip } from "./gltf.ts"
import { sampleChannel } from "./clip.ts"
import { setTransform } from "./node.ts"
import type { SceneNode } from "./node.ts"
import { updateSkins } from "./model.ts"
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
}

export type Mixer = {
  /** (Re)start a clip by name from its beginning; an unknown name throws
   * listing what the model has. */
  play(name: string, opts?: MixerPlayOptions): void
  /** Fade every active clip out (instantly with no fadeMs); the nodes
   * keep the last written pose. */
  stop(opts?: { fadeMs?: number }): void
  /** Advance by `dt` seconds and write the blended pose. Call from your
   * onFrame; returns true while clips are active (poses were written). */
  update(dt: number): boolean
  /** Names of the clips currently playing or fading in. */
  playing(): string[]
  /** Fired once when a `loop: false` clip reaches its end; the pose
   * holds until another play(). A plain field, like node pointer
   * handlers. */
  onFinish?: (name: string) => void
  /** The model's clip names, in file order. */
  clips: string[]
}

// One playing clip. `fade` is the weight change per second: positive
// fading in, negative fading out, 0 steady.
type Action = {
  clip: ModelClip
  targets: SceneNode[]
  time: number
  speed: number
  loop: boolean
  weight: number
  fade: number
  finished: boolean
}

// A node's blended pose this update; `has` bits say which paths any
// active channel wrote (1 position, 2 rotation, 4 scale) and `sum` the
// per-path accumulated weight for the running weighted average.
type Slot = {
  node: SceneNode
  has: number
  sum: [number, number, number]
  position: Vec3
  rotation: Quat
  scale: Vec3
}

const PATH_INDEX = { position: 0, rotation: 1, scale: 2 } as const
const PATH_BIT = [1, 2, 4]

/**
 * A mixer over a model's clips. One mixer per model; make it where you
 * made the model and drive it from the same onFrame that steps your
 * scene.
 */
export function createMixer(model: Model): Mixer {
  // Channel targets resolve once: clips index the model's node table.
  let targetsFor = (clip: ModelClip): SceneNode[] =>
    clip.channels.map((c) => {
      let entry = model.nodes[c.node]
      if (entry === undefined) throw new Error("createMixer: clip '" + clip.name + "' targets a missing node " + c.node)
      return entry.node
    })

  let actions: Action[] = []
  let slots = new Map<number, Slot>()
  let sample: number[] = [0, 0, 0, 0]

  let mixer: Mixer = {
    clips: model.clips.map((c) => c.name),
    play(name, opts = {}) {
      let clip = model.clips.find((c) => c.name === name)
      if (clip === undefined) {
        throw new Error("play: no clip '" + name + "' (the model has: " + (mixer.clips.join(", ") || "none") + ")")
      }
      let fadeMs = opts.fadeMs ?? 0
      if (fadeMs > 0) {
        for (let action of actions) action.fade = -1000 / fadeMs
        actions.push({ clip, targets: targetsFor(clip), time: 0, speed: opts.speed ?? 1, loop: opts.loop ?? true, weight: 0, fade: 1000 / fadeMs, finished: false })
      } else {
        actions.length = 0
        actions.push({ clip, targets: targetsFor(clip), time: 0, speed: opts.speed ?? 1, loop: opts.loop ?? true, weight: 1, fade: 0, finished: false })
      }
    },
    stop(opts = {}) {
      let fadeMs = opts.fadeMs ?? 0
      if (fadeMs > 0) for (let action of actions) action.fade = -1000 / fadeMs
      else actions.length = 0
    },
    update(dt) {
      if (actions.length === 0) return false
      // Advance clocks and fades; drop actions that faded out.
      for (let i = actions.length - 1; i >= 0; i--) {
        let action = actions[i]!
        if (action.fade !== 0) {
          action.weight += action.fade * dt
          if (action.weight >= 1) {
            action.weight = 1
            action.fade = 0
          } else if (action.weight <= 0) {
            actions.splice(i, 1)
            continue
          }
        }
        let duration = action.clip.duration
        action.time += dt * action.speed
        if (duration <= 0) action.time = 0
        else if (action.loop) action.time = ((action.time % duration) + duration) % duration
        else if (action.time >= duration) {
          action.time = duration
          if (!action.finished) {
            action.finished = true
            mixer.onFinish?.(action.clip.name)
          }
        } else if (action.time < 0) action.time = 0
      }
      if (actions.length === 0) return false

      // Blend: per (node, path), the weighted average over the actions
      // that animate it (incremental - each contributor slerps/lerps in
      // by its share of the accumulated weight).
      for (let slot of slots.values()) slot.has = 0
      for (let action of actions) {
        let channels = action.clip.channels
        for (let i = 0; i < channels.length; i++) {
          let channel = channels[i]!
          sampleChannel(channel, action.time, sample)
          let slot = slots.get(channel.node)
          if (slot === undefined) {
            slot = {
              node: action.targets[i]!,
              has: 0,
              sum: [0, 0, 0],
              position: [0, 0, 0],
              rotation: [0, 0, 0, 1],
              scale: [1, 1, 1],
            }
            slots.set(channel.node, slot)
          }
          let path = PATH_INDEX[channel.path]
          let value = path === 0 ? slot.position : path === 1 ? slot.rotation : slot.scale
          if ((slot.has & PATH_BIT[path]!) === 0) {
            slot.has |= PATH_BIT[path]!
            slot.sum[path] = action.weight
            for (let e = 0; e < value.length; e++) value[e] = sample[e]!
          } else {
            let total = slot.sum[path] + action.weight
            let share = total > 0 ? action.weight / total : 0
            slot.sum[path] = total
            if (path === 1) quatSlerp(value as Quat, value as Quat, readQuatSample(sample), share)
            else for (let e = 0; e < 3; e++) value[e] = value[e]! + (sample[e]! - value[e]!) * share
          }
        }
      }
      for (let slot of slots.values()) {
        if (slot.has === 0) continue
        setTransform(slot.node, {
          position: slot.has & 1 ? slot.position : undefined,
          quaternion: slot.has & 2 ? slot.rotation : undefined,
          scale: slot.has & 4 ? slot.scale : undefined,
        })
      }
      updateSkins(model)
      return true
    },
    playing() {
      return actions.filter((a) => a.fade >= 0).map((a) => a.clip.name)
    },
  }
  return mixer
}

const SAMPLE_Q: Quat = [0, 0, 0, 1]

function readQuatSample(sample: number[]): Quat {
  SAMPLE_Q[0] = sample[0]!
  SAMPLE_Q[1] = sample[1]!
  SAMPLE_Q[2] = sample[2]!
  SAMPLE_Q[3] = sample[3]!
  return SAMPLE_Q
}
