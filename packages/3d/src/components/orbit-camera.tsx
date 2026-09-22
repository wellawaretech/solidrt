import { createEffect, merge, onFrame, untrack, useContext } from "@solidrt/core"
import type { InputMap, VoidComponent } from "@solidrt/core"
import type { Vec2 } from "@solidrt/core/input"
import { SceneContext } from "./context.tsx"
import { createOrbitCamera } from "../orbit.ts"
import type { OrbitAxes, OrbitCamera as OrbitCameraHandle, OrbitCameraOptions } from "../orbit.ts"
import type { Vec3 } from "../math.ts"
import type { Mesh } from "../mesh.ts"

// Cap on the frame loop's per-frame dt in seconds, so the first tick after
// a suspended stretch (a resumed app, a reload) cannot leap the pose.
const MAX_ORBIT_DT = 0.1
// Default occlusion radius, world units: the sphere the eye keeps clear
// of an obstacle (Babylon's collisionRadius is 0.5; a near plane's worth).
const OCCLUSION_RADIUS = 0.2

export type OrbitCameraProps = OrbitCameraOptions & {
  /** How the zoom and `focus` map a screen point to the world (the
   * built-in `zoomAnchor`; a `zoomAnchor` prop overrides it): "pick"
   * (the default) picks the scene under the point and falls back to the
   * plane at the target's depth when nothing is hit, "plane" uses that
   * plane only (cheaper; a scene whose geometry is not what the user
   * zooms at), false zooms toward the target. */
  anchor?: "pick" | "plane" | false
  /** Re-seat the pivot at what the view centre looks at when a drag
   * begins (the built-in `rotateAnchor`, a pick; a `rotateAnchor` prop
   * overrides it): Blender's auto-depth. Off by default - a drag orbits
   * the target as it is, and a double tap (`focus`) re-seats it
   * explicitly - because a re-seat changes `distance` on every press,
   * which a viewer reading the pose does not expect. */
  repivot?: boolean
  /** The occlusion constraint (the built-in `occluder`): keep the eye
   * `radius` clear of whatever the scene raycast finds between the
   * target and the eye - a wall behind a third-person camera. Hits
   * within `minDistance` of the target are ignored (default: the
   * radius), so the followed character's own mesh, which the ray starts
   * inside, does not pull the camera in (Cinemachine's rule); `layers`
   * and `meshes` filter the query further (scene.raycast's options). */
  occlusion?: { radius?: number; minDistance?: number; layers?: number; meshes?: Mesh[] }
  /** The input map driving the control (InputMap.drive): its `rotate`,
   * `zoom`, `pan` and `focus` actions, or the ones `actions` names. Live:
   * a new map reconnects. Without one the control moves only through
   * `ref`. */
  input?: InputMap<any>
  /** Action names per axis when the map's differ (null skips an axis). */
  actions?: Partial<Record<keyof OrbitAxes, string | null>>
  /** The control's handle (pose()/set()/glideTo()/fit()/follow()/eye()/
   * orbiting()/active(), the verbs, `axes` - also the debug command
   * shape). */
  ref?: (orbit: OrbitCameraHandle) => void
}

/**
 * createOrbitCamera as a Scene (or View3d) child: drives the enclosing
 * camera - the scene's, or inside a `<View3d>` that view's - through
 * context, and takes its input from the map in `input`, nothing else
 * (ARCHITECTURE.md: no device wiring in a component; the app binds a
 * pointer feed, a pad or anything else to the map). The pose props
 * (target, azimuth, elevation, distance) are initial values - change the
 * pose at runtime through `ref`'s set() or the verbs; every other prop is
 * live, forwarded to the control as a getter and read where it applies,
 * so a clamp, a rate, `damping`, the follow's zones or an anchor callback
 * follows its prop, and a clamp change (`clampPose` included) re-clamps
 * the pose at once. The projection-dependent hooks are built here from
 * the owner's pick, unproject and raycast (`anchor`, `repivot`,
 * `occlusion`), since the component sits where the projection is. The frame loop runs only
 * while `active()` (auto-orbit on, a glide, follow, shake or occlusion
 * return in flight, or an axis rate driving); a camera moved by drags
 * alone leaves the app demand-driven idle.
 */
export let OrbitCamera: VoidComponent<OrbitCameraProps> = props => {
  let ctx = useContext(SceneContext)
  let viewport = ctx.viewport
  let mode = () => props.anchor ?? "pick"
  // The world point under a focal (a fraction of the owner's size): the
  // nearest pick, else the point on the plane at the target's depth.
  let pointAt = (focal: Vec2, view: { eye: Vec3; target: Vec3 }): Vec3 | null => {
    let m = mode()
    if (m === false) return null
    let size = viewport.size()
    let x = focal[0] * size.width
    let y = focal[1] * size.height
    if (m === "pick") {
      let hit = viewport.pick(x, y)[0]
      if (hit) return hit.point
    }
    let depth = Math.hypot(view.target[0] - view.eye[0], view.target[1] - view.eye[1], view.target[2] - view.eye[2])
    return viewport.unproject(x, y, depth)
  }
  let zoomAnchor = (focal: Vec2, view: { eye: Vec3; target: Vec3 }): Vec3 | null => pointAt(focal, view)
  // The pivot re-seat: what the view centre looks at, a pick.
  let rotateAnchor = (): Vec3 | null => {
    if (!props.repivot) return null
    let size = viewport.size()
    let hit = viewport.pick(size.width / 2, size.height / 2)[0]
    return hit ? hit.point : null
  }
  let occluder = (target: Vec3, eye: Vec3): number | null => {
    let o = props.occlusion
    if (!o) return null
    let dx = eye[0] - target[0]
    let dy = eye[1] - target[1]
    let dz = eye[2] - target[2]
    let len = Math.hypot(dx, dy, dz)
    if (!(len > 0)) return null
    let radius = o.radius ?? OCCLUSION_RADIUS
    let near = o.minDistance ?? radius
    for (let hit of viewport.raycast(target, [dx / len, dy / len, dz / len], { layers: o.layers, meshes: o.meshes })) {
      if (hit.distance < near) continue
      if (hit.distance >= len) return null
      return Math.max(hit.distance - radius, 0)
    }
    return null
  }
  // The control keeps this object and reads each option where it applies,
  // so the props go through as getters (merge), never a spread: a spread
  // would snapshot every prop once and a change could only remount. A
  // hook prop overrides the built-in one.
  let options: OrbitCameraOptions = merge(props, {
    get zoomAnchor() {
      return props.zoomAnchor ?? zoomAnchor
    },
    get rotateAnchor() {
      return props.rotateAnchor ?? rotateAnchor
    },
    get occluder() {
      return props.occluder ?? occluder
    },
  })
  let orbit = untrack(() => createOrbitCamera(viewport, options))
  createEffect(
    () => props.input,
    input => (input ? input.drive(orbit.axes, untrack(() => props.actions)) : undefined),
  )
  createEffect(
    () => orbit.active(),
    on => {
      if (!on) return
      let last: number | null = null
      return onFrame(tick => {
        let now = tick / 1000
        let dt = last === null ? 0 : Math.min(now - last, MAX_ORBIT_DT)
        last = now
        orbit.update(dt)
      })
    },
  )
  // A clamp or lane prop change re-clamps the pose and pushes at once
  // (set({}) applies the clamps and the lanes, and writes no pose field,
  // so a glide in flight keeps running) - the update() call a Three
  // OrbitControls app makes after setting minDistance, done by the prop.
  // Deferred: the creation already clamped and pushed the initial pose.
  // Untracked: the control reads the options through the getters while
  // it applies them, and the apply wants the values of that moment (the
  // compute above is what tracks them).
  createEffect(
    () => [options.minDistance, options.maxDistance, options.minElevation, options.maxElevation, options.clampPose, options.offset, options.occluder],
    () => untrack(() => orbit.set({})),
    { defer: true },
  )
  untrack(() => props.ref)?.(orbit)
  return null
}
