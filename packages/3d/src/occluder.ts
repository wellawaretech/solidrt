// Occluders: `setOccluder(node, shape)` marks a node as something the
// camera cannot see through, and the spatial core culls what lies wholly
// behind the shown occluders per target - Godot's OccluderInstance3D,
// Unity's static occluders, in their vocabulary (a box, a quad, a sphere,
// a triangle list) plus the two shortcuts a mesh affords: its own
// geometry and its own bounds. Nothing to call per frame: the core
// rebuilds a target's small depth buffer only when its camera or an
// occluder moves, and a hidden occluder occludes nothing (an open door).
// A triangle occludes exactly what lies behind it, so "geometry" and a
// proxy geometry are exact for any mesh, holes included; a box or a
// sphere stands for a solid and must be used as one. A mesh whose
// triangles move in the vertex stage (skinned, morphing, a sprite, an
// instanced population) has no geometry to occlude with and refuses
// "geometry": give it a proxy shape at the node instead.
//
// The same occluders block SOUND: a positional sound (attachSound) whose
// straight path to the listener crosses a shown occluder is dimmed and
// dulled by the occluder's `audio` (a gain and a low-pass cutoff; the
// defaults are an interior wall heard through), `audio: false` makes an
// occluder sight-only (glass). One authoring for sight and sound.

import * as spatial from "flux:spatial"
import type { CullState as CoreCullState, OccluderAudio, OccluderShape } from "flux:spatial"
import { geometryAttribute, geometryBounds, geometryTopology, geometryVertexCount } from "./geometry.ts"
import type { Geometry } from "./geometry.ts"
import type { SceneNode } from "./node.ts"
import type { Mesh } from "./mesh.ts"
import type { Vec3 } from "./math.ts"
import type { Scene } from "./scene.ts"

/** Why a mesh is not drawn on a target, or "on": "hidden" (it or an
 * ancestor invisible), "frustum" (outside the camera), "occluded"
 * (behind the occluders), "lod" (not the level drawn), the first in that
 * order; read with scene.cullState and view.cullState. */
export type CullState = CoreCullState

/** What a node occludes with, in its own space: `"geometry"` a mesh's own
 * triangles (exact, the common case for walls and floors); `"bounds"` a
 * mesh's local box as a solid (twelve triangles, for a crate or a
 * building block that fills its box); `{ box }` a solid box, min xyz then
 * max xyz; `{ quad: [width, height] }` a two-sided quad in the node's XY
 * plane centered at its origin (Godot's QuadOccluder3D, the one-wall
 * shape); `{ sphere: { center, radius } }` a solid sphere; `{ geometry }`
 * a proxy geometry (a low-poly hull of the mesh, a triangle list). The
 * object forms (`{ shape: "geometry" | "bounds" }` for the two
 * shortcuts) take `audio`: what the occluder does to a positional sound
 * crossing it - absent, the default blocking (a gain of 0.25 and a
 * low-pass at 2000 Hz); `false`, sight only; `{ gain?, cutoff? }`, its
 * own level (0..1) and cutoff (Hz). */
export type OccluderSpec =
  | "geometry"
  | "bounds"
  | ((
      | { shape: "geometry" | "bounds" }
      | { box: Float32Array | number[] }
      | { quad: [number, number] }
      | { sphere: { center: Vec3; radius: number } }
      | { geometry: Geometry }
    ) & { audio?: OccluderAudio | false })

/**
 * Declare what the node occludes (null clears it). Takes effect in every
 * target with occlusion on, re-applied on every scene enter; the node's
 * own draw is never occluded by its own shape. A Group takes any explicit
 * shape; the "geometry" and "bounds" shortcuts need a mesh. Throws on a
 * shape the node cannot provide.
 */
export function setOccluder(node: SceneNode, occluder: OccluderSpec | null): void {
  if (node._destroyed) return
  let shape = occluder === null ? null : resolve(node, occluder)
  node._occluder = shape
  if (node._node !== null) {
    spatial.setOccluder(node._node, shape)
    node._scene?._schedule()
  }
}

/**
 * Mark every mesh under the scene root whose local box's longest edge is
 * at least `minSize` (world units, before the node's scale) as an
 * occluder by its own geometry (Unity's static-occluder flag by size):
 * the large static pieces of a level - walls, floors, buildings - with
 * one call, the small props left alone. Meshes that cannot occlude by
 * geometry (sprites, instanced populations, skinned and morphing parts,
 * non-triangle topologies) are skipped. Returns the meshes marked, so an
 * app can clear or refine them.
 */
export function occludersFromMeshes(scene: Scene, opts: { minSize: number }): Mesh[] {
  if (!(opts.minSize >= 0)) throw new Error("occludersFromMeshes: minSize must be >= 0")
  let marked: Mesh[] = []
  let walk = (node: SceneNode) => {
    if (node.kind === "mesh") {
      let mesh = node as Mesh
      if (canOccludeByGeometry(mesh) === null) {
        let b = geometryBounds(mesh.geometry)
        let longest = Math.max(b[3]! - b[0]!, b[4]! - b[1]!, b[5]! - b[2]!)
        if (longest >= opts.minSize) {
          setOccluder(mesh, "geometry")
          marked.push(mesh)
        }
      }
    }
    for (let c of node.children) walk(c)
  }
  walk(scene.root)
  return marked
}

function resolve(node: SceneNode, spec: OccluderSpec): OccluderShape {
  if (typeof spec === "object" && "shape" in spec) {
    let shape = resolve(node, spec.shape)
    return spec.audio === undefined ? shape : { ...shape, audio: spec.audio }
  }
  if (typeof spec === "object" && spec.audio !== undefined) {
    let { audio, ...rest } = spec
    if (audio !== false && !(typeof audio === "object" && audio !== null)) throw new Error("setOccluder: audio is false or { gain?, cutoff? }")
    return { ...resolve(node, rest as OccluderSpec), audio }
  }
  if (spec === "geometry") {
    let mesh = asMesh(node, spec)
    let reason = canOccludeByGeometry(mesh)
    if (reason !== null) throw new Error("setOccluder: " + reason + " cannot occlude by its geometry; give it a box, quad, sphere or proxy geometry")
    return triangles(mesh.geometry)
  }
  if (spec === "bounds") {
    let mesh = asMesh(node, spec)
    if (mesh._sprite) throw new Error("setOccluder: a sprite has no bounds to occlude with; give it a shape")
    let b = mesh._instances !== null ? mesh._instances.bounds : geometryBounds(mesh.geometry)
    if (b === null) throw new Error("setOccluder: an instanced mesh without explicit bounds has no box to occlude with")
    return { box: Float32Array.from(b) }
  }
  if ("box" in spec) {
    if (spec.box.length !== 6) throw new Error("setOccluder: box must hold 6 numbers (min xyz, max xyz)")
    return { box: Float32Array.from(spec.box) }
  }
  if ("quad" in spec) {
    if (!(spec.quad[0] > 0 && spec.quad[1] > 0)) throw new Error("setOccluder: quad needs a width and a height above 0")
    return { quad: [spec.quad[0], spec.quad[1]] }
  }
  if ("sphere" in spec) {
    if (!(spec.sphere.radius > 0)) throw new Error("setOccluder: sphere needs a radius above 0")
    return { sphere: { center: [spec.sphere.center[0], spec.sphere.center[1], spec.sphere.center[2]], radius: spec.sphere.radius } }
  }
  if ("geometry" in spec) return triangles(spec.geometry)
  throw new Error('setOccluder: a shape is "geometry", "bounds", { box }, { quad }, { sphere } or { geometry }')
}

function asMesh(node: SceneNode, spec: string): Mesh {
  if (node.kind !== "mesh") throw new Error('setOccluder: "' + spec + '" needs a mesh; a ' + node.kind + " takes an explicit shape")
  return node as Mesh
}

/** Why the mesh's own triangles are not where its geometry says, or null
 * when they are. */
function canOccludeByGeometry(mesh: Mesh): string | null {
  if (mesh._sprite) return "a sprite"
  if (mesh._instances !== null) return "an instanced mesh"
  if (mesh._cullJoints !== null) return "a skinned mesh"
  if (mesh.geometry.morphs !== undefined) return "a morphing mesh"
  if (geometryTopology(mesh.geometry) !== "triangles") return "a " + geometryTopology(mesh.geometry) + " geometry"
  return null
}

/** The geometry's triangles as an occluder mesh: positions copied out of
 * the interleaved stream, the index array as it stands. */
function triangles(geometry: Geometry): OccluderShape {
  if (geometryTopology(geometry) !== "triangles") throw new Error("setOccluder: an occluder geometry must be a triangle list, got " + geometryTopology(geometry))
  let pos = geometryAttribute(geometry, "aPos")
  if (pos === null) throw new Error("setOccluder: the geometry has no aPos channel")
  let count = geometryVertexCount(geometry, "setOccluder")
  let positions = new Float32Array(count * 3)
  for (let i = 0; i < count; i++) {
    positions[i * 3] = pos.get(i, 0)
    positions[i * 3 + 1] = pos.get(i, 1)
    positions[i * 3 + 2] = pos.get(i, 2)
  }
  if (geometry.indices.length === 0 || geometry.indices.length % 3 !== 0) throw new Error("setOccluder: the geometry's indices do not form triangles")
  return { mesh: { positions, indices: geometry.indices } }
}
