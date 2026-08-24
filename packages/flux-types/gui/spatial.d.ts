// The spatial core (gui-enabled runtime only): a native transform hierarchy
// whose flush recomputes only the subtrees that changed and writes the
// results to draw sinks - a draw entry's `uModel` (+ `uNormal`) params and
// its instance count as the visibility switch. Generic on purpose: no camera,
// mesh or light concept. @solidrt/3d is the first consumer; any draw-list
// user with a tree of transforms (a 2D sprite scene, a skeleton) is the same
// shape. Node ids are plain numbers, generation-tagged and never reused, so
// a destroyed node's id throws everywhere.
//
// A transform argument is one Float32Array of 10: position xyz, unit
// quaternion xyzw, scale xyz. Writes queue the node; nothing reaches the
// GPU until flush(). worldMatrix() reads through pending writes.

declare module "flux:spatial" {
  import type { BufferId, DrawId, TextureId } from "flux:gpu"

  export type NodeId = number & { readonly __spatialNode: unique symbol }

  /** A new root node. `visible: false` hides the node's whole subtree. */
  export function createNode(transform: Float32Array, visible: boolean): NodeId
  /** Free a node; its children become roots. A bound sink is dropped
   * without a write (removing the entry is the caller's job). */
  export function destroyNode(node: NodeId): void
  /** Re-parent (null = make a root). Throws on a cycle. */
  export function setParent(node: NodeId, parent: NodeId | null): void
  /** Replace the local transform (compare before calling; an unchanged
   * write still queues the node). */
  export function setTransform(node: NodeId, transform: Float32Array): void
  export function setVisible(node: NodeId, visible: boolean): void
  /**
   * Route the node's world matrix to one draw entry's `uModel` (and
   * `uNormal`, the inverse-transpose, when `normal`). Validated like
   * setDrawParams: the entry must exist and declare those uniforms. The
   * entry is assumed switched off (instanceCount 0); the next flush turns it
   * on with `count` when the node is shown, and off again when hidden.
   */
  export function bindDraw(node: NodeId, target: TextureId, draw: DrawId, normal: boolean, count: number): void
  export function unbindDraw(node: NodeId): void
  /** Change the bound entry's "on" count (an instanced mesh's record count);
   * written at once if the entry is currently on. */
  export function setDrawCount(node: NodeId, count: number): void
  /** Fill `out` (a Float32Array of 16, column-major) with the node's world
   * matrix as the tree stands now, pending writes included. */
  export function worldMatrix(node: NodeId, out: Float32Array): void
  /** Effective visibility (every ancestor visible too) as of the last flush. */
  export function shown(node: NodeId): boolean
  /** Recompute every changed subtree and write the sinks; requests a frame
   * when anything was written. */
  export function flush(): void

  export type ShapeId = number & { readonly __spatialShape: unique symbol }

  /** One hit of raycast(), nearest first. `face`/`uv`/`normal` are present
   * for nodes with a shape (uv only when the shape has UVs); a node with
   * bounds but no shape reports its local box, distance and point only. */
  export type Hit = {
    node: NodeId
    /** World units along the normalized ray. */
    distance: number
    point: [number, number, number]
    /** World-space geometric normal, facing the ray. */
    normal?: [number, number, number]
    /** Triangle index into the shape's index list. */
    face?: number
    uv?: [number, number]
  }

  /** Set (null clears) the node's LOCAL tight box [minX, minY, minZ, maxX,
   * maxY, maxZ]. With one the node is in the picking index: its world box
   * follows the flush; hidden nodes stay in and are skipped at query time. */
  export function setBounds(node: NodeId, bounds: Float32Array | null): void
  /**
   * Triangle data for the picking narrowphase, one copy shared by every
   * node that references it: positions read from an interleaved vertex
   * array (`stride` floats per vertex, xyz at `posOffset`, uv at
   * `uvOffset`, -1 for none) and a Uint16Array/Uint32Array triangle list.
   * Throws on out-of-range indices.
   */
  export function createShape(vertices: Float32Array, stride: number, posOffset: number, uvOffset: number, indices: Uint16Array | Uint32Array): ShapeId
  /** Free a shape; nodes still referencing it fall back to their box. */
  export function destroyShape(shape: ShapeId): void
  export function setShape(node: NodeId, shape: ShapeId | null): void
  /** Every shown node with bounds the ray strikes, nearest first. The
   * direction need not be normalized; distances are world units. Reads
   * the index as of the last flush. */
  export function raycast(origin: Float32Array, direction: Float32Array): Hit[]
  /**
   * Every shown node with bounds whose local box, carried through its
   * world transform, overlaps the world-axis box `bounds` (a Float32Array
   * of 6: [minX, minY, minZ, maxX, maxY, maxZ]; touching counts, a point
   * is min == max). Tested by separating axes, so a rotated flat rect -
   * the 2d marquee case - tests exactly, never by its world AABB.
   * Unordered; reads the index as of the last flush, like raycast.
   */
  export function overlap(bounds: Float32Array): NodeId[]

  /**
   * Route the world DIRECTION of the node's local `vector` (a
   * Float32Array of 3) into
   * vec3 slot `index` of the `len`-float shared array param `name` on a
   * draw target: the flush writes `normalize(worldRotation * v)` there
   * and re-sends the whole array when any slot changes; unbound slots are
   * zeros. Every sink naming the same param shares one array (`len` must
   * agree); what the slots mean - light directions, an emitter axis - is
   * the caller's business, packed alongside its own non-spatial params.
   * Rebinding replaces the node's slot sink; the abandoned slot zeroes.
   */
  export function bindDirectionSlot(node: NodeId, target: TextureId, name: string, len: number, index: number, vector: Float32Array): void
  /** Remove the node's slot sink (its slot zeroes at the next flush). */
  export function unbindSlot(node: NodeId): void

  /**
   * Route the node's world pose to record slot `index` of vertex buffer
   * `buffer` used as an instance buffer: the flush writes the 5 floats
   * [x, y, angle, sx, sy] (world xy translation, rotation of the local x
   * axis in the world xy plane, xy scale with sy negated when the matrix
   * mirrors) at float offset index * 5. Writes batch: however many bound
   * nodes moved, each flush issues at most one coalesced write per
   * buffer, so producer-driven populations cost one buffer write per
   * frame. A hidden node's slot zeroes (zero scale collapses the
   * instance); so does an unbound or destroyed node's. Validated at bind
   * time: the buffer must exist and the slot must fit its byte size.
   * Rebinding replaces the node's record sink; the abandoned slot zeroes.
   */
  export function bindPoseRecord(node: NodeId, buffer: BufferId, index: number): void
  /** Remove the node's record sink (its slot zeroes at the next flush). */
  export function unbindRecord(node: NodeId): void
  /**
   * Move every record sink on buffer `old` to buffer `new`, slot indices
   * untouched: the growth swap. The whole used range republishes into
   * `new` at the next flush, so a population outgrowing its buffer swaps
   * in a larger one with one call and one bulk write instead of a
   * bindPoseRecord per node (pair it with the draw entry's own buffer
   * swap, setDraw's `instanceBuffers`). Throws when nothing is bound to
   * `old`, when `new` does not exist or cannot hold every bound slot, or
   * when `new` already carries record sinks.
   */
  export function retargetRecords(old: BufferId, next: BufferId): void
}
