// Positional sound: `attachSound(node, sound, options)` places a core
// Sound (createSound, createPcmSound, createSoundStream) at a scene node,
// heard by the scene's listener (the camera, or scene.setListener's
// node): Three's PositionalAudio under a mesh, Unity's AudioSource on a
// GameObject, Godot's AudioStreamPlayer3D. Nothing to call per frame: the
// spatial core writes each voice's pan, distance gain, doppler rate and
// occlusion low-pass after every flush that moved the node, the camera
// or an occluder, and nothing while the scene stands still. The sound's
// own setGain and setRate keep meaning "how loud / how fast is this
// thing": the placement composes under them.
//
// The binding follows the node's scene membership, like spring bones: it
// places the sound's voices when the node enters a scene (the core node
// is new each time) and, when the node leaves, stops them and releases
// the placement - a looping engine must not outlive its kart. A sound
// played while its node is outside any scene plays unplaced until the
// node enters.

import type { EmitterOptions, Sound } from "@solidrt/core/audio"
import type { SceneNode, SubtreeBinding } from "./node.ts"

/** How a positional sound falls off and shifts: flux:audio's emitter
 * parameters, minus the node and the listener (the node's scene's). */
export type PositionalSoundOptions = Pick<EmitterOptions, "distanceModel" | "refDistance" | "maxDistance" | "rolloff" | "doppler">

/**
 * Place `sound` at `node` for as long as the node is in a scene (see the
 * header). Returns the detach: the voices keep playing where they are,
 * unplaced. Throws for a destroyed node.
 */
export function attachSound(node: SceneNode, sound: Sound, options: PositionalSoundOptions = {}): () => void {
  if (node._destroyed) throw new Error("attachSound: the node was destroyed")
  let binding: SubtreeBinding = {
    enter() {
      let scene = node._scene
      if (scene === null || node._node === null) return
      sound.setEmitter({
        node: node._node,
        listener: scene._listener(),
        distanceModel: options.distanceModel,
        refDistance: options.refDistance,
        maxDistance: options.maxDistance,
        rolloff: options.rolloff,
        doppler: options.doppler,
      })
    },
    leave() {
      sound.stop()
      sound.setEmitter(null)
    },
  }
  if (node._bindings === null) node._bindings = []
  node._bindings.push(binding)
  if (node._node !== null) binding.enter()
  return () => {
    let list = node._bindings
    if (list !== null) {
      let i = list.indexOf(binding)
      if (i >= 0) list.splice(i, 1)
    }
    sound.setEmitter(null)
  }
}
