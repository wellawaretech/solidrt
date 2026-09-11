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
  /** Free a node NOW, exit or no exit; its children become roots, except
   * leaving ones (exitNode), which go with it. A bound sink is dropped
   * without a write (removing the entry is the caller's job). On a leaving
   * node this is the cancel: it frees where it stands and still reports
   * through "spatialNodeFreed". */
  export function destroyNode(node: NodeId): void
  /**
   * Let go of a node the way its declaration says (the removal every
   * consumer's destroy verb rides on): each component with an `exit`
   * animates from where it is now to that value on the exit's own motion,
   * and the node is LEAVING meanwhile - still drawn and flushed, invisible
   * to raycast/overlap/sweep/moveAndSlide and every pick built on them,
   * refused by setParent on either side - until its exit tracks settle and
   * no leaving child remains under it, when it frees and one
   * "spatialNodeFreed" engine event (payload `{ node }`) reports it.
   * Children let go of first are those leaving children, so a subtree
   * torn down children-first frees root-last with each corpse in its
   * parent's frame to the end. Returns whether the node is now leaving:
   * with nothing to animate (no exit declared, every exit value already
   * held, nothing leaving below) it frees at once and returns false, so
   * the caller cleans up synchronously and no event follows. No
   * "spatialTransitionEnd" fires for an exit track; a second exitNode on
   * a leaving node changes nothing.
   */
  export function exitNode(node: NodeId): boolean
  /**
   * The node's lifecycle state, for probing: whether it is leaving, its
   * effective visibility as of the last flush, and the motion in force
   * per component - one entry per running track or held write, `to` the
   * target lanes, `heldUntil` the animation-clock ms a held write applies
   * at (null for a running track).
   */
  export function describeNode(node: NodeId): {
    leaving: boolean
    shown: boolean
    motion: { component: "position" | "rotation" | "scale"; to: number[]; heldUntil: number | null }[]
  }
  /** Re-parent (null = make a root). Throws on a cycle. */
  export function setParent(node: NodeId, parent: NodeId | null): void
  /** Replace the local transform (compare before calling; an unchanged
   * write still queues the node). Never consults or cancels transition
   * tracks: a running track overwrites a raw write at the next frame
   * (last write wins - the producer rule). */
  export function setTransform(node: NodeId, transform: Float32Array): void
  /**
   * One node-transition spec, the element `transition` vocabulary whole
   * minus stagger: `{ duration }` / `{ duration, bounce }` is a spring
   * (the default kind; retargets keep position and velocity, rotation
   * springs keep angular velocity along the geodesic), `{ duration, curve }`
   * a tween (rotation tweens slerp the geodesic; retargets restart from
   * the current value), or the shorthand string
   * `"<duration>ms [curve] [<delay>ms]"`. Durations and delays in ms; a
   * `delay` holds every write to the component that long on the animation
   * clock before it applies, and a held write that starts late (a hitch)
   * runs as if started on time.
   *
   * `from` and `exit` on a component entry (not on `all`) are its
   * lifecycle endpoints, in the lanes of the component - position and
   * scale `[x, y, z]`, rotation a quaternion `[x, y, z, w]` - each a bare
   * lane array (the entry's motion) or the endpoint object
   * `{ value, duration?, curve?, bounce?, delay? }` owning its direction's
   * motion (a field left out is the entry's; naming a curve or a bounce
   * decides the kind outright). `from` is the enter value: the node starts
   * there and animates to the transform it holds at its first frame - its
   * created transform, or the target of a write made in the creating
   * tick; an enter plays once per node, at creation, so the declaration
   * must be set in the creating tick (a later one animates writes only).
   * `exit` is where the component animates to when exitNode lets go of
   * the node.
   */
  export type NodeEndpoint = {
    value: number[]
    duration?: number
    curve?: "linear" | "ease" | "ease-in" | "ease-out" | "ease-in-out" | [number, number, number, number]
    bounce?: number
    delay?: number
  }
  export type NodeTransitionSpec =
    | { duration: number; bounce?: number; delay?: number; from?: number[] | NodeEndpoint; exit?: number[] | NodeEndpoint }
    | {
        duration: number
        curve: "linear" | "ease" | "ease-in" | "ease-out" | "ease-in-out" | [number, number, number, number]
        delay?: number
        from?: number[] | NodeEndpoint
        exit?: number[] | NodeEndpoint
      }
    | string
  /** The declaration setTransition takes: a spec per transform component
   * plus `all` as a catch-all (per-component entries win). `stagger`
   * (ms) makes the node a stagger group: every descendant enter (`from`)
   * or exit that begins in the same frame under it gets `index * stagger`
   * of extra delay, indexed in occurrence order - creation order for
   * enters, TREE order (children order, depth first) for exits whatever
   * order the exits were let go of in, so a cascade out reads like the
   * cascade in; enters and exits count separately. The nearest declaring
   * ancestor wins, nested groups never compound, and it orchestrates
   * descendants only: the node's own lifecycle is staggered by ITS
   * ancestors, and ordinary writes never stagger. */
  export interface NodeTransition {
    position?: NodeTransitionSpec
    rotation?: NodeTransitionSpec
    scale?: NodeTransitionSpec
    all?: NodeTransitionSpec
    stagger?: number
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
   * restarts the others (a held write's target counts, so re-sending it
   * does not restart its delay). Each settled component fires one
   * "spatialTransitionEnd" engine event (srt:events), payload
   * `{ node, component: "position" | "rotation" | "scale" }` - on a live
   * node; a leaving node's settles feed its free instead (exitNode).
   */
  export function writeTransform(node: NodeId, transform: Float32Array): void
  export function setVisible(node: NodeId, visible: boolean): void
  /**
   * Route the node's world matrix to one draw entry's `uModel` (and
   * `uNormal`, the inverse-transpose, when `normal`; and with `fade` the
   * LOD cross-fade band into `uLodFade`, see setLod). Validated like
   * setDrawParams: the entry must exist and declare those uniforms. The
   * entry is assumed switched off (instanceCount 0); the next flush turns it
   * on with `count` when the node is shown, and off again when hidden.
   * One draw sink PER TARGET: binding on a target the node already draws
   * into replaces that sink, binding on another target adds one - a mesh
   * drawn by a scene and by each of its views is one node with one flush.
   */
  export function bindDraw(node: NodeId, target: TextureId, draw: DrawId, normal: boolean, count: number, fade: boolean): void
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

  /** One hit of raycast(), nearest first. `face`/`uv` are present for
   * nodes with a shape (uv only when the shape has UVs); a node with
   * bounds but no shape is its local box's twelve triangles, so its hit
   * carries the struck face's normal and no face/uv. */
  export type Hit = {
    node: NodeId
    /** World units along the normalized ray. */
    distance: number
    point: [number, number, number]
    /** World-space geometric normal, facing the ray. */
    normal: [number, number, number]
    /** Triangle index into the shape's index list. */
    face?: number
    uv?: [number, number]
  }

  /** Set (null clears) the LOCAL tight box [minX, minY, minZ, maxX, maxY,
   * maxZ] of a node without a shape (a shaped node's box is its shape's:
   * throws). With one the node is in the picking index: its world box
   * follows the flush; hidden nodes stay in and are skipped at query time. */
  export function setBounds(node: NodeId, bounds: Float32Array | null): void
  /**
   * The clip volume gating every draw sink on `target`: its view-projection
   * (Float32Array of 16, column-major), or null to lift it. An entry whose
   * node box (grown by its cull margin) falls wholly outside reads
   * instanceCount 0 like a hidden node, and comes back with a fresh params
   * write. Nodes without a box, or with culling off, are never gated. Read
   * at flush: set it before the flush that should see it.
   */
  export function setFrustum(target: TextureId, viewProj: Float32Array | null): void
  /** Whether frustums gate the node's draw sinks (default true), and the
   * world-unit margin the test grows its box by (default 0). */
  export function setCull(node: NodeId, enabled: boolean, margin: number): void
  /** A local box for culling ONLY - the node stays out of the picking
   * index (setBounds is the indexed one). null falls back to its bounds. */
  export function setCullBounds(node: NodeId, bounds: Float32Array | null): void
  /** Cull the node by the union of these nodes' world boxes instead of
   * its own (a skinned mesh by its joints): an empty list restores its
   * own. Members without a box contribute nothing; with none at all the
   * node is not culled. */
  export function setCullGroup(node: NodeId, members: NodeId[]): void
  /** One level of a LOD group (setLod): the node drawn at this level (a
   * direct child of the group), absent for a population level (the
   * instance nodes under the group carry one record buffer per level,
   * bindMatrixRecord's list), and `size` - the projected size (the group's
   * bounding sphere's diameter as a fraction of the viewport height) below
   * which the level hands over to the next. */
  export type LodLevel = { node?: NodeId; size: number }
  /**
   * Make the node a LOD group (an empty list makes it plain again):
   * `levels` nearest first with strictly descending sizes, all naming a
   * child or none at all (never mixed); the last level's size is the cull
   * threshold, 0 for never culled. After the walk each flush measures the
   * group on every target with a LOD view (setLodView) - the sphere
   * around its own box, else around its levels' world boxes - and gates
   * the draw sinks under each level to the level that target picked, so
   * a thousand groups cost no JS per frame. `fade` widens each threshold
   * `s` into a cross-fade band `[s, s * (1 + fade))` in which both levels
   * draw with complementary screen-hash dithers (entries bound with
   * bindDraw's `fade`; others switch hard at the band's midpoint); 0 is a
   * hard switch with a one-sided hysteresis band, so a boundary never
   * flickers. `reference` is the target population levels measure by
   * (records are target-agnostic); node levels measure per target. A
   * nested group chains: a node draws only where every group above it
   * picked its level. A target with no LOD view draws the first level.
   */
  export function setLod(node: NodeId, levels: LodLevel[], fade: number, reference?: TextureId): void
  /**
   * What `target` measures projected size with: a Float32Array of 6 - the
   * eye position xyz, the projection's vertical focal factor (the
   * magnitude of `proj[5]`: `1 / tan(fov / 2)` for a perspective
   * projection, `2 / (top - bottom)` for an orthographic one - positive,
   * whatever clip flip the projection bakes in), 1 for orthographic else
   * 0, and a bias every
   * measured size is multiplied by (below 1 switches sooner) - or null to
   * lift it. Read at flush, like setFrustum; set it beside the frustum on
   * every camera move.
   */
  export function setLodView(target: TextureId, view: Float32Array | null): void
  /**
   * A geometry's positions as the core keeps them, one copy shared by
   * every node that references it: the source of those nodes' local
   * boxes, and the picking narrowphase's triangles when `indices` (a
   * Uint16Array/Uint32Array triangle list) is given - without it the
   * shape only gives its box. Positions are read from an interleaved
   * vertex array (`stride` floats per vertex, xyz at `posOffset`, uv at
   * `uvOffset`, -1 for none). Throws on out-of-range indices.
   */
  export function createShape(vertices: Float32Array, stride: number, posOffset: number, uvOffset: number, indices?: Uint16Array | Uint32Array): ShapeId
  /**
   * Rewrite the shape's vertices from `first` on with those in
   * `vertices`, laid out as for createShape (uvs exactly when the shape
   * has them). The box follows at the next flush on every node carrying
   * the shape; the triangle index is rebuilt by the next query. Throws
   * when the range runs past the shape's vertices.
   */
  export function updateShape(shape: ShapeId, vertices: Float32Array, stride: number, posOffset: number, uvOffset: number, first: number): void
  /** Free a shape; nodes still referencing it keep their last box and
   * fall back to it. */
  export function destroyShape(shape: ShapeId): void
  /** Give the node a shape (null takes it away): its box is the shape's
   * from here on, and setBounds is refused. A shape with no vertices
   * leaves the node without a box. */
  export function setShape(node: NodeId, shape: ShapeId | null): void
  /** The node's layer membership bitmask (default 1), what a query's
   * `layers` mask is tested against. Query-only: needs no flush. */
  export function setLayers(node: NodeId, layers: number): void
  /**
   * The filter every query takes as its trailing argument, each field
   * optional: `root` admits only the nodes under it (the caller's own
   * subtree in the arena every scene and layer shares), `layers` only
   * nodes whose mask intersects it (a node's mask of 0 then never
   * reports), `nodes` only the listed ones, `target` only the LOD levels
   * that draw target picks (setLod; without it every level reports). A
   * dead root throws.
   */
  export type QueryFilter = {
    root?: NodeId
    layers?: number
    nodes?: NodeId[]
    target?: TextureId
  }
  /** Every shown node with bounds the ray strikes, nearest first. The
   * direction need not be normalized; distances are world units. Reads
   * the index as of the last flush. */
  export function raycast(origin: Float32Array, direction: Float32Array, filter?: QueryFilter): Hit[]
  /** A query volume's layout for overlap/sweep: "capsule" is
   * 7 floats (a, b, radius; a sphere when a == b), "box" is 10 (center,
   * half extents, unit quaternion xyzw). */
  export type VolumeKind = "capsule" | "box"
  /** One node overlap() touches: its deepest contact - the point on
   * the node's surface, the unit direction out of it, and the depth along
   * that direction that clears the contact. */
  export type Overlap = {
    node: NodeId
    point: [number, number, number]
    normal: [number, number, number]
    depth: number
  }
  /** One node sweep() touches: `time` is the fraction of the motion
   * at first touch, `point` the touch point on the node's surface,
   * `normal` the unit normal there facing the volume. */
  export type Impact = {
    node: NodeId
    time: number
    point: [number, number, number]
    normal: [number, number, number]
  }
  /**
   * Every shown node with bounds the volume touches, each with its
   * deepest contact. A node with a shape is tested per triangle in world
   * space, so it holds under any transform; a node without one is its
   * local box's twelve triangles (a flat rect is four of them, so a
   * rotated sprite tests exactly - the 2d marquee's box query is a "box"
   * volume). Surfaces, not solids: a volume wholly inside a closed mesh
   * with no triangle in reach touches nothing. Unordered; reads the index
   * as of the last flush, like raycast.
   */
  export function overlap(kind: VolumeKind, volume: Float32Array, filter?: QueryFilter): Overlap[]
  /**
   * The volume moved by `motion` (a Float32Array of 3): every shown node
   * with bounds it touches on the way, at its first touch, earliest
   * first. A node already in contact reports time 0 while the motion
   * closes in, and nothing while it leaves or slides along the contact;
   * a zero motion touches nothing. Same testing and index contract as
   * overlap.
   */
  export function sweep(kind: VolumeKind, volume: Float32Array, motion: Float32Array, filter?: QueryFilter): Impact[]
  /** The mover's tuning, every field optional over the core's defaults:
   * `up` (the direction floors face, [0, 1, 0]), `floorMaxAngle` (radians
   * from `up` a contact still counts as floor, 45 degrees), `maxSlides`
   * (6), `skin` (the gap kept from every surface, 0.01) and `floorSnap`
   * (how far below a floor is pulled to when the motion does not rise,
   * 0.1; 0 disables). */
  export type MoveOptions = {
    up?: [number, number, number]
    floorMaxAngle?: number
    maxSlides?: number
    skin?: number
    floorSnap?: number
  }
  /** What moveAndSlide() returns: the displacement the body gets, the
   * unit normal of the floor it ends on (null when airborne or on a slope
   * too steep to stand on), whether a wall or a ceiling was met, and
   * every impact in order, the floor snap's last. */
  export type MoveResult = {
    motion: [number, number, number]
    floor: [number, number, number] | null
    wall: boolean
    ceiling: boolean
    hits: Impact[]
  }
  /**
   * Move the volume by `motion` through the admitted nodes, sliding along
   * what it hits (Godot's move_and_slide, Unity's CharacterController.Move
   * as one pure call): a push out of anything it starts inside, then
   * sweep, stop a skin short, slide the rest along the contact, up to
   * `maxSlides` times, then unless the motion rises a snap down onto a
   * floor within `floorSnap`. Same index and filter contract as sweep.
   */
  export function moveAndSlide(kind: VolumeKind, volume: Float32Array, motion: Float32Array, opts?: MoveOptions, filter?: QueryFilter): MoveResult

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
  /**
   * Route the node's world matrix to record slot `index` of vertex buffer
   * `buffer` as 16 floats (one column-major mat4) at float offset
   * index * 16: the per-instance model matrix a vertex stage reads as
   * four vec4 attributes. With `anchor` (an ANCESTOR of the node, one per
   * buffer - every bind on the buffer names the same one) the record is
   * the matrix RELATIVE to it, `inverse(anchorWorld) * world`, so a mesh's
   * instances stay in the mesh's own space and its uModel still places
   * the whole population (`uModel * instanceMatrix`); without one it is
   * the plain world matrix. Batching, growth (retargetRecords) and
   * rebinding as bindPoseRecord; a hidden, unbound or destroyed node's
   * slot becomes a zero-scale matrix (w stays 1), so the instance
   * collapses to a point and draws nothing. Every sink on one buffer
   * shares one projection. `buffers` names one buffer per LOD level of
   * the anchor's group (setLod with population levels), slot `index` in
   * each: the flush stages the record into the level the node's own
   * projected size picks on the group's reference target and the hidden
   * record into the others; one buffer is the plain population.
   */
  export function bindMatrixRecord(node: NodeId, buffers: BufferId[], index: number, anchor?: NodeId): void
  /** Remove the node's record sink (its slot hides at the next flush). */
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
  /**
   * Bind root motion to a player: `channel` of `clip` (a position channel
   * - normally the ROOT track of the authored clip, while the player
   * plays an in-place variant that holds the root still) is sampled at
   * the player's time each advance, and its travel since the previous
   * advance (continuous across a loop wrap, weighted by the player's
   * weight) is reported as one "spatialRootMotion" engine event
   * (srt:events) per advance, payload { player, x, y, z, yaw }, before
   * the frame's handlers. The translation is given in the root's CURRENT
   * facing (the clip's own turn so far undone), i.e. the character's
   * local frame. `rotation`, a rotation channel of the same clip (the
   * root's), adds the yaw half: the twist about `up`, in radians (zero
   * without it). With an `anchor` the translation, rotated by the
   * anchor's own rotation, is added to the anchor's position and the yaw
   * turns the anchor about `up` in its own frame - Unity's
   * applyRootMotion; without one it is Godot's root_motion_track, the
   * app's to apply. `opts.up` is the root's parent-space up axis (default
   * [0, 1, 0]); `opts.vertical: false` drops the component along it from
   * the delta (the height stays in the pose). Binding primes at the
   * player's current time, so the next advance already delivers. A
   * second call rebinds. Throws on a dropped player, a wrong channel
   * kind, a zero up axis, or a dead anchor.
   */
  export function bindRootMotion(player: PlayerId, clip: ClipId, channel: number, rotation?: number, anchor?: NodeId, opts?: { up?: [number, number, number]; vertical?: boolean }): void
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
