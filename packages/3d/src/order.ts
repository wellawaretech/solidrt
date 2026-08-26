// Draw-list ordering for a scene: a pure function of the live meshes and the
// camera's view matrix, with no GUI import, so the check rig
// (checks/order-check.ts) runs it headless on flux against a linear oracle.
// The scene calls it whenever the order is dirty and hands the result to
// setDrawOrder.

import type { Mat4, Vec3 } from "./math.ts"

/** The slice of a Mesh the sort reads (field names match Mesh so the
 * scene passes its meshes straight through). */
export type Orderable<T> = {
  _entry: T | null
  _transparent: boolean
  renderOrder: number
  _center: Vec3
}

/**
 * Draw order: `first` (the background entry, if any), then opaque meshes by
 * renderOrder with add order within a key, then transparent meshes by
 * renderOrder then back-to-front by the view-space depth of the world-bounds
 * center. The center, not the origin (Three's key), so geometry built
 * off-origin sorts by where it is; and not the nearest bounds point, which
 * would draw a large translucent ground plane over the small translucents
 * resting on it. Per-mesh only: no per-triangle sort, no OIT. `entry`
 * picks the id ordered for each mesh (default its own `_entry`; a view
 * passes its per-mesh entries in its target).
 */
export function orderEntries<T>(
  meshes: readonly Orderable<T>[],
  view: Mat4,
  first?: T,
  entry: (m: Orderable<T>) => T | null = m => m._entry,
): T[] {
  let opaque: Orderable<T>[] = []
  let transparent: Orderable<T>[] = []
  for (let m of meshes) {
    if (entry(m) === null) continue
    ;(m._transparent ? transparent : opaque).push(m)
  }
  // Array sort is stable, so equal keys keep add order.
  opaque.sort((a, b) => a.renderOrder - b.renderOrder)
  if (transparent.length > 1) {
    // The camera looks down -z in view space, so farther is more negative
    // and ascending depth is back-to-front.
    let depth = new Map<Orderable<T>, number>()
    for (let m of transparent) {
      let c = m._center
      depth.set(m, view[2] * c[0] + view[6] * c[1] + view[10] * c[2] + view[14])
    }
    transparent.sort((a, b) => a.renderOrder - b.renderOrder || depth.get(a)! - depth.get(b)!)
  }
  let order: T[] = []
  if (first !== undefined) order.push(first)
  for (let m of opaque) order.push(entry(m)!)
  for (let m of transparent) order.push(entry(m)!)
  return order
}
