// Meshes: the drawable nodes - plain, sprite and instanced - with their
// create-time state (geometry, material, per-mesh bindings) and the
// setter write paths that keep a live scene's draw entries in step. The
// scene side is reached through the node's SceneHooks (node.ts).

import { createBuffer, destroyBuffer, writeBuffer } from "@solidrt/core/gpu"
import type { BufferId, DrawId, InstanceAttribute, ShaderParams, TextureBindings, VertexAttribute } from "@solidrt/core/gpu"
import { geometryBounds, plane } from "./geometry.ts"
import type { Geometry } from "./geometry.ts"
import type { GeometryBuffers } from "./geometry-gpu.ts"
import type { Material } from "./material.ts"
import type { TransformUpdate, Vec3 } from "./math.ts"
import * as spatial from "flux:spatial"
import { enterScene, leaveScene, makeNode, remove, setTransform } from "./node.ts"
import type { SceneNode } from "./node.ts"

export type Mesh = SceneNode & {
  kind: "mesh"
  geometry: Geometry
  material: Material
  /** Explicit draw-order key (default 0), Three's name: lower draws first.
   * Sorts within the opaque group and within the transparent group; the
   * transparent group always follows the opaque one. Set with setRenderOrder. */
  renderOrder: number
  /** Draw into the scene's shadow map (default false, Three's default).
   * Set with setCastShadow. A casting instanced mesh is skipped (the
   * depth pass cannot know its record layout). */
  castShadow: boolean
  /** Layer membership bitmask (default 1, Three's `object.layers`): a
   * target draws the mesh when its mask intersects this. Not inherited
   * from ancestor Groups (Three's and Godot's rule). Set with setLayers. */
  layers: number
  /** Whether every target's frustum gates the mesh (default true, Three's
   * `frustumCulled`): outside it the entry draws nothing. Off for geometry
   * a vertex stage moves beyond its box (a fullscreen quad, a custom
   * displacement). Set with setCulling. */
  frustumCulled: boolean
  /** World units the frustum test grows the box by on every side (default
   * 0, Godot's `extra_cull_margin`): the pad for wind, wobble and any other
   * vertex-stage displacement that stays bounded. Set with setCulling. */
  cullMargin: number
  /** Joint nodes whose world boxes, united, stand in for this mesh's own
   * in the frustum test - a skinned part's cull box follows its pose.
   * null for an unskinned mesh. */
  _cullJoints: SceneNode[] | null
  _entry: DrawId | null
  /** The geometry-buffer reference the entry was built from, acquired at
   * attach and what _detach releases - like _transparent, a snapshot,
   * because setGeometry swaps mesh.geometry before the rebuild. */
  _buffers: GeometryBuffers | null
  /** material.transparent as of the last attach - the entry's actual
   * pipeline state, and what _detach counts against (setMaterial swaps
   * mesh.material before the rebuild). */
  _transparent: boolean
  /** World-space center of the local bounds, refreshed at sort time: the
   * transparent sort key. */
  _center: Vec3
  _params: ShaderParams | null
  /** Per-mesh sampler bindings merged over the material's at attach (a
   * skin's uBones palette texture): create-time state, set before the
   * mesh joins a scene, applied to entries drawn with the mesh's OWN
   * material or a skinned stand-in (Material.skinned - a skinned shadow
   * variant declares uBones) - any other override validates its bindings
   * against a program that may not declare these names. */
  _textures: TextureBindings | null
  /** Instance state when the mesh was made by createInstancedMesh; null on
   * an ordinary mesh. */
  _instances: MeshInstances | null
  /** True for a createSprite mesh: its quad faces the camera in the
   * vertex stage, so it picks by a unit box instead of its flat triangles. */
  _sprite: boolean
}

/** The per-mesh half of instancing: the record buffer and its bookkeeping.
 * Read the public fields freely; write through the population's own
 * functions (addInstance/removeInstance, setRecords/setRecordCount) so
 * the draw range follows. */
export type MeshInstances = {
  /** The GPU record buffer (instance slot 0), owned by the mesh
   * (disposeInstances frees it): the core-written matrices on an
   * instanced mesh, the app's records on a record mesh. */
  buffer: BufferId
  /** Floats per record: INSTANCE_FLOATS on an instanced mesh, the
   * material's slot-0 instanceAttributes summed on a record mesh. */
  stride: number
  /** Records the buffer has room for; doubles on growth (a replacement
   * buffer, never a resize). */
  capacity: number
  /** The buffer label, carried to replacement buffers on growth. */
  label: string | undefined
  /** Records currently drawn (the entry's instanceCount while visible):
   * the slot high-water mark on an instanced mesh, the records written
   * on a record mesh. */
  count: number
  /** Explicit LOCAL bounds covering the whole population ([minX, minY,
   * minZ, maxX, maxY, maxZ]) or null: the mesh node's own box, what the
   * frustum test and the transparent sort use, and on a record mesh the
   * only picking leaf (records are opaque data). An instanced mesh picks
   * per instance regardless and, without them, culls by its instances'
   * union (the scene's cull group over the live instance nodes). */
  bounds: Float32Array | null
  /** The node-backed population (createInstancedMesh): handles by slot
   * (null = free) and the free list; null on a record mesh. */
  nodes: InstanceSlots | null
  /** The app-written style records (instance slot 1) of an instanced mesh
   * whose material declares slot-1 attributes; null otherwise (and on a
   * record mesh, whose one buffer is the app's already). */
  style: InstanceStyle | null
}

export type InstanceSlots = { slots: (InstanceNode | null)[]; free: number[] }

/**
 * The style half of an instanced mesh: one record per instance slot of
 * the material's slot-1 instanceAttributes (a tint, a frame index, any
 * per-copy floats the vertex stage reads), owned by the app and written
 * per instance with setInstanceStyle - the JS twin of the core's matrix
 * records in slot 0, @solidrt/2d's pose/style split one dimension up.
 * Writes land in `data` and publish as ONE coalesced buffer write per
 * mesh at the scene's next sync, whatever the number of instances
 * styled; `buffer` grows with the matrix buffer.
 */
export type InstanceStyle = {
  /** The GPU style buffer (instance slot 1), mesh-owned like `buffer`. */
  buffer: BufferId
  /** Floats per style record: the material's slot-1 attributes summed. */
  stride: number
  /** The JS mirror, capacity * stride floats; the buffer is published
   * from it. Read freely, write through setInstanceStyle. */
  data: Float32Array
  /** The record a fresh (or recycled) slot starts with: the material's
   * instanceStyle, zeros without one. */
  blank: Float32Array
  /** The float range [lo, hi) of `data` written since the last publish,
   * or null. */
  dirty: [number, number] | null
}

/** A mesh from createInstancedMesh: one draw entry drawing the geometry
 * once per instance node (addInstance). */
export type InstancedMesh = Mesh & { _instances: MeshInstances & { nodes: InstanceSlots } }

/** A mesh from createRecordMesh: one draw entry drawing the geometry once
 * per record of a JS-written buffer (setRecords). */
export type RecordMesh = Mesh & { _instances: MeshInstances & { nodes: null } }

/**
 * One instance of an InstancedMesh: a scene node like any other
 * (setTransform, setTransition, setVisible, lookAt, worldPosition,
 * pointer handlers, children of its own) whose placement inside the mesh
 * the core writes into the mesh's record buffer at `_slot` - one
 * coalesced buffer write per flush however many instances moved. A
 * hidden instance draws nothing (its record collapses to zero scale).
 * Slot-bound to its mesh: created by addInstance, destroyed by
 * removeInstance; the generic add/remove reject it.
 */
export type InstanceNode = SceneNode & {
  kind: "instance"
  /** The owning mesh; null once removed (the handle is inert). */
  mesh: InstancedMesh | null
  /** The record slot, fixed for the instance's life. */
  readonly _slot: number
}

// Layer masks are 32-bit sets, Three's Object3D.layers width.
export function checkMask(mask: number, site: string): number {
  if (!Number.isInteger(mask) || mask < 0 || mask > 0xffffffff) {
    throw new Error(site + ": layers must be an integer bitmask in 0..0xffffffff, got " + mask)
  }
  return mask
}

export function createMesh(geometry: Geometry, material: Material): Mesh {
  let mesh = makeNode("mesh") as Mesh
  mesh.geometry = geometry
  mesh.material = material
  mesh.renderOrder = 0
  mesh.castShadow = false
  mesh.layers = 1
  mesh.frustumCulled = true
  mesh.cullMargin = 0
  mesh._cullJoints = null
  mesh._entry = null
  mesh._buffers = null
  mesh._transparent = false
  mesh._center = [0, 0, 0]
  mesh._params = null
  mesh._textures = null
  mesh._instances = null
  mesh._sprite = false
  return mesh
}

// Every sprite draws the same unit quad, built once: geometry is data
// and its GPU buffers are acquired per mesh, so one shared value is the
// normal sharing story. The box is the quad's reach at any facing: the
// unit quad's corners lie on a sphere of radius sqrt(0.5), so the box of
// that sphere holds it however the camera turns it - what the frustum
// test needs, and close enough for a pick.
let spriteQuad: Geometry | undefined
const SPRITE_REACH = Math.SQRT1_2
const SPRITE_BOUNDS = new Float32Array([-SPRITE_REACH, -SPRITE_REACH, -SPRITE_REACH, SPRITE_REACH, SPRITE_REACH, SPRITE_REACH])

/**
 * A camera-facing quad, Three's `Sprite`: a unit plane drawn with a
 * `sprite()` material (any material works, but only a sprite material
 * turns the quad; there is no `geometry` argument). Size it with `scale` -
 * a scale of [2, 1, 1] is a 2 x 1 world-unit quad - and place it like any
 * mesh; its rotation is ignored, the camera decides the facing. Picking
 * is by a unit box around the center (the quad's reach at any facing, an
 * approximation), so hits carry no normal/face/uv.
 */
export function createSprite(material: Material): Mesh {
  if (spriteQuad === undefined) spriteQuad = plane({ label: "sprite" })
  let mesh = createMesh(spriteQuad, material)
  mesh._sprite = true
  return mesh
}

/** The local box picking and sorting work from: explicit instance bounds
 * when the mesh is instanced (null without them - no leaf, no hits), the
 * unit box for a sprite, the geometry's own bounds otherwise. */
export function localBounds(mesh: Mesh): Float32Array | null {
  if (mesh._instances !== null) return mesh._instances.bounds
  return mesh._sprite ? SPRITE_BOUNDS : geometryBounds(mesh.geometry)
}

const ATTRIBUTE_FLOATS: Record<VertexAttribute["format"], number> = { f32: 1, vec2: 2, vec3: 3, vec4: 4 }

/** Floats per record of one instance slot of an attribute list (slot 0
 * by default; the list's `slot` keys pick the others). */
export function instanceStride(attributes: InstanceAttribute[], slot = 0): number {
  let stride = 0
  for (let a of attributes) if ((a.slot ?? 0) === slot) stride += ATTRIBUTE_FLOATS[a.format]
  return stride
}

// The highest instance slot an attribute list names.
function instanceSlots(attributes: InstanceAttribute[]): number {
  let top = 0
  for (let a of attributes) top = Math.max(top, a.slot ?? 0)
  return top + 1
}

/** Floats per instance record on an instanced mesh: one column-major
 * mat4, the instance's placement inside the mesh (the material reads it
 * through INSTANCE_MATRIX_ATTRIBUTES / INSTANCE_MATRIX in ./glsl). */
export const INSTANCE_FLOATS = 16
// Instance slots an instanced mesh reserves without a capacity; growth
// doubles past it.
const DEFAULT_CAPACITY = 64
// The instance buffers a mesh binds: the matrix slot and the style slot.
const MESH_INSTANCE_SLOTS = 2

/** What both population kinds take. */
type PopulationOptions = {
  /** LOCAL bounds covering the population ([minX, minY, minZ, maxX, maxY,
   * maxZ] - geometryBounds' shape), copied in: the mesh node's own box,
   * what the frustum test and the transparent sort use. Absent, an
   * instanced mesh culls by the union of its live instances' boxes (the
   * core follows them) and sorts by its node position; a record mesh is
   * never culled. */
  bounds?: ArrayLike<number>
  /** Debug label for the record buffer. */
  label?: string
}

export type InstancedMeshOptions = PopulationOptions & {
  /** Instance slots reserved up front (default 64). addInstance past it
   * doubles the buffer: a replacement whose live records the core moves
   * in one call and republishes at the next flush - amortized like a
   * dynamic array; reserve realistically to skip the copies. */
  capacity?: number
}

/** A record mesh's bounds are also its ONLY picking leaf: records are
 * opaque data, so without them the mesh never picks and pointer events
 * never target it. */
export type RecordMeshOptions = PopulationOptions

function copyBounds(bounds: ArrayLike<number> | undefined, site: string): Float32Array | null {
  if (bounds === undefined) return null
  if (bounds.length !== 6) throw new Error(site + ": bounds must be [minX, minY, minZ, maxX, maxY, maxZ]")
  let out = new Float32Array(6)
  for (let i = 0; i < 6; i++) out[i] = bounds[i]!
  return out
}

function instancedAttributes(material: Material, site: string): InstanceAttribute[] {
  let attributes = material.instanceAttributes
  if (attributes === undefined) {
    throw new Error(site + ": the material declares no instanceAttributes - build it with shaderMaterialClass({ instanceAttributes: [...] }) or a stock material's `instanced`")
  }
  return attributes
}

/**
 * Whether a material's instance attributes fit a mesh's population: the
 * slot-0 stride must be the mesh's record stride, a slot-1 layout must
 * match the mesh's style stride (a record mesh has no slot 1), and no
 * slot beyond. Throws otherwise - at creation for the mesh's own
 * material, at add() for a swapped one, at a shadow view's attach for a
 * shadow variant.
 */
export function checkInstancePairing(material: Material, inst: MeshInstances, site: string): void {
  let attrs = instancedAttributes(material, site)
  let slots = instanceSlots(attrs)
  if (slots > MESH_INSTANCE_SLOTS) {
    throw new Error(site + ": instance attributes name slot " + (slots - 1) + "; a mesh binds slots 0 (records) and 1 (style) only")
  }
  let stride = instanceStride(attrs, 0)
  if (stride !== inst.stride) {
    throw new Error(site + ": the material's slot-0 instance attributes take " + stride + " floats but the mesh's records are " + inst.stride)
  }
  let styleStride = instanceStride(attrs, 1)
  if (styleStride > 0) {
    if (inst.style === null) {
      throw new Error(site + ": the material declares slot-1 (style) instance attributes but the mesh has no style records" + (inst.nodes === null ? " (a record mesh is slot 0 only)" : ""))
    }
    if (styleStride !== inst.style.stride) {
      throw new Error(site + ": the material's slot-1 (style) attributes take " + styleStride + " floats but the mesh's style records are " + inst.style.stride)
    }
  }
}

/** The instance-buffer binding of a draw entry for a material over a
 * population: the plain slot-0 key, or both slots when the material's
 * pipeline reads the style records too (the two spellings are exclusive
 * and must match the pipeline's slot count exactly). */
export function instanceBinding(material: Material, inst: MeshInstances): { instanceBuffer?: BufferId; instanceBuffers?: BufferId[] } {
  let attrs = material.instanceAttributes
  if (attrs !== undefined && inst.style !== null && instanceSlots(attrs) > 1) return { instanceBuffers: [inst.buffer, inst.style.buffer] }
  return { instanceBuffer: inst.buffer }
}

/**
 * A mesh drawing `geometry` once per instance NODE: one draw entry, one
 * uModel placing the whole population, and N instances (addInstance)
 * that are scene nodes of their own, whose placement inside the mesh the
 * core writes into the record buffer - so native transitions, clip
 * players and picking reach every instance, and JS writes nothing per
 * frame. Three's InstancedMesh count constructor over Unity's
 * one-transform-per-instance model; for records only JS can compute at
 * scale, createRecordMesh is the raw form. The material must declare the
 * instance-matrix attributes in slot 0 (INSTANCE_MATRIX_ATTRIBUTES, read
 * through INSTANCE_MATRIX in ./glsl as `uModel * instanceMatrix()` - a
 * stock material's `instanced` does both); anything else throws here.
 * Slot-1 attributes, when the material declares any (a stock material's
 * `instanceColors`, a custom class's own layout), give every instance a
 * STYLE record beside its matrix: app-owned floats written with
 * setInstanceStyle, starting from the material's instanceStyle.
 *
 * The result is an ordinary Mesh: add/remove, setTransform (the whole
 * population), setVisible (hiding zeroes the drawn count, unhiding
 * restores it), setMeshParams and renderOrder all apply; disposeInstances
 * frees the record buffers when done for good.
 */
export function createInstancedMesh(geometry: Geometry, material: Material, opts?: InstancedMeshOptions): InstancedMesh {
  let attrs = instancedAttributes(material, "createInstancedMesh")
  let stride = instanceStride(attrs, 0)
  if (stride !== INSTANCE_FLOATS) {
    throw new Error(
      "createInstancedMesh: the material's slot-0 instanceAttributes take " + stride + " floats per record; an instance record is the " + INSTANCE_FLOATS + "-float matrix of INSTANCE_MATRIX_ATTRIBUTES",
    )
  }
  let capacity = opts?.capacity ?? DEFAULT_CAPACITY
  if (!(Number.isInteger(capacity) && capacity > 0)) {
    throw new Error("createInstancedMesh: capacity must be a positive integer, got " + capacity)
  }
  let styleStride = instanceStride(attrs, 1)
  let style: InstanceStyle | null = null
  if (styleStride > 0) {
    let blank = new Float32Array(styleStride)
    if (material.instanceStyle !== undefined) {
      if (material.instanceStyle.length !== styleStride) {
        throw new Error("createInstancedMesh: the material's instanceStyle has " + material.instanceStyle.length + " floats but its slot-1 attributes take " + styleStride)
      }
      blank.set(material.instanceStyle)
    }
    style = {
      buffer: createBuffer(capacity * styleStride * 4, { autoFree: false, label: opts?.label === undefined ? undefined : opts.label + "-style" }),
      stride: styleStride,
      data: new Float32Array(capacity * styleStride),
      blank,
      dirty: null,
    }
  }
  let mesh = createMesh(geometry, material) as InstancedMesh
  mesh._instances = {
    buffer: createBuffer(capacity * INSTANCE_FLOATS * 4, { autoFree: false, label: opts?.label }),
    stride: INSTANCE_FLOATS,
    capacity,
    label: opts?.label,
    count: 0,
    bounds: copyBounds(opts?.bounds, "createInstancedMesh"),
    nodes: { slots: [], free: [] },
    style,
  }
  checkInstancePairing(material, mesh._instances, "createInstancedMesh")
  return mesh
}

// Extend a style's dirty range over [lo, hi) floats of the mirror and
// ask the scene to publish it at the next sync.
function markStyle(mesh: InstancedMesh, style: InstanceStyle, lo: number, hi: number): void {
  if (style.dirty === null) style.dirty = [lo, hi]
  else {
    if (lo < style.dirty[0]) style.dirty[0] = lo
    if (hi > style.dirty[1]) style.dirty[1] = hi
  }
  mesh._scene?._setStyle(mesh)
}

/**
 * Write an instance's style record - the floats of the material's
 * slot-1 instance attributes, in order (`[r, g, b, a]` for a stock
 * material's instanceColors): the app's per-copy data beside the matrix
 * the core writes. Any number of instances styled between two frames
 * cost one coalesced buffer write at the scene's sync, so this is a
 * frame-rate path like setTransform. Throws on a material without
 * slot-1 attributes or a record of the wrong length; a no-op on a
 * removed instance.
 */
export function setInstanceStyle(instance: InstanceNode, values: ArrayLike<number>): void {
  let mesh = instance.mesh
  if (mesh === null) return
  let style = mesh._instances.style
  if (style === null) throw new Error("setInstanceStyle: the mesh's material declares no slot-1 (style) instance attributes")
  if (values.length !== style.stride) {
    throw new Error("setInstanceStyle: a style record is " + style.stride + " floats, got " + values.length)
  }
  let at = instance._slot * style.stride
  style.data.set(values, at)
  markStyle(mesh, style, at, at + style.stride)
}

/** Publish an instanced mesh's pending style writes as one buffer write
 * (the scene calls it from its sync; nothing to do without a dirty
 * range). */
export function publishInstanceStyle(mesh: InstancedMesh): void {
  let style = mesh._instances.style
  if (style === null || style.dirty === null) return
  let [lo, hi] = style.dirty
  style.dirty = null
  writeBuffer(style.buffer, style.data.subarray(lo, hi), lo * 4)
}

/**
 * Add an instance to an instanced mesh: a scene node placed by `update`
 * (position/rotation/quaternion/scale; absent keys at their identity)
 * under `parent` - the mesh itself by default, or a node inside the
 * mesh's subtree (a squad group within a fleet: the record stays
 * mesh-relative through it). Its record slot is fixed for its life (a
 * removed instance's slot recycles to the next add); past the
 * reservation the buffer doubles. From here on it is a node like any
 * other: setTransform/setTransition/setVisible, lookAt, worldPosition,
 * pointer handlers, children of its own (a headlight mesh under a car
 * instance) - and removeInstance, never remove.
 */
export function addInstance(mesh: InstancedMesh, update?: TransformUpdate, parent: SceneNode = mesh): InstanceNode {
  let inst: (MeshInstances & { nodes: InstanceSlots }) | null = mesh._instances
  if (inst === null) throw new Error("addInstance: the mesh's instances are disposed")
  for (let p: SceneNode | null = parent; p !== mesh; p = p.parent) {
    if (p === null) throw new Error("addInstance: parent must be the mesh or a node inside its subtree")
  }
  let slot = inst.nodes.free.pop() ?? inst.nodes.slots.length
  if (slot >= inst.capacity) growInstances(mesh, inst.capacity * 2)
  let instance = makeNode("instance") as InstanceNode
  instance.mesh = mesh
  ;(instance as { _slot: number })._slot = slot
  if (update !== undefined) setTransform(instance, update)
  inst.nodes.slots[slot] = instance
  instance.parent = parent
  parent.children.push(instance)
  if (slot >= inst.count) {
    inst.count = slot + 1
    mesh._scene?._setCount(mesh)
  }
  // A recycled slot must not wear its last occupant's style.
  let style = inst.style
  if (style !== null) {
    let at = slot * style.stride
    style.data.set(style.blank, at)
    markStyle(mesh, style, at, at + style.stride)
  }
  if (parent._scene) enterScene(instance, parent._scene)
  return instance
}

// Double the record buffers: replacements (never a resize) the live
// instances' record sinks move to in one core call, republished whole at
// the next flush, and the style mirror re-published whole from JS; the
// entry re-points, then the old buffers, which the entry held alive until
// now, are freed.
function growInstances(mesh: InstancedMesh, next: number): void {
  let inst = mesh._instances
  let previous = inst.buffer
  inst.buffer = createBuffer(next * INSTANCE_FLOATS * 4, { autoFree: false, label: inst.label })
  inst.capacity = next
  // Out of a scene nothing is bound, and retargeting an empty source throws.
  if (inst.nodes.slots.some(n => n !== null && n._node !== null)) spatial.retargetRecords(previous, inst.buffer)
  let style = inst.style
  let previousStyle: BufferId | null = null
  if (style !== null) {
    previousStyle = style.buffer
    style.buffer = createBuffer(next * style.stride * 4, { autoFree: false, label: inst.label === undefined ? undefined : inst.label + "-style" })
    let grown = new Float32Array(next * style.stride)
    grown.set(style.data)
    style.data = grown
    style.dirty = null
    markStyle(mesh, style, 0, inst.count * style.stride)
  }
  mesh._scene?._setBuffer(mesh)
  destroyBuffer(previous)
  if (previousStyle !== null) destroyBuffer(previousStyle)
}

/**
 * Destroy an instance: its slot hides at the next flush and recycles to
 * the next addInstance, and the handle goes inert (`mesh` null; later
 * writes are no-ops). Its children leave the scene with it and stay
 * parented to the dead node - remove() one to re-add it elsewhere. Remove
 * means destroy here, as for a 2d sprite: an instance cannot exist
 * outside its mesh.
 */
export function removeInstance(instance: InstanceNode): void {
  let mesh = instance.mesh
  if (mesh === null) return
  if (instance._scene) leaveScene(instance)
  let parent = instance.parent
  if (parent !== null) {
    let i = parent.children.indexOf(instance)
    if (i >= 0) parent.children.splice(i, 1)
    instance.parent = null
  }
  let nodes = mesh._instances.nodes
  nodes.slots[instance._slot] = null
  nodes.free.push(instance._slot)
  instance.mesh = null
}

/**
 * A mesh drawing `geometry` once per record of `records`: one draw entry,
 * one uModel write, N instances whose per-copy data is a few JS-written
 * floats - the raw escape hatch for motion only JS can compute at scale
 * (a particle sim, a crowd stepped in a worker), Three's InstancedMesh
 * and Godot's MultiMesh as plain data. The material must declare
 * `instanceAttributes` (shaderMaterialClass); its vertex stage reads each
 * record through those `in` variables. `records` is the interleaved
 * attribute data (stride = the attributes' floats summed) and is uploaded
 * here; its length is the buffer's initial capacity, which setRecords
 * grows past on demand. `count` limits how many records draw (default
 * all), up to capacity. Everything mesh applies as on an instanced mesh;
 * the population is written with setRecords, dialed with setRecordCount,
 * and its buffer freed with disposeInstances.
 */
export function createRecordMesh(
  geometry: Geometry,
  material: Material,
  records: Float32Array,
  count?: number,
  opts?: RecordMeshOptions,
): RecordMesh {
  let attrs = instancedAttributes(material, "createRecordMesh")
  let stride = instanceStride(attrs, 0)
  if (records.length % stride !== 0) {
    throw new Error("createRecordMesh: " + records.length + " floats is not a whole number of " + stride + "-float records")
  }
  let capacity = records.length / stride
  let mesh = createMesh(geometry, material) as RecordMesh
  mesh._instances = {
    buffer: createBuffer(records, { autoFree: false, label: opts?.label }),
    stride,
    capacity,
    label: opts?.label,
    count: Math.max(0, Math.min(Math.floor(count ?? capacity), capacity)),
    bounds: copyBounds(opts?.bounds, "createRecordMesh"),
    nodes: null,
    style: null,
  }
  checkInstancePairing(material, mesh._instances, "createRecordMesh")
  return mesh
}

/**
 * Overwrite a record mesh's records from the start of its buffer and (by
 * default) draw exactly the records written - pass `count` to draw fewer,
 * or to keep more previously written ones alive past a partial rewrite.
 * More records than the buffer holds grow it: capacity doubles (or jumps
 * to the records written when that is more), a new buffer is created and
 * written, the mesh's entry is re-pointed at it and the old buffer freed
 * - so a population grows without a new mesh, with the copies amortized
 * like any dynamic array (size the initial records to skip them).
 * Frame-rate-safe like setMeshParams when no growth happens.
 */
export function setRecords(mesh: RecordMesh, records: Float32Array, count?: number): void {
  let inst = mesh._instances
  if (records.length % inst.stride !== 0) {
    throw new Error("setRecords: " + records.length + " floats is not a whole number of " + inst.stride + "-float records")
  }
  let written = records.length / inst.stride
  if (written > inst.capacity) {
    let previous = inst.buffer
    inst.capacity = Math.max(written, inst.capacity * 2)
    inst.buffer = createBuffer(inst.capacity * inst.stride * 4, { autoFree: false, label: inst.label })
    mesh._scene?._setBuffer(mesh)
    destroyBuffer(previous)
  }
  writeBuffer(inst.buffer, records)
  setRecordCount(mesh, count ?? written)
}

/** Set how many records draw (clamped to [0, capacity]). The visibility
 * switch composes: a hidden mesh stores the count and draws it on unhide. */
export function setRecordCount(mesh: RecordMesh, count: number): void {
  let inst = mesh._instances
  let n = Math.max(0, Math.min(Math.floor(count), inst.capacity))
  if (n === inst.count) return
  inst.count = n
  mesh._scene?._setCount(mesh)
}

/**
 * Detach the mesh (if attached) and free its record buffers (the style
 * buffer with the matrix one). The buffers are mesh-owned with no
 * reference count (unlike geometry buffers they are never shared), so
 * this is the one explicit free; the mesh cannot be re-added afterwards.
 * An instanced mesh's instances go inert with it.
 */
export function disposeInstances(mesh: InstancedMesh | RecordMesh): void {
  let inst: MeshInstances | null = mesh._instances
  if (inst === null) return
  if (mesh._scene) remove(mesh)
  if (inst.nodes !== null) {
    for (let n of inst.nodes.slots) if (n !== null) n.mesh = null
    // The destroyed instance nodes' hiding writes land now, while the
    // buffer still exists (a write into a freed buffer warns).
    spatial.flush()
  }
  destroyBuffer(inst.buffer)
  if (inst.style !== null) destroyBuffer(inst.style.buffer)
  ;(mesh as Mesh)._instances = null
}

/** Set a mesh's explicit draw-order key (see Mesh.renderOrder). */
export function setRenderOrder(mesh: Mesh, order: number): void {
  if (mesh.renderOrder === order) return
  mesh.renderOrder = order
  mesh._scene?._reorder()
}

/**
 * Set a mesh's layer membership (bitmask, default 1). A target draws the
 * mesh when its mask intersects this: the scene's own mask (`layers` on
 * createScene, `scene.setLayers`), each view's (`layers` on createView,
 * `view.setLayers`); shadow views follow the scene's. A mesh masked out of
 * the scene is also skipped by pick()/raycast(), like an invisible one -
 * unless a raycast passes its own mask (QueryOptions.layers), which is
 * how an undrawn collision-only mesh stays queryable. `layers: 0` draws
 * nowhere. Not inherited from ancestor Groups.
 */
export function setLayers(mesh: Mesh, layers: number): void {
  checkMask(layers, "setLayers")
  if (mesh.layers === layers) return
  mesh.layers = layers
  mesh._scene?._setLayers(mesh)
}

/** Draw the mesh into the scene's shadow map, or stop (see Mesh.castShadow). */
export function setCastShadow(mesh: Mesh, cast: boolean): void {
  if (mesh.castShadow === cast) return
  mesh.castShadow = cast
  mesh._scene?._setCast(mesh)
}

/** Frustum culling per mesh: `frustumCulled` (default true) switches the
 * gate, `cullMargin` (world units, default 0) grows the tested box. */
export function setCulling(mesh: Mesh, options: { frustumCulled?: boolean; cullMargin?: number }): void {
  let culled = options.frustumCulled ?? mesh.frustumCulled
  let margin = options.cullMargin ?? mesh.cullMargin
  if (!(margin >= 0)) throw new Error("setCulling: cullMargin must be >= 0")
  if (culled === mesh.frustumCulled && margin === mesh.cullMargin) return
  mesh.frustumCulled = culled
  mesh.cullMargin = margin
  if (mesh._node !== null) {
    spatial.setCull(mesh._node, culled, margin)
    mesh._scene?._schedule()
  }
}

/** Swap a mesh's geometry: its draw entry is rebuilt (the scene re-sorts
 * the list, so the mesh keeps its place). */
export function setGeometry(mesh: Mesh, geometry: Geometry): void {
  if (mesh._sprite) throw new Error("setGeometry: a sprite draws the shared unit quad and takes no geometry")
  if (mesh.geometry === geometry) return
  mesh.geometry = geometry
  rebuildEntry(mesh)
}

/** Swap a mesh's material: its draw entry is rebuilt. */
export function setMaterial(mesh: Mesh, material: Material): void {
  if (mesh.material === material) return
  mesh.material = material
  rebuildEntry(mesh)
}

function rebuildEntry(mesh: Mesh): void {
  let scene = mesh._scene
  if (scene) {
    scene._detach(mesh)
    scene._attach(mesh)
  }
}

/**
 * Write per-mesh uniforms - the channel for a custom material's app-driven
 * values (a time, a per-object tint). Names must be
 * declared and used by the mesh's material shaders (unknown names throw at
 * the call site, the engine's validation contract). Values persist on the
 * mesh: they survive geometry/material entry rebuilds and re-apply then.
 * Also the frame-rate path - like setTransform, call it from onFrame
 * freely.
 */
export function setMeshParams(mesh: Mesh, params: ShaderParams): void {
  if (mesh._params === null) mesh._params = {}
  Object.assign(mesh._params, params)
  mesh._scene?._setParams(mesh, params)
}