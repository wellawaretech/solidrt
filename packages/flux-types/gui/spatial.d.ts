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
   * write still queues the node). Never consults or cancels transition
   * tracks: a running track overwrites a raw write at the next frame
   * (last write wins - the producer rule). */
  export function setTransform(node: NodeId, transform: Float32Array): void
  /**
   * One node-transition spec, the element `transition` vocabulary minus
   * the lifecycle conveniences: `{ duration }` / `{ duration, bounce }`
   * is a spring (the default kind; retargets keep position and velocity,
   * rotation springs keep angular velocity along the geodesic),
   * `{ duration, curve }` a tween (rotation tweens slerp the geodesic;
   * retargets restart from the current value), or the shorthand string
   * `"<duration>ms [curve]"`. Durations in ms; no delay, from or exit.
   */
  export type NodeTransitionSpec =
    | { duration: number; bounce?: number }
    | { duration: number; curve: "linear" | "ease" | "ease-in" | "ease-out" | "ease-in-out" | [number, number, number, number] }
    | string
  /** The declaration setTransition takes: a spec per transform component
   * plus `all` as a catch-all (per-component entries win). */
  export interface NodeTransition {
    position?: NodeTransitionSpec
    rotation?: NodeTransitionSpec
    scale?: NodeTransitionSpec
    all?: NodeTransitionSpec
  }
  /**
   * Declare (or with null clear) the node's transitions: with a config
   * set, writeTransform animates instead of snapping. A bare string is
   * the `all` catch-all. Clearing cancels the node's running tracks in
   * place - it keeps its mid-flight transform, no settled events fire,
   * and later writes snap. Replacing a config affects future writes only.
   */
  export function setTransition(node: NodeId, transition: NodeTransition | string | null): void
  /**
   * Replace the local transform THROUGH the transition declaration: a
   * declared component animates toward the written value (the write is a
   * target), an undeclared one snaps. Without a declaration this is
   * setTransform. A component matching its running track's target is
   * left alone, so rewriting the whole array to move one component never
   * restarts the others. Each settled component fires one
   * "spatialTransitionEnd" engine event (srt:events), payload
   * `{ node, component: "position" | "rotation" | "scale" }`.
   */
  export function writeTransform(node: NodeId, transform: Float32Array): void
  export function setVisible(node: NodeId, visible: boolean): void
  /**
   * Route the node's world matrix to one draw entry's `uModel` (and
   * `uNormal`, the inverse-transpose, when `normal`). Validated like
   * setDrawParams: the entry must exist and declare those uniforms. The
   * entry is assumed switched off (instanceCount 0); the next flush turns it
   * on with `count` when the node is shown, and off again when hidden.
   * One draw sink PER TARGET: binding on a target the node already draws
   * into replaces that sink, binding on another target adds one - a mesh
   * drawn by a scene and by each of its views is one node with one flush.
   */
  export function bindDraw(node: NodeId, target: TextureId, draw: DrawId, normal: boolean, count: number): void
  /** Remove the node's draw sink on `target`, or every draw sink without
   * one. Issues no write: the entries are the caller's to remove. */
  export function unbindDraw(node: NodeId, target?: TextureId): void
  /** Change every bound entry's "on" count (an instanced mesh's record
   * count); written at once to the entries currently on. */
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
   * One slot sink per (target, name): rebinding the same param on the
   * same target replaces that sink (the abandoned slot zeroes); another
   * param or another target adds one, so a node may feed several arrays
   * of one target (a spot light: its direction and its position).
   */
  export function bindDirectionSlot(node: NodeId, target: TextureId, name: string, len: number, index: number, vector: Float32Array): void
  /**
   * Route the node's world POSITION into vec3 slot `index` of the
   * `len`-float shared array param `name` on a draw target -
   * bindDirectionSlot's translation sibling, with the same sharing,
   * zeroing and per-(target, name) replacement rules. What a positional
   * light's slot follows.
   */
  export function bindPositionSlot(node: NodeId, target: TextureId, name: string, len: number, index: number): void
  /** Remove the node's slot sinks on `target`, or every slot sink without
   * one (the abandoned slots zero at the next flush). */
  export function unbindSlot(node: NodeId, target?: TextureId): void

  /**
   * Route the node's world matrix, post-multiplied by the constant `post`
   * (a Float32Array of 16, column-major), into row `row` of a float
   * texture: the flush writes the 16 floats as the row's four rgba32f
   * texels - the matrix-palette channel a vertex shader `texelFetch`es
   * (a skin binds each joint node with `post` its inverse bind). Writes
   * batch: however many bound nodes moved, each flush uploads at most one
   * whole palette per texture. With `anchor` - one node shared by every
   * bind on the texture, and an ANCESTOR of every bound node - rows are
   * anchor-local (`inverse(anchorWorld) * world * post`), so a model
   * root's placement stays out of its own palette. Rows keep updating
   * while nodes are hidden (visibility is the drawing entry's business),
   * and an unbound or destroyed node's row keeps its last value. Validated
   * at bind time: the texture must be an uploadable rgba32f texture 4
   * texels wide with `row` inside it. Rebinding the same texture replaces
   * the node's slot there; another texture adds one.
   */
  export function bindTextureSlot(node: NodeId, texture: TextureId, row: number, post: Float32Array, anchor?: NodeId): void
  /** Remove the node's texture slot on `texture`, or every texture slot
   * without one (abandoned rows keep their last value). */
  export function unbindTextureSlot(node: NodeId, texture?: TextureId): void

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

  /** A registered clip's handle (createClip). */
  export type ClipId = number
  /** A playing clip instance's handle (createPlayer). */
  export type PlayerId = number

  /**
   * Register a baked animation clip: `duration` in seconds, `meta` four
   * words per channel - [targetSlot, path (0 position, 1 rotation,
   * 2 scale), interpolation (0 step, 1 linear, 2 cubic), keyCount] - and
   * `times`/`values` every channel's key arrays concatenated in meta
   * order (3 floats per key, 4 for rotation; cubic stores three elements
   * per key: in-tangent, value, out-tangent, tangents per second - the
   * glTF CUBICSPLINE layout). A clip is shared data: `targetSlot`
   * indexes the target table each PLAYER supplies, so one clip drives
   * any number of instances, and retargeting is a different table.
   */
  export function createClip(duration: number, meta: Uint32Array, times: Float32Array, values: Float32Array): ClipId
  /** Free a clip. Players still on it drop at their next advance (a
   * "dropped" spatialClipEnd). */
  export function destroyClip(clip: ClipId): void
  /**
   * Start playing `clip`: `targets[slot]` is the node each channel
   * animates - every target must be a live scene node (throws
   * otherwise). Players advance on the frame clock BEFORE each frame's
   * JS, sample and weight-blend every active player per (node, path) -
   * two players on one node crossfade - and write the blended TRS into
   * the arena, so frame handlers read and may overwrite freshly posed
   * nodes (last write wins) and the frame's flush publishes the result.
   * `speed` scales clip time (1 = as authored); `loop` wraps, else the
   * player holds its final pose and reports once; `weight`/`fade` start
   * the crossfade state (weight 0..1, fade = weight change per second -
   * positive fades in, negative out; past 0 the player is removed).
   * When a player finishes or is removed without finishing (faded out,
   * clip or target destroyed), the "spatialClipEnd" engine event
   * (srt:events) fires with payload { player, reason: "finished" |
   * "dropped" }, before the same frame's handlers.
   */
  export function createPlayer(clip: ClipId, targets: NodeId[], speed: number, loop: boolean, weight: number, fade: number): PlayerId
  /** Write the given fields of a player - the O(changes) crossfade
   * channel. Setting `time` (seconds) also re-arms a finished player's
   * end report. Throws on an id that already dropped. */
  export function setPlayer(player: PlayerId, update: { weight?: number; fade?: number; speed?: number; time?: number }): void
  /** Remove a player at once, holding whatever pose it last wrote (no
   * event; stop-with-fade is a setPlayer fade write instead). A dropped
   * id is fine. */
  export function destroyPlayer(player: PlayerId): void
  /**
   * Fill `out` (a Float32Array of 10) with the node's CURRENT local
   * transform - position xyz, quaternion xyzw, scale xyz - as the arena
   * holds it, players' writes included. The pose read for root-motion
   * strips and skeleton copies: JS-side mirrors of a node animated by a
   * player go stale, this does not.
   */
  export function readTransform(node: NodeId, out: Float32Array): void
}
