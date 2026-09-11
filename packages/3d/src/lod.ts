// Level of detail: a group whose children are variants of one object,
// nearest first, with the projected size below which each hands over to
// the next; and the instanced form, a population whose every instance
// picks its own level. The choice is the spatial core's, per target,
// after each flush (okf/done/3d-lod.md): a thousand groups cost no JS
// per frame. Three's LOD vocabulary over Unity's screen-relative model.
import { add, createGroup } from "./node.ts"
import type { LodConfig, SceneNode } from "./node.ts"
import { createInstancedMesh, setCastShadow } from "./mesh.ts"
import type { InstancedMesh, InstancedMeshOptions } from "./mesh.ts"
import type { Geometry } from "./geometry.ts"
import type { Material } from "./material.ts"

/**
 * One level of a LOD group: the node drawn at this level and `size`, the
 * group's projected size - the diameter of the sphere around its levels'
 * boxes (radius = half the box diagonal) as a fraction of the viewport
 * height - BELOW which the level hands over to the next (Unity's screen
 * relative transition height). The last level's size is the cull
 * threshold: 0 keeps it drawn at any distance. For a perspective camera
 * `size = radius / (distance * tan(fov / 2))`, so a tree whose box is 4
 * across (radius 3.5) at 30 units under a 60 degree camera measures about
 * 0.2.
 */
export type LodLevel = { node: SceneNode; size: number }

export type LodOptions = {
  /** The cross-fade band as a fraction of each threshold (default 0, a
   * hard switch with hysteresis): a boundary at size `s` widens to
   * `[s, s * (1 + fade))`, in which both levels draw with complementary
   * screen-hash dithers (the stock materials; a custom class composes
   * LOD_FADE). Shadow tiles never fade: their depth pass switches hard
   * at the band's midpoint. */
  fade?: number
}

// A level's size and the band fraction, checked where the mistake is
// made; the core re-checks with the same rules.
function checkSizes(levels: { size: number }[], fade: number | undefined, site: string): number {
  levels.forEach((l, k) => {
    if (!(Number.isFinite(l.size) && l.size >= 0)) throw new Error(site + ": level " + k + " size must be a finite number >= 0, got " + l.size)
    if (k > 0 && l.size >= levels[k - 1]!.size) throw new Error(site + ": level sizes must be strictly descending, nearest first (level " + k + ": " + l.size + " after " + levels[k - 1]!.size + ")")
  })
  let f = fade ?? 0
  if (!(Number.isFinite(f) && f >= 0)) throw new Error(site + ": fade must be a finite fraction >= 0, got " + f)
  return f
}

/**
 * A LOD group (Three's LOD, Unity's LODGroup): a group node whose levels
 * are its children, one drawn at a time by projected size on every
 * target - the scene and each view by their own camera, shadow tiles by
 * the scene camera, so a caster's shadow matches the level the camera
 * sees. The choice is the spatial core's after each flush; zero JS per
 * frame however many groups. Picking and collision through the scene see
 * the level the scene draws (a view's pick the view's). The group is
 * measured by the sphere around its levels' boxes; nested groups chain.
 * Re-declare with setLod; `setLod(group, [])` makes it a plain group.
 */
export function createLod(levels: LodLevel[], opts?: LodOptions): SceneNode {
  let group = createGroup()
  setLod(group, levels, opts)
  return group
}

/**
 * Declare (or with an empty list clear) a node's LOD levels; a level node
 * not yet under the node is added as its child (re-parented if it was
 * elsewhere). On the population of createInstancedLod the levels are
 * sizes only (`{ size }`): its level meshes are fixed at creation.
 */
export function setLod(node: SceneNode, levels: (LodLevel | { size: number })[], opts?: LodOptions): void {
  if (node._destroyed) throw new Error("setLod: the node was destroyed")
  let fade = checkSizes(levels, opts?.fade, "setLod")
  let population = node.kind === "mesh" && (node as InstancedMesh)._instances?.levels !== null && (node as InstancedMesh)._instances !== null
  let config: LodConfig | null = null
  if (levels.length > 0) {
    let nodes = levels.map(l => ("node" in l ? l.node : null))
    if (population) {
      if (nodes.some(n => n !== null)) throw new Error("setLod: an instanced LOD's levels are its meshes, fixed at creation - pass sizes only")
      let count = (node as InstancedMesh)._instances.levels!.length
      if (levels.length !== count) throw new Error("setLod: an instanced LOD with " + count + " level meshes takes " + count + " sizes, got " + levels.length)
    } else {
      if (nodes.some(n => n === null)) throw new Error("setLod: every level names a node")
      for (let n of nodes as SceneNode[]) {
        if (n === node) throw new Error("setLod: a level cannot be the group itself")
        if (n.kind === "instance") throw new Error("setLod: an instance is slot-bound to its mesh and cannot be a level")
        if (n._destroyed) throw new Error("setLod: a level node was destroyed")
        if (nodes.indexOf(n) !== nodes.lastIndexOf(n)) throw new Error("setLod: a node is listed twice")
      }
      for (let n of nodes as SceneNode[]) if (n.parent !== node) add(node, n)
    }
    config = { levels: levels.map((l, k) => ({ node: nodes[k]!, size: l.size })), fade }
  }
  node._lod = config
  if (node._scene !== null) node._scene._bindLod(node)
}

/** One level of an instanced LOD: what the population draws at this
 * level, and the projected size (per INSTANCE) below which it hands
 * over - see LodLevel. */
export type InstancedLodLevel = { geometry: Geometry; material: Material; size: number }

export type InstancedLodOptions = InstancedMeshOptions &
  LodOptions & {
    /** Every level draws into the scene's shadow map (setCastShadow on
     * each); default false. */
    castShadow?: boolean
  }

/**
 * An instanced LOD: one population whose every instance draws at the
 * level its OWN projected size picks - a forest as three draw entries
 * (mesh, simplified mesh, card) whatever the spread, the near trees full
 * and the far ones cards. Returns the first level's mesh, which is the
 * population: addInstance / `<Instance>` on it, its instance nodes place
 * and pick as on any InstancedMesh, and the other levels are its
 * children at identity, each with a matrix buffer of its own the core
 * stages an instance's record into (`_instances.levels`, nearest first;
 * their per-level style streams take setInstanceStyle together). Records
 * are target-agnostic, so instances pick by the SCENE camera and every
 * view and shadow tile draws that choice; instanced levels switch hard
 * with hysteresis (`fade` is refused). Layers and castShadow set on the
 * population reach every level.
 */
export function createInstancedLod(levels: InstancedLodLevel[], opts?: InstancedLodOptions): InstancedMesh {
  if (levels.length === 0) throw new Error("createInstancedLod: at least one level")
  let fade = checkSizes(levels, opts?.fade, "createInstancedLod")
  if (fade !== 0) throw new Error("createInstancedLod: instanced levels switch hard (a per-instance fade would need a per-record value); drop fade")
  let meshes = levels.map((l, k) =>
    createInstancedMesh(l.geometry, l.material, { capacity: opts?.capacity, bounds: opts?.bounds, label: opts?.label !== undefined ? opts.label + "-lod" + k : undefined }),
  )
  let population = meshes[0]!
  let shared = population._instances.nodes
  for (let m of meshes) {
    m._instances.nodes = shared
    m._instances.levels = meshes
  }
  for (let m of meshes.slice(1)) add(population, m)
  population._lod = { levels: levels.map(l => ({ node: null, size: l.size })), fade }
  if (opts?.castShadow === true) setCastShadow(population, true)
  return population
}
