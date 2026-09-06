// The 2d camera: Godot's Camera2D and Three's MapControls in one control
// over the layers' shared CameraUpdate - drag to pan with inertia, wheel
// and pinch to zoom about the pointer, eased glides, fit-to-rect, follow a
// moving point through a dead zone with damping, rotation about the pivot,
// world bounds - extracted so apps stop rebuilding the same hundred
// lines. The motion itself is camera-motion.ts, pure and
// headless-checked; this file adds the input and the reactive face.
//
// Input goes through core's merged transform recognizer (createTransform),
// the same path as @solidrt/3d's orbit camera: the drag participates in
// the gesture arena (a layer inside a scroller arbitrates instead of both
// panning), the slop is swallowed, and one delta arrives per frame. One
// finger or a mouse drag pans; two fingers pan by their focal point and
// zoom by their span about it; the wheel zooms directly through an eased
// glide. Taps are not the camera's business: the layer's dispatch
// synthesizes them (onTap on sprites, groups and the root), as the DOM
// does for the elements a map control sits on.
//
// Two ways in. `attach(layer)` listens at the layer's root, so the camera
// sees exactly the presses the sprites let through: a sprite that drags
// itself stops its down and the camera never pans it, a drag on empty
// space or on a sprite without a claim pans, a wheel anywhere zooms.
// `handlers` spread straight onto a leaf for the layers without a
// dispatch (a tile world on its own). Either way the pose reaches the
// layers synchronously on input, so a drag needs no frame loop and the
// next paint carries the new camera.
//
// Coordinates: everything arrives in the input element's own frame. The
// recognizer reports the pinch focal in the element's local pixels (layer
// pixels on the layer's leaf) and the drag deltas in its parent frame, and
// the wheel uses the event's localX/localY - so a leaf under a designSize
// fit or any scaled ancestor pans and zooms correctly.
//
// Pose is plain mutable state advanced by update(dt) from a frame loop;
// `active()` is the reactive gate for that loop (true while a glide,
// fling, fit or follow needs frames; false at rest, when the camera costs
// nothing), which is how <Camera2d> runs frames only while something
// moves. update() pushes the pose to the layers only when it changed (one
// setCamera per driven layer: a shared-params write, however many sprites
// exist) and reports that, so per-frame dependents (labels re-projected
// via projectCamera) can follow the camera without recomputing every
// frame.

import { createSignal, createTransform } from "@solidrt/core"
import type { PointerEvent, WheelEvent } from "@solidrt/core"
import { createCameraMotion } from "./camera-motion.ts"
import type { Camera2dMotion, Camera2dMotionOptions, Camera2dTarget } from "./camera-motion.ts"
import type { LayerPointerListener } from "./layer.ts"

export type Camera2dOptions = Camera2dMotionOptions

export type Camera2d = Camera2dMotion & {
  /** Reactive: whether update(dt) needs frames (see Camera2dMotion.active).
   * Track it to run a frame loop only while the camera moves. */
  active: () => boolean
  /** Spread onto the element that receives input where there is no sprite
   * dispatch to attach to - a tile world's leaf. Layers with a dispatch
   * take `attach`. */
  handlers: {
    onPointerDown(e: PointerEvent): void
    onPointerMove(e: PointerEvent): void
    onPointerUp(e: PointerEvent): void
    onWheel(e: WheelEvent): void
  }
  /**
   * Take input from a layer's root (LayerBase.listen): the presses the
   * sprites let through pan and pinch, every wheel zooms. A sprite that
   * claims a press (stopPropagation on its down) keeps the camera out of
   * that drag. Returns the detach. The layer must be one the camera
   * drives, or its events arrive in a frame the camera does not show.
   */
  attach(layer: { listen(listener: LayerPointerListener): () => void }): () => void
}

/**
 * Create a 2d camera driving `target`'s camera, where `target` is a sprite
 * or record layer (or several: a layer and its overlay layer share one
 * camera), or anything else with the layers' `setCamera`, such as a signal
 * setter feeding `<TileLayer camera>`. The initial pose applies
 * immediately: the world fitted when `world` is given and no `zoom`.
 * Create it from an owned scope (a component body, like the orbit
 * camera): the input recognizer registers its cleanup with the owner.
 */
export function createCamera2d(target: Camera2dTarget | Camera2dTarget[], options: Camera2dOptions): Camera2d {
  // Written by the motion from wherever it is driven - handlers, frames,
  // a set() in a component body - hence the owned-write opt-in.
  let [active, setActive] = createSignal(false, { ownedWrite: true })
  let motion = createCameraMotion(target, options, setActive)

  let transform = createTransform({
    onTransformMove: (t) => {
      if (t.scale !== 1) motion.zoomAt(t.x, t.y, t.scale)
      if (t.dx !== 0 || t.dy !== 0) motion.panBy(t.dx, t.dy)
    },
    onTransformEnd: () => motion.release(),
  })

  let handlers: Camera2d["handlers"] = {
    onPointerDown(e) {
      if (e.button != null && e.button !== 0) return
      // A finger landing on a gliding view stops it where it is.
      motion.interrupt()
      transform.handlers.onPointerDown(e)
    },
    onPointerMove(e) {
      transform.handlers.onPointerMove(e)
    },
    onPointerUp(e) {
      transform.handlers.onPointerUp(e)
    },
    onWheel(e) {
      motion.wheel(e.localX, e.localY, e.deltaY)
    },
  }

  return {
    ...motion,
    active,
    handlers,
    attach(layer) {
      // Input pushes the pose synchronously (update(0)), so a drag needs
      // no frame loop and the next paint carries the new camera.
      return layer.listen({
        onPointerDown: e => {
          handlers.onPointerDown(e.native)
          motion.update(0)
        },
        onPointerMove: e => {
          handlers.onPointerMove(e.native)
          motion.update(0)
        },
        onPointerUp: e => handlers.onPointerUp(e.native),
        onWheel: e => {
          handlers.onWheel(e.native)
          motion.update(0)
        },
      })
    },
  }
}
