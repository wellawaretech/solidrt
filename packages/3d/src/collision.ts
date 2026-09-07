// The kinematic character move, Godot's CharacterBody3D.move_and_slide
// and Unity's CharacterController.Move as one pure function: no node, no
// velocity state. It takes a volume where the body IS and the motion it
// WANTS, and returns the motion it gets plus what it touched - the
// first-person camera (which has no node) composes it in clampPosition,
// a node-driven body applies the result with setTransform. The loop
// itself (depenetration, sweep, slide, floor snap) runs in the spatial
// core as scene.moveAndSlide; this is its free-function spelling.

import type { MoveOptions, MoveResult, Scene, Volume } from "./scene.ts"
import type { Vec3 } from "./math.ts"

/** What moveAndSlide needs of a scene. */
export type MoveScene = Pick<Scene, "moveAndSlide">

/** Move a body `motion` through the scene's colliders, sliding along what
 * it hits: scene.moveAndSlide as a function (see it for the contract). */
export function moveAndSlide(scene: MoveScene, volume: Volume, motion: Vec3, opts: MoveOptions = {}): MoveResult {
  return scene.moveAndSlide(volume, motion, opts)
}
