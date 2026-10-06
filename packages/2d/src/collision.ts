// The kinematic character move, Godot's CharacterBody2D.move_and_slide
// and Unity's Rigidbody2D mover as one pure function: no sprite, no
// velocity state. It takes a volume where the body IS and the motion it
// WANTS, and returns the motion it gets plus what it touched; a
// sprite-driven body applies the result with setSprite. The loop itself
// (depenetration, sweep, slide, floor snap) runs in the spatial core as
// layer.moveAndSlide; this is its free-function spelling, the same as
// @solidrt/3d's moveAndSlide(scene, ...).

import type { MoveOptions, MoveResult, SpriteLayer, Volume } from "./layer.ts"

/** What moveAndSlide needs of a layer. */
export type MoveLayer = Pick<SpriteLayer, "moveAndSlide">

/** Move a body by (dx, dy) through the layer's sprites, sliding along
 * what it hits: layer.moveAndSlide as a function (see it for the
 * contract). */
export function moveAndSlide(layer: MoveLayer, volume: Volume, dx: number, dy: number, opts: MoveOptions = {}): MoveResult {
  return layer.moveAndSlide(volume, dx, dy, opts)
}
