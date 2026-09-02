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
// `sampleChannel` (./clip.ts) stays the pure JS sampling core (checks,
// custom drivers, bake-side resampling) - the native evaluator implements
// the same glTF contract.

import * as spatial from "flux:spatial"
import type { NodeId } from "flux:spatial"
import { on } from "srt:events"
import type { ModelClip } from "./gltf.ts"
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
   * listing what the model has. The model must be in a scene. */
  play(name: string, opts?: MixerPlayOptions): void
  /** Fade every active clip out (instantly with no fadeMs); the nodes
   * keep the last written pose. */
  stop(opts?: { fadeMs?: number }): void
  /** Names of the clips currently playing or fading in. */
  playing(): string[]
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

// Player end reports ("spatialClipEnd", emitted before each frame's JS)
// routed back to the owning mixer's action.
const PLAYER_ENDS = new Map<number, (reason: string) => void>()
on("spatialClipEnd", (event: { player: number; reason: string }) => {
  PLAYER_ENDS.get(event.player)?.(event.reason)
})

/** Register the clip's baked channels with the core once; the id is
 * cached on the clip (shared by every mixer) and freed by model.dispose. */
function coreClip(clip: ModelClip): number {
  if (clip._core !== undefined) return clip._core
  let channels = clip.channels
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
  clip._core = spatial.createClip(clip.duration, meta, times, values)
  return clip._core
}

// One playing clip: the JS bookkeeping over a core player.
type Action = { name: string; player: number; fadingOut: boolean; finished: boolean }

/**
 * A mixer over a model's clips. One mixer per model; make it where you
 * made the model. Playback is core-driven - nothing to call per frame.
 */
export function createMixer(model: Model): Mixer {
  // Channel targets resolve at play: clips index the model's node table,
  // and the arena ids are per scene-entry, so a re-added model binds
  // fresh ones.
  let targetsFor = (clip: ModelClip): NodeId[] =>
    clip.channels.map((c) => {
      let entry = model.nodes[c.node]
      if (entry === undefined) throw new Error("createMixer: clip '" + clip.name + "' targets a missing node " + c.node)
      let id = entry.node._node
      if (id === null) throw new Error("play: the model must be in a scene (add() it first) before clips can play")
      return id
    })

  let actions: Action[] = []
  let dropAction = (action: Action) => {
    PLAYER_ENDS.delete(action.player)
    let i = actions.indexOf(action)
    if (i >= 0) actions.splice(i, 1)
  }
  let start = (clip: ModelClip, opts: MixerPlayOptions, weight: number, fade: number): void => {
    let player = spatial.createPlayer(coreClip(clip), targetsFor(clip), opts.speed ?? 1, opts.loop ?? true, weight, fade)
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
  }
  return mixer
}
