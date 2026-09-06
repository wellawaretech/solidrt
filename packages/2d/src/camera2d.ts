// The 2d camera: Godot's Camera2D and Three's MapControls in one control
// over the layers' shared CameraUpdate - drag to pan with inertia, wheel
// and pinch to zoom about the pointer, eased glides, fit-to-rect, follow a
// moving point through a dead zone with damping, rotation about the pivot,
// world bounds - extracted so apps stop rebuilding the same hundred
// lines. The motion itself is camera-motion.ts, pure and
// headless-checked; this file adds the input.
//
// Input goes through core's merged transform recognizer (createTransform),
// the same path as @solidrt/3d's orbit camera: the drag participates in
// the gesture arena (a layer inside a scroller arbitrates instead of both
// panning), the slop is swallowed, and one delta arrives per frame. One
// finger or a mouse drag pans; two fingers pan by their focal point and
// zoom by their span about it; the wheel zooms directly through an eased
// glide. A press that releases without the recognizer engaging is a tap,
// reported as a world point.
//
// Coordinates: anchors (the point a zoom keeps pinned, a tap) use the
// event's localX/localY, i.e. the input element's own pixels, which for
// the layer's leaf are layer pixels. The recognizer's deltas are
// window-logical pixels (clientX/clientY), the convention core's
// ScrollView shares - so the input element must not be scaled relative
// to the window; a translated leaf is fine.
//
// Pose is plain mutable state advanced by update(dt) from the app's own
// onFrame; the control registers no frame loop of its own. update() pushes
// the pose to the layers only when it changed (one setCamera per driven
// layer: a shared-params write, however many sprites exist) and reports
// that, so per-frame dependents (labels re-projected via projectCamera)
// can follow the camera without recomputing every frame.

import { createTransform } from "@solidrt/core"
import type { PointerEvent, WheelEvent } from "@solidrt/core"
import { createCameraMotion } from "./camera-motion.ts"
import type { Camera2dMotion, Camera2dMotionOptions, Camera2dTarget } from "./camera-motion.ts"
import { unprojectCamera } from "./camera.ts"

export type Camera2dOptions = Camera2dMotionOptions & {
  /** A press that released without dragging or pinching: the world point
   * under it, plus the release event for its modifiers. */
  onTap?: (x: number, y: number, event: PointerEvent) => void
}

export type Camera2d = Camera2dMotion & {
  /** Spread onto the element that receives input - the layer's own
   * `<texture>` leaf: `<texture src={layer.texture} {...cam.handlers} />`. */
  handlers: {
    onPointerDown(e: PointerEvent): void
    onPointerMove(e: PointerEvent): void
    onPointerUp(e: PointerEvent): void
    onWheel(e: WheelEvent): void
  }
}

/**
 * Create a 2d camera driving `target`'s camera, where `target` is a sprite
 * or record layer (or several: a layer and its overlay layer share one
 * camera), or anything else with the layers' `setCamera`, such as a signal
 * setter feeding `<TileLayer camera>`. The initial pose applies
 * immediately: the world fitted when `world` is given and no `zoom`.
 */
export function createCamera2d(target: Camera2dTarget | Camera2dTarget[], options: Camera2dOptions): Camera2d {
  let motion = createCameraMotion(target, options)
  // Every pointer down on the element, in its local pixels: the pinch's
  // focal anchor, which the recognizer reports in window pixels only.
  let pointers = new Map<number, { x: number; y: number }>()
  let gesture = false
  // The pointer whose press may still be a tap (alone, and the recognizer
  // has not engaged).
  let tap: number | null = null

  let focal = () => {
    let fx = 0
    let fy = 0
    for (let p of pointers.values()) {
      fx += p.x
      fy += p.y
    }
    let n = Math.max(1, pointers.size)
    return { x: fx / n, y: fy / n }
  }

  let transform = createTransform({
    onTransformStart: () => {
      gesture = true
      tap = null
    },
    onTransformMove: (t) => {
      if (t.scale !== 1) {
        let f = focal()
        motion.zoomAt(f.x, f.y, t.scale)
      }
      if (t.dx !== 0 || t.dy !== 0) motion.panBy(t.dx, t.dy)
    },
    onTransformEnd: () => {
      gesture = false
      motion.release()
    },
  })

  return {
    ...motion,
    handlers: {
      onPointerDown(e) {
        if (e.button != null && e.button !== 0) return
        pointers.set(e.pointerId, { x: e.localX, y: e.localY })
        tap = pointers.size === 1 ? e.pointerId : null
        // A finger landing on a gliding view stops it where it is.
        motion.interrupt()
        transform.handlers.onPointerDown(e)
      },
      onPointerMove(e) {
        let p = pointers.get(e.pointerId)
        if (p) {
          p.x = e.localX
          p.y = e.localY
        }
        transform.handlers.onPointerMove(e)
      },
      onPointerUp(e) {
        let wasTap = tap === e.pointerId && !gesture && pointers.size === 1
        transform.handlers.onPointerUp(e)
        pointers.delete(e.pointerId)
        if (tap === e.pointerId) tap = null
        if (wasTap && options.onTap) {
          let [wx, wy] = unprojectCamera(motion.camera(), e.localX, e.localY)
          options.onTap(wx, wy, e)
        }
      },
      onWheel(e) {
        motion.wheel(e.localX, e.localY, e.deltaY)
      },
    },
  }
}
