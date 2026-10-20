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
  /** A viewer key for a view with no target of its own (allocViewKey): a
   * 2d camera pushes its view under one. Counts up from a range the core
   * reserves above any texture id, so it never collides with a target. */
  export type ViewKey = number & { readonly __viewKey: unique symbol }
  /** A residency grid (createGrid). */
  export type GridId = number & { readonly __spatialGrid: unique symbol }

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
   * target lanes, `heldFor` the ms of a held write's hold still left on
   * the node's own time as of the last advance (null for a running
   * track).
   */
  export function describeNode(node: NodeId): {
    leaving: boolean
    shown: boolean
    motion: { component: NodeComponent; to: number[]; heldFor: number | null }[]
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
   * scale `[x, y, z]`, rotation a quaternion `[x, y, z, w]`, weights one
   * number per morph target - each a bare lane array (the entry's motion)
   * or the endpoint object
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
  /** A motion alone - a spec without lifecycle endpoints: what `all` and
   * a one-off motion on a write (writeWeights) take. */
  export type NodeMotionSpec =
    | { duration: number; bounce?: number; delay?: number }
    | {
        duration: number
        curve: "linear" | "ease" | "ease-in" | "ease-out" | "ease-in-out" | [number, number, number, number]
        delay?: number
      }
    | string
  /** The registers a node transition animates: the local TRS components
   * (writeTransform) and the weights register (writeWeights). */
  export type NodeComponent = "position" | "rotation" | "scale" | "weights"
  /** The declaration setTransition takes: a spec per node component
   * (the TRS components and `weights`, the morph target weights
   * register) plus `all` as a catch-all (per-component entries win). `stagger`
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
    weights?: NodeTransitionSpec
    all?: NodeMotionSpec
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
   * does not restart its delay). The core steps every track before the
   * frame's JS (beside the clip players), so a read in the frame sees
   * this frame's pose and a raw setTransform made in the frame wins the
   * frame. Each settled component fires one "spatialTransitionEnd"
   * engine event (sol:events), payload `{ node, component:
   * NodeComponent }`, ahead of the frame's JS - on a live node; a
   * leaving node's settles feed its free instead (exitNode).
   */
  export function writeTransform(node: NodeId, transform: Float32Array): void
  /**
   * Declare (null clears) the rate the native motion in the node's
   * subtree runs at: node transitions with their delays, enters, exits
   * and stagger slots, weights tracks, and the clip players clocked on a
   * node under it (createPlayer's `clock`). 0 freezes, 1 is app time,
   * other values are slow or fast motion. The nearest declaring ancestor
   * wins, never a product through nesting (Godot's process_mode shape): a
   * scene root at 0 freezes the world and a node inside it declaring 1
   * keeps running - a preview spinning on a pause screen. A frozen track
   * is not frame demand (a paused scene lets the loop idle; this write
   * requests the frame that resumes it). Writes while frozen behave as
   * always: undeclared components snap, declared ones retarget and wait;
   * a node let go of (exitNode) stays a ghost until its exit can play,
   * and a node created frozen sits at its `from`. What it does NOT
   * cover: the app's own frame handlers, timers, video and audio stay on
   * app time - app code gates itself, with `timeRate` as its reading.
   * Negative and non-finite values throw (reverse playback is a player's
   * `speed`). The dev clock (`/clock?scale=`) scales app time underneath
   * all of this.
   */
  export function setTimeScale(node: NodeId, scale: number | null): void
  /** The rate the node's motion runs at (setTimeScale): its own scale,
   * else the nearest declaring ancestor's, else 1. What JS-driven motion
   * inside a scaled subtree multiplies its own frame step by. */
  export function timeRate(node: NodeId): number
  export function setVisible(node: NodeId, visible: boolean): void
  /** What a draw sink writes and where it sorts (bindDraw). */
  export type BindDrawOptions = {
    /** The entry declares `uModel`: the flush writes the node's world
     * matrix there. Default true; false binds the entry for its visibility
     * switch and its sort key alone (a background pinned to a root). */
    params?: boolean
    /** Write `uNormal`, the world matrix's inverse-transpose, too. */
    normal?: boolean
    /** Write the LOD cross-fade band into `uLodFade` (see setLod). */
    fade?: boolean
    /** The instanceCount the entry is switched on with; default 1. */
    count?: number
    /** The sort queue on a sorted target (setDrawSort): 0 background,
     * 1 opaque (the default), 2 cutout, 3 transparent. */
    queue?: number
    /** Ascending above depth inside the queue; default 0. */
    renderOrder?: number
    /** Feed the entry's instance-order projected-key direction (the
     * `orderDirection` update of a position-key `instanceOrder`) from the
     * target's view: each flush whose view forward, mapped into the
     * node's model frame, turned past a small core-side epsilon writes
     * the new direction - on a retained order that re-sorts and
     * republishes the records core-side, so a whole population follows
     * the camera with no per-frame work here. For entries that declared
     * a position-key instanceOrder; on any other entry the first feed
     * fails (with a warning) and the feed stops, the sink stays. */
    orderFeed?: boolean
  }
  /**
   * Route the node's world matrix to one draw entry's `uModel` (and
   * `uNormal` and `uLodFade` when the options ask). Validated like
   * setDrawParams: the entry must exist and declare those uniforms. The
   * entry is assumed switched off (instanceCount 0); the next flush turns it
   * on with `count` when the node is shown, and off again when hidden.
   * One draw sink PER TARGET: binding on a target the node already draws
   * into replaces that sink, binding on another target adds one - a mesh
   * drawn by a scene and by each of its views is one node with one flush.
   * A node's draw sinks share one sort key: a bind keys every sink of
   * the node with its `queue` and `renderOrder`.
   */
  export function bindDraw(node: NodeId, target: TextureId, draw: DrawId, opts?: BindDrawOptions): void
  /** Re-key the node's draw sinks without a rebind (a renderOrder
   * change): the queue codes of BindDrawOptions, and the renderOrder
   * above depth inside the queue. Sorted targets re-sort at the next
   * flush. */
  export function setDrawKey(node: NodeId, queue: number, renderOrder: number): void
  /**
   * Whether the core orders the target's bound draw entries by queue:
   * background entries first whatever their depth, opaque entries
   * front-to-back by a logarithmic distance bucket of their world box
   * center (four buckets per doubling, so a small camera move changes
   * nothing), cutout entries (an alpha-tested fragment, no early-z) after
   * them the same way, transparent entries last back-to-front by exact
   * depth along the view, `renderOrder` above depth inside each queue,
   * bind order breaking ties.
   * Measured against the target's view (setView), so a target
   * without one waits for it. Every flush that bound, unbound, re-keyed or
   * moved an entry's node, or moved the view past the nearest opaque
   * center's bucket margin, re-keys and issues one setDrawOrder when the
   * permutation changed; entries the core does not bind draw first, before
   * the background queue. Off (the default) writes no order: a shadow tile, an
   * override-material view, a 2d target. Enabling a target already on
   * re-issues its order at the next flush: call it after adding an
   * unbound entry.
   */
  export function setDrawSort(target: TextureId, enabled: boolean): void
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
  /** The frame's start pass, core's to call after the frame callbacks and
   * their reactive flush: start the lifecycle motion they declared (the
   * enter of a node created in the frame from its `from`, the exit of one
   * let go of), so a read in the late pass of onBeforeRender sees the pose
   * the frame draws. Running motion steps before the callbacks instead.
   * Idempotent within a frame; requests a frame for what it started. */
  export function startTransitions(): void

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
   * What `target` sees: its view (world to camera) and projection
   * (camera to clip), two Float32Arrays of 16, column-major - or null to
   * forget the target (its view, LOD bias and LOD reference). The core
   * derives everything a target measures with from them: the clip volume
   * gating every draw sink on it (an entry whose node box, grown by its
   * cull margin, falls wholly outside reads instanceCount 0 like a hidden
   * node, and comes back with a fresh params write; nodes without a box,
   * or with culling off, are never gated), the LOD view (eye, forward,
   * vertical focal factor, orthographic or not - setLod) and the view the
   * draw sort measures depth against (setDrawSort), and the viewer the
   * residency gate tests regions against (setRegion, createGrid). Read at
   * flush: set it before the flush that should see it, on every camera
   * move. A target is a texture id, or a key from allocViewKey for a view
   * without a target (a 2d camera, whose composite pushes its
   * orthographic view and projection on camera change); a viewer that
   * stops (null) wants no region.
   */
  export function setView(target: TextureId | ViewKey, view: Float32Array, proj: Float32Array): void
  export function setView(target: TextureId | ViewKey, view: null): void
  /** A viewer key for setView, for a view with no target of its own. */
  export function allocViewKey(): ViewKey
  /** The hysteresis band of a residency region: wanted by a viewer while
   * within `enter` of its view volume (world units), kept until beyond
   * `leave` (`leave >= enter`, else throws), so a camera on a border never
   * thrashes. */
  export type RegionMargins = { enter: number; leave: number }
  /**
   * Make the node a residency region, or with null end it. The gate
   * beside culling and LOD that gates MEMORY: which regions of a world
   * should exist right now, per viewer, reported only on change. The
   * node's box (setCullBounds, else setBounds: a boxless node throws),
   * carried through its world matrix as the cull box is, is wanted by a
   * viewer while within `enter` of the viewer's view volume (every plane
   * pushed out by `enter`, the way a cull margin widens the test; a far
   * region in view is wanted) and stays wanted until beyond `leave`; it is
   * wanted overall if ANY viewer wants it (a minimap beside the main view,
   * a shadow camera, a split screen). The region moves with its parent
   * like any node; visibility is not consulted (a hidden region still
   * exists). One "spatialRegionEnter" engine event (payload `{ node }`)
   * when it becomes wanted, one "spatialRegionLeave" when it stops, both
   * delivered in the frame's event slot ahead of the frame's callbacks, so
   * a handler's creates land in the same frame. Leaves are immediate;
   * enters are paced (setResidencyBudget). Ending the region, or
   * destroying the node, reports a leave if its enter went out; a region
   * still pending reports nothing. What a region holds is its handler's:
   * the gate knows wanted or not, and a handler still loading when the
   * leave arrives cancels its own work.
   */
  export function setRegion(node: NodeId, margins: RegionMargins | null): void
  /**
   * A grid of regions: `origin` the world corner of cell [0, 0, 0], `cell`
   * the size per axis with 0 meaning unbounded along that axis (the index
   * is 0 there and the test ignores the axis: a 2d tile world's chunks are
   * `[chunkW, chunkH, 0]` in the layer's plane, a column grid over XZ
   * `[32, 0, 32]`, a voxel world `[32, 32, 32]`), `enter`/`leave` as
   * RegionMargins, `bounds` an optional inclusive cell-index box
   * `[minI, minJ, minK, maxI, maxJ, maxK]` outside which no cell exists,
   * `wrap` an optional period in cells per axis (0 = none; a torus:
   * indices modulo the period, the canonical cell reported once however
   * many images a viewer sees - the handler's composite places the
   * images), `weight` the handler's cost of one resident cell in a unit of
   * its choosing (a tile layer's chunk texels; default 1, 0 = free under
   * every limit), `limit` the grid's own cap on resident weight (see
   * setResidencyLimit for how a cap evicts), `label` the name the stats
   * group the grid's footprint under.
   */
  export type GridSpec = {
    origin: [number, number, number]
    cell: [number, number, number]
    enter: number
    leave: number
    bounds?: [number, number, number, number, number, number]
    wrap?: [number, number, number]
    weight?: number
    limit?: number
    label?: string
  }
  /**
   * Create a grid of regions (GridSpec); returns its id. A cell is wanted
   * by a viewer while within `enter` of the viewer's view volume and kept
   * until beyond `leave`, where "within m" means reaching into the
   * volume's side and near planes pushed out by m and lying no deeper
   * than m along the view direction - the far plane is a draw distance and
   * is not consulted, so the margins read as load distances in 3d and as
   * margins around an orthographic 2d view. A cell touching the margin
   * only does not count, so a camera aligned to the grid never loads the
   * ring beyond its edge. Cells have no state in the core until
   * approached: an unbounded or procedural world costs what it has near a
   * viewer. Events as setRegion's, payload `{ grid, cell: [i, j, k] }`.
   */
  export function createGrid(spec: GridSpec): GridId
  /** End a grid: every cell whose enter went out leaves. */
  export function destroyGrid(grid: GridId): void
  /** A grid's weight per resident cell (GridSpec.weight), re-weighed
   * against the limits at the next pass: a tile layer re-baking at a new
   * oversample reports its new chunk texel count here. */
  export function setGridWeight(grid: GridId, weight: number): void
  /**
   * The scene-wide cap on resident grid weight, null for none (default):
   * Unreal's streaming pool size in our costume, every grid's cells
   * weighed by their grid's `weight`, beside each grid's own `limit`.
   * Past a cap the most urgent pending cell displaces the least urgent
   * resident ones until it fits, urgency being the distance to the
   * nearest wanting eye in the grid's own `leave`s (dimensionless, so a
   * 2d layer in pixels and a 3d grid in metres compare); the eviction's
   * leave precedes the enter it made room for, so a handler's pool hands
   * the freed resources straight over. A cell nothing can make room for
   * is wanted and STALLED: no event, and no frame demanded for it until
   * a view moves. Explicit regions (setRegion) weigh nothing. A cap
   * counts resident cells, not what a handler put in them: a resident
   * chunk a tile layer holds no texture for still weighs a chunk.
   */
  export function setResidencyLimit(limit: number | null): void
  /**
   * The region enters delivered per frame scene-wide (default 16): every
   * region form and consumer spends the one budget, nearest to a viewer's
   * eye first, and the pending set re-sorts every frame, so a fast camera
   * streams what it is heading for and a region it turned away from never
   * enters (no event, nothing to cancel). Leaves are never budgeted. A
   * handler's own readiness (a chunk baked, a model parsed) is the
   * handler's; the budget bounds how many regions START per frame.
   */
  export function setResidencyBudget(enters: number): void
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
  /** An occluder's shape in node space: a solid box (min xyz, max xyz), a
   * `[width, height]` quad in the XY plane centered at the origin (two
   * sided), a solid sphere, or any triangle list (`indices` absent: the
   * positions in triples). Box and sphere assume a solid; a triangle
   * occludes exactly what lies behind it. */
  export type OccluderShape =
    | { box: Float32Array }
    | { quad: [number, number] }
    | { sphere: { center: [number, number, number]; radius: number } }
    | { mesh: { positions: Float32Array; indices?: Uint16Array | Uint32Array } }
  /** The node's occluder (null clears it): rasterized into every
   * occluding target's depth buffer under the node's world matrix while
   * the node is shown. A node never occludes itself. */
  export function setOccluder(node: NodeId, shape: OccluderShape | null): void
  /** Whether `target` tests its draw sinks against its occluders' depth
   * buffer (default off): a small CPU buffer of the shown occluders,
   * rebuilt when the target's view or an occluder moves, and a sink whose
   * box (grown by its cull margin) lies wholly behind it switches off like
   * a frustum-culled one. Conservative to the buffer's texel: a box at an
   * occluder's edge, or with a corner before the near plane, stays drawn;
   * a crack narrower than a texel occludes as if closed. */
  export function setOcclusion(target: TextureId | ViewKey, enabled: boolean): void
  /** Why a draw sink is off, or "on": the first gate that switched it off,
   * in test order (hidden, frustum, occluded, lod); "hidden" before the
   * first flush. */
  export type CullState = "on" | "hidden" | "frustum" | "occluded" | "lod"
  /** The node's draw sink on `target` as the last cull pass left it, null
   * without a sink there. */
  export function cullState(node: NodeId, target: TextureId | ViewKey): CullState | null
  export type OcclusionStats = {
    /** Targets with occlusion on. */
    targets: number
    /** Live nodes carrying an occluder. */
    occluders: number
    /** Occluder triangles the latest build of every target took (the
     * shown occluders' within the budget), how many of those reached the
     * raster (not clipped away whole; zero with every occluder behind the
     * camera), and the triangles the builds skipped past the budget. */
    triangles: number
    drawn: number
    skipped: number
    /** Buffer builds so far; the latest build's time and the slowest, ms. */
    builds: number
    buildMs: number
    buildMaxMs: number
    /** Draw sinks off because occluded, as of the last cull pass. */
    occluded: number
  }
  export function occlusionStats(): OcclusionStats
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
   * group on every target with a view (setView) - the sphere
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
  /** The bias every projected size measured on `target` is multiplied by
   * (default 1; below 1 switches levels sooner, a quality knob). Kept
   * across setView writes; a target measuring by a reference takes the
   * reference's. */
  export function setLodBias(target: TextureId, bias: number): void
  /** Whether `target` cross-fades LOD levels at all (default true): false
   * makes every group's band 0 there, a hard switch at each threshold (a
   * fading band draws both levels of a group - the cost a quality tier
   * turns off). Forgotten with the view; a change re-measures the target. */
  export function setLodFade(target: TextureId, enabled: boolean): void
  /** The level a LOD group (setLod) draws on `target`: the index into its
   * levels, the level count when culled past the last threshold, null
   * before the target measured it. Throws for a node without levels. A
   * population group's members pick per instance; this is the group's
   * own pick. */
  export function lodLevel(node: NodeId, target: TextureId): number | null
  /** Make `target` measure projected size by `source`'s view instead of
   * its own (null: its own again): a shadow tile culls by its light but
   * draws the level the scene camera sees, so its shadow matches. Only the
   * LOD measurement follows; culling and the draw sort stay the target's
   * own. Set once; a source view move re-measures the target. */
  export function setLodReference(target: TextureId, source: TextureId | null): void
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
  /** One channel of an interleaved float vertex array: `stride` floats
   * per vertex, the channel's floats at `offset` within each. */
  export type VertexChannel = { data: Float32Array; stride: number; offset: number }
  /**
   * Recompute vertex normals in place from a triangle list (a
   * Uint16Array/Uint32Array): positions are read from the `positions`
   * channel and unit normals written into the `normals` channel (its
   * `data` may be the same array when the layout interleaves them). Each
   * vertex sums the normals of the faces that name it, weighted by the
   * corner angle, so the result does not depend on how a surface was
   * triangulated; a vertex no face names gets zero. Throws on an index
   * past the last vertex or an offset outside its stride.
   */
  export function computeNormals(positions: VertexChannel, normals: VertexChannel, indices: Uint16Array | Uint32Array): void
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
   * Bind the node's WEIGHTS register (setWeights - a mesh's morph target
   * weights) to row `row` of `texture`: four weights per texel across the
   * texture's width, the register padded with zeros to the row (or cut
   * to it), one whole-texture upload per texture per flush like a
   * palette. Weights rows stage from the register, not the transform
   * walk, so a weights write costs no matrix recompute. A texture holds
   * one row kind - matrix rows (bindTextureSlot) or weights rows - never
   * both. Validated at bind time: an uploadable rgba32f texture with
   * `row` inside it. Rebinding the same texture replaces the node's slot
   * there; another texture adds one.
   */
  export function bindWeightsSlot(node: NodeId, texture: TextureId, row: number): void
  /** Remove the node's weights slot on `texture`, or every weights slot
   * without one (abandoned rows keep their last value). */
  export function unbindWeightsSlot(node: NodeId, texture?: TextureId): void
  /** Write the node's weights register, any length; an equal write is
   * free. Published through its weights slots at the next flush. Never
   * consults or cancels a weights track (setTransform's rule): a running
   * track overwrites it at the next frame. */
  export function setWeights(node: NodeId, weights: Float32Array): void
  /**
   * Write the weights register THROUGH a motion, the writeTransform of
   * the weights lane: with `transition` (a motion spec, the `all`
   * vocabulary: `{ duration, bounce? | curve, delay? }` or a shorthand
   * string) the write animates on it whatever the node declares - a
   * one-off motion for this write; without one the declaration's
   * `weights` entry (or `all`) decides, and with neither the write snaps
   * like setWeights. One lane per target: a write shorter or longer than
   * the register widens the track to the longer of the two, the missing
   * lanes at zero. A write matching the running track's target is left
   * alone (a tween never restarts on a re-send); each settle fires one
   * "spatialTransitionEnd" with component "weights". The clip players
   * write the same register (a weights channel, path code 3): both are
   * producers, last write wins at each frame.
   */
  export function writeWeights(node: NodeId, weights: Float32Array, transition?: NodeMotionSpec | null): void
  /** Fill `out` from the node's weights register as the arena holds it
   * (zeros past the register's length) and return the register's length. */
  export function readWeights(node: NodeId, out: Float32Array): number

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
  /** A rig's handle (createRig). */
  export type RigId = number
  /** A playing clip instance's handle (createPlayer). */
  export type PlayerId = number
  /** A bone mask's handle (createMask). */
  export type MaskId = number
  /** A sync group's handle (createGroup). */
  export type GroupId = number

  /**
   * Register a baked animation clip: `duration` in seconds, `meta` five
   * words per channel - [targetSlot, path (0 position, 1 rotation,
   * 2 scale, 3 weights, 4 curve), interpolation (0 step, 1 linear,
   * 2 cubic), keyCount, elements] - and `times`/`values` every channel's
   * key arrays concatenated in meta order (`elements` floats per key: 3
   * for position and scale, 4 for rotation, one per morph target for
   * weights, 1 for a curve - the TRS counts are checked; cubic stores
   * three elements per key: in-tangent, value, out-tangent, tangents per
   * second - the glTF CUBICSPLINE layout). A clip is shared data: a
   * joint channel's `targetSlot` indexes the JOINT TABLE of the rig a
   * player binds it to (createRig), so one clip drives any number of
   * instances and retargeting is another rig; a weights channel writes
   * the slot's node's weights register (setWeights); a curve channel's
   * slot indexes `curves`, the clip's curve names, and feeds the rig's
   * curve of that name (readCurve). Key times must ascend.
   */
  export function createClip(duration: number, meta: Uint32Array, times: Float32Array, values: Float32Array, curves?: string[]): ClipId
  /** Free a clip. Players still on it drop at their next advance (a
   * "dropped" spatialClipEnd). */
  export function destroyClip(clip: ClipId): void
  /**
   * Replace a clip's markers: `times` (seconds) with `durations` (0 for
   * an instant marker, else a window) and `names` alongside, one entry
   * each. The core sorts them by time; a "spatialClipMarker" event names
   * a marker by its index in that order. A player already on the clip
   * takes them from its next advance.
   */
  export function setClipMarkers(clip: ClipId, times: Float32Array, durations: Float32Array, names: string[]): void
  /**
   * The additive form of a clip: a NEW clip whose keys are deltas
   * against a reference, so that playing it on an additive layer over
   * the reference pose reproduces the authored clip - Three's
   * makeClipAdditive, Unity's additive reference pose, Godot's Sub2.
   * `{ kind: "first" }` takes the clip's own first frame, `{ kind:
   * "clip", clip, time }` another clip at a time (a channel it lacks
   * falls back to the first frame), `{ kind: "rest", rig }` the rig's
   * rest pose (a joint the rig lacks falls back likewise; a curve's
   * rest is 0). Positions and weights subtract, a rotation is
   * left-multiplied by the reference's inverse, a scale divides.
   * Markers and curve names carry over. Free it like any clip.
   */
  export function deriveAdditiveClip(clip: ClipId, reference: { kind: "first" } | { kind: "clip"; clip: ClipId; time: number } | { kind: "rest"; rig: RigId }): ClipId
  /**
   * Declare a node's rest pose - ten floats: position xyz, quaternion
   * xyzw, scale xyz - or null to clear it: what a rig over the node
   * composes its layers over (a stop-with-fade lands on it) and what a
   * spring chain on it turns from and settles to. A rig or chain made
   * before the declaration keeps the rest it resolved at its create (the
   * pose as it stood). A model declares its file's node transforms at
   * every scene enter. Throws on a dead node or a zero rotation.
   */
  export function setRest(node: NodeId, rest: Float32Array | null): void
  /**
   * Create a rig - one model's animation state, Unity's Animator,
   * Godot's AnimationMixer, Unreal's AnimInstance: `joints` the nodes
   * clip channels' target slots index (in order), `clock` the node whose
   * rate (setTimeScale) the rig's time, fades and root motion run at
   * (the model's root for a mixer). The layers compose over each joint's
   * REST pose: its setRest declaration, else the pose as it stands at
   * the create. The joints' morph weights as they stand are the rest
   * weights. Every node must be a live scene node (throws otherwise); a
   * later death drops the rig: one "spatialClipEnd" (reason "dropped")
   * per player, then "spatialRigEnd" `{ rig }`. The base layer (index 0,
   * override, weight 1, unmasked) exists from the start.
   *
   * Rigs advance on the frame clock BEFORE each frame's JS: every player
   * steps its time, each layer in order composes its players' blend over
   * the pose so far, and the fed joints are written into the arena, so
   * frame handlers read and may overwrite freshly posed nodes (last
   * write wins) and the frame's flush publishes the result. Per joint
   * and path, an override layer normalizes its players' weighted
   * contributions when they sum to one or more; below one the remainder
   * is what is UNDER the layer (the rest pose for the base), and a
   * player lacking a channel another player on the layer animates
   * contributes that same "under" for it; the result composes over the
   * pose so far by the layer weight times its mask's share for the
   * joint. An additive layer adds its players' weighted deltas (a delta
   * clip from deriveAdditiveClip: positions and weights add, rotations
   * compose, scales multiply) scaled the same way. A joint no player on
   * any layer animates is left alone.
   */
  export function createRig(clock: NodeId, joints: NodeId[]): RigId
  /** Remove a rig and its players at once (no events), the joints
   * holding whatever was last written. */
  export function destroyRig(rig: RigId): void
  /**
   * Append a layer above the rig's existing ones; returns its index.
   * `weight` 0..1 (default 1), `mode` "override" (default) or
   * "additive", `mask` a mask id (default none: every joint). A layer at
   * weight 0 costs no sampling (its players' time still runs).
   */
  export function createLayer(rig: RigId, opts?: { weight?: number; mode?: "override" | "additive"; mask?: MaskId }): number
  /** Write a layer's weight (ramped over `fadeMs` of the rig's time;
   * absent or 0 snaps) and mask (null clears). The base layer's weight
   * fades it toward rest. */
  export function setLayer(rig: RigId, index: number, update: { weight?: number; fadeMs?: number; mask?: MaskId | null }): void
  /** Remove a layer above the base and its players (no events); the
   * layers above move down one index. */
  export function destroyLayer(rig: RigId, index: number): void
  /**
   * Register a bone mask (Unity's AvatarMask, Godot's filters, Unreal's
   * blend masks): `shares[i]` (0..1) for `nodes[i]`, `defaultShare` for
   * every other node. Shared by any number of layers and players
   * (createLayer/createPlayer `mask`); a layer's and a player's shares
   * multiply. A mask is keyed by node, so one serves every clip of a
   * rig. setMask re-resolves every binding at the next advance.
   */
  export function createMask(defaultShare: number, nodes: NodeId[], shares: Float32Array): MaskId
  export function setMask(mask: MaskId, defaultShare: number, nodes: NodeId[], shares: Float32Array): void
  /** Free a mask; a layer or player still naming it blends unmasked. */
  export function destroyMask(mask: MaskId): void
  /**
   * Start playing `clip` on `rig`: `opts` (the setPlayer field names)
   * `layer` (default 0, the base), `speed` scales clip time (1 = as
   * authored), `loop` wraps (the default; a "spatialClipLoop" per wrap),
   * else the player holds its final pose and reports once; `weight`/
   * `fade` start the crossfade state (weight 0..1, default 1; fade =
   * weight change per second, default 0 - positive fades in, negative
   * out; past 0 the player is removed); `mask` a per-player mask on top
   * of the layer's; `window` `[start, end]` seconds the time stays
   * within (loops within, finishes at `end`); `fadeOut` seconds before a
   * non-looping player's end at which it starts fading out so it ends
   * on what is under it (then "finished" AND "dropped" are reported);
   * `time` the start time (default 0). Every joint channel must target
   * a slot the rig has. When a player finishes or is removed without
   * finishing (faded out, clip or rig destroyed), the "spatialClipEnd"
   * engine event (sol:events) fires with payload { player, reason:
   * "finished" | "dropped" }, before the same frame's handlers; a marker
   * crossing fires "spatialClipMarker" { player, marker (its index),
   * phase: "instant" | "begin" | "end", weight (the player's times its
   * layer's), layer } - once per crossing however long the frame, loop
   * wraps and reverse play included, a window's end also on a stop
   * inside it; a seek fires none of the instants it jumps over, ends
   * the windows it leaves and begins those it lands in.
   */
  export function createPlayer(rig: RigId, clip: ClipId, opts?: { layer?: number; speed?: number; loop?: boolean; weight?: number; fade?: number; mask?: MaskId; window?: [number, number]; fadeOut?: number; time?: number }): PlayerId
  /** Write the given fields of a player - the O(changes) crossfade
   * channel. Setting `time` (seconds, clamped to the window) also
   * re-arms a finished player's end report; `window` null clears the
   * subclip, `mask` null the mask. Returns the window changes a seek
   * made, at once, as `[marker, phase, ...]` pairs (phase 1 begin,
   * 2 end; a seek fires no instants) - the caller's to report, since
   * they are its own doing. Throws on an id that already dropped. */
  export function setPlayer(player: PlayerId, update: { weight?: number; fade?: number; speed?: number; time?: number; window?: [number, number] | null; mask?: MaskId | null }): number[]
  /** A player's current `[time, weight]` (seconds; the fade included). */
  export function readPlayer(player: PlayerId): [number, number]
  /** Whether a player is inside the window of marker `marker` (an index
   * in the clip's sorted markers) as of the last advance. */
  export function markerInside(player: PlayerId, marker: number): boolean
  /**
   * Bind root motion to a player: `channel` of `clip` (a position channel
   * - normally the ROOT track of the authored clip, while the player
   * plays an in-place variant that holds the root still) is sampled at
   * the player's time each advance, and its travel since the previous
   * advance (continuous across a loop wrap, weighted by the player's
   * EFFECTIVE weight: its own times its layer's times the root joint's
   * mask shares) is reported as one "spatialRootMotion" engine event
   * (sol:events) per advance, payload { player, x, y, z, yaw }, before
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
   * second call rebinds. Throws on a dropped player, a player on an
   * additive layer (a delta clip has no travel), a wrong channel kind,
   * a zero up axis, or a dead anchor.
   */
  export function bindRootMotion(player: PlayerId, clip: ClipId, channel: number, rotation?: number, anchor?: NodeId, opts?: { up?: [number, number, number]; vertical?: boolean }): void
  /** Remove a player at once, holding whatever pose it last wrote (no
   * end event; stop-with-fade is a setPlayer fade write instead).
   * Returns the markers whose windows the player was inside, which end
   * with it: the caller's to report. A dropped id is fine (empty). */
  export function destroyPlayer(player: PlayerId): number[]
  /**
   * Make a sync group of `players` on `rig` (Unreal's sync groups,
   * Unity's blend-tree time warp): every advance the heaviest member
   * leads and the others take its phase, so a walk and a run blended
   * half and half land their feet together. `sync` "time" (the default)
   * aligns normalized time within the members' windows; "markers"
   * aligns the markers common to every member's clip (matched by name
   * and order; a member whose sequence differs falls back to time). A
   * member's own speed scales the group while it leads. A player leaves
   * its previous group.
   */
  export function createGroup(rig: RigId, players: PlayerId[], sync?: "time" | "markers"): GroupId
  /** Replace a group's members. */
  export function setGroup(group: GroupId, players: PlayerId[]): void
  /** Dissolve a group; its members keep their own time from here. */
  export function destroyGroup(group: GroupId): void
  /**
   * Request an inertial transition (Unreal's inertialization): at the
   * next advance every joint the layers feed keeps the offset from the
   * pose the rig last wrote - with the velocity from the two previous
   * outputs - and decays it over `ms` of the rig's time on a quintic
   * that lands at zero with zero velocity, its duration capped so the
   * curve never overshoots. Call it in the frame of the change it
   * smooths (a player swapped at full weight, a stop, a layer switched
   * on, a seek); a request during a running transition re-captures
   * from the pose as it stands. One sample per frame, no averaging of
   * two cycles: the transition for locomotion.
   */
  export function inertialize(rig: RigId, ms: number): void
  /** A rig's curve (the clips' float tracks, blended like the pose) as
   * of the last advance, or undefined when no playing clip fed it this
   * frame. */
  export function readCurve(rig: RigId, name: string): number | undefined
  /**
   * Route a rig's curve into the scalar param `name` of draw entry
   * `draw` on `target` (no draw: a target-level param, a scene-wide
   * uniform): every flush after an advance that fed the curve writes
   * its value when it changed, the way a node's world matrix reaches
   * uModel - a clip drives a material's opacity or a light's intensity
   * with no JS per frame. A curve no clip on the rig carries yet binds
   * all the same and writes once one does; a second bind on the same
   * (curve, target, draw) replaces the name; the binding drops by itself
   * when the entry or target dies. Throws on an unknown rig.
   */
  export function bindCurveParam(rig: RigId, curve: string, target: TextureId, name: string, draw?: DrawId): void
  /** Remove a curve's binding on (target, draw); a missing one is fine. */
  export function unbindCurveParam(rig: RigId, curve: string, target: TextureId, draw?: DrawId): void
  /**
   * Fill `out` (a Float32Array of 10) with the node's CURRENT local
   * transform - position xyz, quaternion xyzw, scale xyz - as the arena
   * holds it, players' writes included. The pose read for root-motion
   * strips and skeleton copies: JS-side mirrors of a node animated by a
   * player go stale, this does not.
   */
  export function readTransform(node: NodeId, out: Float32Array): void

  /** A spring-bone chain's handle (createSpring). */
  export type SpringId = number
  /**
   * Declare a spring-bone chain: secondary motion (hair, a skirt, a
   * tail, a cable) solved in core - Unity's SpringBone, Godot's
   * SpringBoneSimulator3D, three-vrm's VRMSpringBoneJoint. `jointNodes`
   * lists the simulated bones PARENTS FIRST (a joint whose parent node is
   * another listed joint's must come after it; throws otherwise), and
   * `joints` carries 10 floats per joint in that order: the tail xyz in
   * the bone's own space (where the chain continues: the next joint's
   * rest position, or a leaf's virtual tail; must not be zero), then
   * VRM's settings -
   * stiffness (the pull back toward the rest direction), drag (0 none ..
   * 1 total), gravity power, gravity direction xyz (world space, any
   * length) and the tail's hit radius against colliders. `colliderNodes`
   * and `colliders` (9 floats each) declare the colliders the chain's
   * tails are kept out of: kind (0 sphere, 1 capsule, 2 plane), inside
   * (1 keeps the tails WITHIN a sphere or capsule), two points xyz in
   * the collider node's space (the sphere's offset and an unused triple;
   * the capsule's ends; the plane's offset and normal) and the radius.
   * Lengths follow the matrices: a scaled ancestor scales bone lengths,
   * hit radii and collider radii alike.
   *
   * The chain steps once per frame AFTER the frame's JS and its flush,
   * the pose post-pass (secondary motion reacts to the FINAL primary
   * pose: the players, the transitions and the frame's own writes - a
   * look-at, a controller's move), at `clock`'s rate (setTimeScale: 0
   * freezes it), in `center`'s space when given (the tails live there,
   * so the center's own motion does not swing them - VRM's center) and
   * in world space otherwise. The solve is frame-rate exact: the free
   * motion is the closed-form flow of three-vrm's 60 Hz step, so authored
   * values mean what they mean there and a 50, 60 or 120 Hz display
   * draws the same motion, and the length and collider projections run
   * at 240 Hz substeps. The chain OWNS its joints' rotations (a write in
   * the frame is overwritten at the step), and a read of one during the
   * frame (readTransform, worldMatrix) sees the previous frame's solve.
   * The tails start at rest under the pose as the tree stands at the
   * call (pending writes included), the rest rotations written. A chain
   * whose every tail is at rest asks for no frames; one whose clock,
   * center, joint or collider node dies drops silently. Every node must
   * be live (throws otherwise).
   */
  export function createSpring(clock: NodeId, jointNodes: NodeId[], joints: Float32Array, colliderNodes: NodeId[], colliders: Float32Array, center?: NodeId): SpringId
  /** Drop a chain, its joints put back at rest: the rest rotations
   * written (as resetSpring writes them), so the bones draw as authored
   * again. A dropped id is fine. */
  export function destroySpring(spring: SpringId): void
  /** Put the chain back at rest now: every tail along its rest direction
   * under the current pose, the rest rotations written, no motion carried
   * over (three-vrm's reset; what a teleport wants). Throws on a dropped
   * id. */
  export function resetSpring(spring: SpringId): void
  /**
   * Change a joint's AUTHORED settings - the fields given replace the
   * joint's, the others stay; the tail and the rest rotation are the
   * chain's structure and stay as created. A drag change re-derives the
   * joint's flow; nothing re-primes, the chain goes on from where it is
   * (three-vrm's settings object, Godot's per-joint setters). Throws on
   * a dropped id, a joint out of range or a setting out of range.
   */
  export function setSpringJoint(spring: SpringId, joint: number, update: { stiffness?: number; drag?: number; gravityPower?: number; gravityDir?: [number, number, number]; hitRadius?: number }): void
  /**
   * Set the chain's RUNTIME force: one vector (a Float32Array of 3, world
   * space, the units of gravity power along a direction) added to every
   * joint's authored gravity in the step - the wind, Godot's
   * external_force and UniVRM's ExternalForce, kept apart from the
   * asset's per-joint gravity so nothing authored is overwritten. Zero
   * until set; a constant force settles like gravity does. Throws on a
   * dropped id or a non-finite force.
   */
  export function setSpringForce(spring: SpringId, force: Float32Array): void

  /** An IK stack's handle (createIk). */
  export type IkId = number
  /** Where a solver aims: a live node (read at solve time, so a moving
   * target costs nothing), or a point in world space or in `space`'s
   * frame. */
  export type IkPoint = { node: NodeId } | { position: [number, number, number]; space?: NodeId }
  /** A target: a node (its position and rotation), or a point with an
   * optional rotation (xyzw) in the same space. */
  export type IkTarget = { node: NodeId } | { position: [number, number, number]; rotation?: [number, number, number, number]; space?: NodeId }
  /** A chain joint's limit, in the joint's own frame relative to its
   * primary rotation (radians): a hinge turns about `axis` within
   * min..max; a cone bounds the swing away from the bone axis (toward
   * the next joint) and the twist about it. */
  export type IkLimit = { kind: "hinge"; axis: [number, number, number]; min: number; max: number } | { kind: "cone"; swing: number; twist: number }
  /** A look-at: `bones` root first (spine, neck, head), each with its
   * `share` of the aim (fractions of what remains; the last takes all,
   * so the chain reaches the target exactly); `forward` and `upAxis` the
   * bones' own axes; `up` a roll reference (`{ vector, space? }`, else
   * the shortest arc); `limit` yaw and pitch `[min, max]` about the last
   * bone's rest forward with `damp` the fraction past which the approach
   * eases instead of clamping (1 = hard); `easeMs` the time-based
   * transition on a target switch, loss or flip through the back. */
  export type IkLookAtSpec = {
    kind: "lookAt"
    bones: { node: NodeId; share: number }[]
    target?: IkTarget | null
    forward: [number, number, number]
    upAxis: [number, number, number]
    up?: { vector: [number, number, number]; space?: NodeId }
    limit?: { yaw: [number, number]; pitch: [number, number]; damp: number }
    easeMs: number
    weight?: number
  }
  /** A two-bone limb: `root`, `mid`, `tip` a descendant path, solved in
   * closed form; `pole` the bend plane (else the animated bend, the rest
   * bend when straight), `poleAngle` a twist about the root-target axis
   * after it; `positionWeight` the reach, `rotationWeight` how much the
   * tip takes the target's rotation; `soft` the reach band as a fraction
   * of the limb (0 off) and `stretch` the most the limb lengthens past
   * full reach (0 off). */
  export type IkTwoBoneSpec = {
    kind: "twoBone"
    root: NodeId
    mid: NodeId
    tip: NodeId
    target?: IkTarget | null
    pole?: IkPoint
    poleAngle: number
    positionWeight: number
    rotationWeight: number
    soft: number
    stretch: number
    weight?: number
  }
  /** A chain from `root` to `tip` (every node between is a joint):
   * `method` fabrik (the chain is turned onto the target first, then
   * swept; a magnet turns the interior toward it and says which way a
   * chain lying on the target's axis folds) or ccd (limits exact), up to
   * `iterations` sweeps until the tip is within `tolerance` (world
   * units); `limits` per joint; `rotationWeight` the tip's. */
  export type IkChainSpec = {
    kind: "chain"
    root: NodeId
    tip: NodeId
    target?: IkTarget | null
    method: "fabrik" | "ccd"
    iterations: number
    tolerance: number
    magnet?: IkPoint
    limits?: { node: NodeId; limit: IkLimit }[]
    rotationWeight: number
    weight?: number
  }
  /** Foot placement: `legs` two-bone limbs under `hips`, their targets
   * GROUND HITS the core takes itself - a ray from `rise` above each
   * animated foot down to `drop` below it through `filter` (a collider
   * layer; the model's own meshes must not answer) - the sole `height`
   * below the tip (measured against the clock node's plane at creation
   * when absent), a foot planted while within `plantHeight` of its rest
   * height and lifting over `plantBand`, the hips moved along `up` by
   * the planted foot that needs it most (eased by `hipsEaseMs`), each
   * planted foot aligned to its ground normal by `rotationWeight`. */
  export type IkFeetSpec = {
    kind: "feet"
    hips: NodeId
    legs: { root: NodeId; mid: NodeId; tip: NodeId; height?: number }[]
    up: [number, number, number]
    filter?: QueryFilter
    rise: number
    drop: number
    plantHeight: number
    plantBand: number
    hipsEaseMs: number
    rotationWeight: number
    weight?: number
  }
  export type IkSolverSpec = IkLookAtSpec | IkTwoBoneSpec | IkChainSpec | IkFeetSpec
  /**
   * Declare an IK stack: the solvers in order, stepped once per frame
   * AFTER the frame's JS and its transitions and BEFORE the spring bones
   * (the pose post-pass: Unity's OnAnimatorIK before LateUpdate, Godot's
   * modifier stack), each over the previous one's result, at `clock`'s
   * rate (setTimeScale). Every frame each solver sees the PRIMARY pose -
   * what the players, the transitions and the frame's JS wrote - not its
   * own last result: a joint written over since the stack's last write
   * takes that write as its input, otherwise last frame's input stands,
   * so a static pose under a half-weight aim holds instead of creeping.
   * The stack OWNS its joints: a write in the frame is solved over, and
   * a read during the frame (readTransform, worldMatrix) sees the
   * previous frame's solve. Solves run in world space over matrices
   * composed on the fly, so a placed, turned or scaled model solves
   * identically and bone lengths follow the pose. `weight` (default 1)
   * blends each solver from the primary pose; 0 costs no solve. The
   * eases (weights, the look-at transition, the hips offset) are
   * per-second rates over the frame delta. A settled stack asks for no
   * frames; a stack whose clock dies drops, a solver whose joint or
   * target node dies is skipped from then on. Throws on a dead node, a
   * tip not under its root, a mid not between them, a setting out of
   * range.
   */
  export function createIk(clock: NodeId, solvers: IkSolverSpec[]): IkId
  /** Drop a stack, its joints put back to the primary pose (what the
   * players and the frame's JS wrote). A dropped id is fine. */
  export function destroyIk(ik: IkId): void
  /** Replace a solver's settings live: a full spec of the same kind
   * (any joints; a new chain re-registers). The weight and its ramp
   * carry over, the eases restart. Throws on a dropped id, an index out
   * of range, a kind change or a bad spec. */
  export function setIkSolver(ik: IkId, index: number, spec: IkSolverSpec): void
  /** Move a solver's target; null for none (a look-at eases back to the
   * primary pose). A point moved to another point tracks (feed a cursor
   * every frame); a node switch, a loss or a find starts the look-at's
   * transition. Throws for a feet solver (its targets are the ground). */
  export function setIkTarget(ik: IkId, index: number, target: IkTarget | null): void
  /** Ramp a solver's weight to `weight` over `fadeMs` of the clock's
   * time (linear, the players' fade shape); absent or 0 snaps. Unbinds
   * a curve (bindIkWeightCurve). */
  export function setIkWeight(ik: IkId, index: number, weight: number, fadeMs?: number): void
  /** Bind a solver's weight to a rig's curve (readCurve): every advance
   * the weight takes the curve's value as of that frame's players (an
   * authored foot-plant or grip track); a frame the curve is not fed
   * keeps the last weight. setIkWeight unbinds. */
  export function bindIkWeightCurve(ik: IkId, index: number, rig: RigId, name: string): void
  /** The last solve's residual: the distance left to the target (world
   * units; a look-at's angle left, radians; a feet solver's largest over
   * its planted feet). */
  export function readIkReach(ik: IkId, index: number): number
}
