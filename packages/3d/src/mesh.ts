// Meshes: the drawable nodes - plain, sprite and instanced - with their
// create-time state (geometry, material, per-mesh bindings) and the
// setter write paths that keep a live scene's draw entries in step. The
// scene side is reached through the node's SceneHooks (node.ts).

import { beginBufferWrite, createBuffer, destroyBuffer, destroyTexture, endBufferWrite, transferRecords, writeBuffer } from "@solidrt/core/gpu"
import type { BufferId, DrawId, ShaderParams, TextureBindings, TextureId, VertexAttribute, VertexBufferLayout } from "@solidrt/core/gpu"
import { checkCubeKnobs } from "./environment.ts"
import type { EnvironmentOptions } from "./environment.ts"
import { geometryBounds, isFloatLayout, layoutKey, layoutStride, plane, vertexBytes, vertexView, VERTEX_FORMATS } from "./geometry.ts"
import type { AttributeAccess, Geometry } from "./geometry.ts"
import { INSTANCE_MATRIX_ATTRIBUTES } from "./glsl.ts"
import type { GeometryBuffers } from "./geometry-gpu.ts"
import type { Material } from "./material.ts"
import type { TransformUpdate } from "./math.ts"
import * as spatial from "flux:spatial"
import { activateMorph, afterFree, createWeightsTexture, enterScene, leaveScene, makeNode, rebindMorph, remove, resetMorph, setTransform } from "./node.ts"
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
   * depth pass cannot know its record layout), and so is any geometry
   * that is not a triangle list: lines and points cast nothing. */
  castShadow: boolean
  /** Layer membership bitmask (default 1, Three's `object.layers`): a
   * target draws the mesh when its mask intersects this. Not inherited
   * from ancestor Groups (Three's and Godot's rule). Set with setLayers. */
  layers: number
  /** This mesh's environment instead of the scene's (resolved; set with
   * setEnvironment): the cube its image-lit and reflective terms sample,
   * `null` for NONE (a sealed interior under an outdoor sky reflects and
   * receives no sky), `undefined` (the default) to follow the scene's,
   * live through scene.setEnvironment. Applied where the material
   * reflects at all (Material.env - `standard`, `phong` with
   * `reflectivity`, a custom source composing ENVIRONMENT); on any other
   * material it is a no-op, so an interior can be masked wholesale. */
  environment: { cube: TextureId; intensity: number; rotation: number } | null | undefined
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
  /** The index range the entries draw, or null for the whole index list.
   * Set with setDrawRange; reset by setGeometry. */
  _range: { first: number; count: number } | null
  /** The geometry-buffer reference the entry was built from, acquired at
   * attach and what _detach releases - a snapshot, because setGeometry
   * swaps mesh.geometry before the rebuild. */
  _buffers: GeometryBuffers | null
  _params: ShaderParams | null
  /** Per-mesh sampler bindings merged over the material's at attach (a
   * skin's uBones palette texture): create-time state, set before the
   * mesh joins a scene, applied to entries drawn with the mesh's OWN
   * material or a skinned stand-in (Material.skinned - a skinned shadow
   * variant declares uBones) - any other override validates its bindings
   * against a program that may not declare these names. */
  _textures: TextureBindings | null
  /** The node whose weights register drives this mesh's morph targets
   * (its weights texture is bound as uMorphWeights): the mesh itself for
   * a standalone mesh whose geometry carries targets, the glTF node's
   * group for a model part - one write and one clip track for every part
   * of that mesh. null for geometry without targets. */
  _morphOwner: SceneNode | null
  /** Instance state when the mesh was made by createInstancedMesh; null on
   * an ordinary mesh. */
  _instances: MeshInstances | null
  /** True for a createSprite mesh: its quad faces the camera in the
   * vertex stage, so it picks by a unit box instead of its flat triangles. */
  _sprite: boolean
}

/** The per-mesh half of instancing: the population's bookkeeping, the
 * core-written matrix buffer of an instanced mesh and the app-owned
 * record streams. Read the public fields freely; write through the
 * population's own functions (addInstance/destroy, setRecords/
 * setRecordCount, instanceAttribute + updateRecords) so the draw range
 * and the publishes follow. */
export type MeshInstances = {
  /** The GPU matrix buffer of an instanced mesh - the material's first
   * instance buffer, INSTANCE_MATRIX_ATTRIBUTES - which the core writes
   * each instance node's placement into (JS never does; disposeInstances
   * frees it). null on a record mesh, whose every buffer is a stream. */
  matrix: BufferId | null
  /** The app-owned record streams: one per instance buffer the mesh's
   * material declared at creation, in declaration order, the matrix
   * buffer excluded (so an instanced mesh's style record is streams[0],
   * a record mesh's records are). */
  streams: InstanceStream[]
  /** Records every buffer has room for; doubles on growth (replacements,
   * never a resize). */
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
  /** The node the instance records are relative to when it is not the
   * mesh itself (InstancedMeshOptions.anchor): an ancestor the mesh sits
   * under at identity, whose subtree the instances may live anywhere in.
   * null on a plain population and on a record mesh. */
  anchor: SceneNode | null
  /** The level meshes of an instanced LOD (createInstancedLod), nearest
   * first - the first is the population itself, the rest its children -
   * sharing this population's instance slots, each with a matrix buffer
   * of its own the core stages an instance into by its projected size.
   * null on a plain population. */
  levels: InstancedMesh[] | null
  /** The population's morph weights texture when the geometry carries
   * targets (an instanced mesh; a record mesh has no nodes to own
   * weights): one row per record slot, `capacity` rows, each instance
   * node's register published into its slot's row (setMorphWeights on
   * the instance), what a morphing entry binds as uMorphWeights and the
   * shader reads at row gl_InstanceID. Replaced on growth (the entries
   * re-point), freed by disposeInstances. */
  morph: { texture: TextureId; rows: number } | null
  /** The mesh's instanceOrder resolved against its material's instance
   * layouts (see InstanceOrderOptions), or null for buffer order. Fixed
   * at creation; slots as the app writes them never move. */
  order: ResolvedInstanceOrder | null
  /** Whether the records were transferred to the engine at creation
   * (RecordMeshOptions.transfer, or an indexed instanceOrder): no JS-side
   * mirror exists, the population is write-once, and rewrites/growth/
   * attribute access throw. Fixed at creation; always false on an
   * instanced mesh. */
  transfer: boolean
  /** Records per drawn instance: 1, or on an indexed instanceOrder the
   * ids per id-stream record (the entry's instanceCount is
   * ceil(count / group), see drawCount). Fixed at creation. */
  group: number
  /** Data textures the population owns (an indexed population's record
   * texture and the like, created by whoever built the mesh), freed by
   * disposeInstances with the buffers. */
  textures: TextureId[]
}

/** The draw's instance count for a population: its records over the
 * records per instance, rounded up (an indexed population's last group
 * pads with the engine's sentinel ids). */
export function drawCount(inst: MeshInstances): number {
  return Math.ceil(inst.count / inst.group)
}

export type InstanceSlots = { slots: (InstanceNode | null)[]; free: number[] }

/**
 * One app-owned per-instance buffer of a populated mesh: a vertex stream
 * stepped per instance, Geometry.streams one step up. `layout` is the
 * attribute list the material declared for it (any vertex format,
 * tightly packed), `data` the JS mirror of `capacity` records - a
 * Float32Array over an all-float layout, a Uint8Array otherwise
 * (vertexView's rule) - and `buffer` the mesh-owned GPU copy. Writes go
 * through instanceAttribute's accessor (or setInstanceStyle / setRecords,
 * which encode for you), land in the mirror, and publish as ONE
 * coalesced buffer write per stream at the scene's next sync over the
 * `dirty` byte range - a frame-rate path, like setTransform. Growth
 * replaces `data` and `buffer` (the accessor follows; a held `data`
 * view does not).
 */
export type InstanceStream = {
  layout: VertexAttribute[]
  /** Bytes per record: the layout's stride. */
  stride: number
  buffer: BufferId
  data: ArrayBufferView
  /** The record a fresh (or recycled) instance slot starts with: the
   * material's instanceStyle encoded, on the first stream of an
   * instanced mesh; zeros otherwise. */
  blank: Uint8Array
  /** The byte range [lo, hi) of `data` written since the last publish,
   * or null. */
  dirty: [number, number] | null
  /** The DataView the accessors read through; follows `data`. */
  _view: DataView
  /** All-float32 layout: `data` is a Float32Array and a record write is
   * plain indexed stores instead of a codec pass per component. */
  _floats: boolean
  /** Values per record: the layout's components summed. */
  _components: number
  /** One accessor per attribute, in layout order. */
  _fields: AttributeAccess[]
}

/** A mesh from createInstancedMesh: one draw entry drawing the geometry
 * once per instance node (addInstance). */
export type InstancedMesh = Mesh & { _instances: MeshInstances & { matrix: BufferId; nodes: InstanceSlots } }

/** A mesh from createRecordMesh: one draw entry drawing the geometry once
 * per record of a JS-written stream (setRecords). */
export type RecordMesh = Mesh & { _instances: MeshInstances & { matrix: null; nodes: null } }

/**
 * One instance of an InstancedMesh: a scene node like any other
 * (setTransform, setTransition, setVisible, lookAt, worldPosition,
 * pointer handlers, children of its own) whose placement inside the mesh
 * the core writes into the mesh's record buffer at `_slot` - one
 * coalesced buffer write per flush however many instances moved. A
 * hidden instance draws nothing (its record collapses to zero scale).
 * Slot-bound to its mesh: created by addInstance, destroyed by
 * destroy; the generic add/remove reject it.
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
  mesh.environment = undefined
  mesh.frustumCulled = true
  mesh.cullMargin = 0
  mesh._cullJoints = null
  mesh._entry = null
  mesh._range = null
  mesh._buffers = null
  mesh._params = null
  mesh._textures = null
  mesh._morphOwner = geometry.morphs === undefined ? null : mesh
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
  uncullFlooredSprite(mesh)
  return mesh
}

/**
 * The screen-size clamp a sprite mesh draws with, `[minPx, maxPx]` (0 =
 * off): the mesh's own `uScreenPx` param over its material's (a sprite
 * material always carries one). null when neither bound is on, or the
 * material is not a sprite material.
 */
export function spriteScreenPx(mesh: Mesh): [number, number] | null {
  let px = mesh._params?.uScreenPx ?? mesh.material.params.uScreenPx
  if (px === undefined || typeof px === "number") return null
  let min = px[0] ?? 0
  let max = px[1] ?? 0
  return min > 0 || max > 0 ? [min, max] : null
}

// A floored sprite draws larger than its unit box far from the camera,
// so the frustum test would hide it while its quad is on screen: a floor
// coming on (at creation, a material swap, a params override) switches
// the mesh's culling off. Only ever off - an app that wants it back says
// so with setCulling.
function uncullFlooredSprite(mesh: Mesh): void {
  if (!mesh._sprite || !mesh.frustumCulled) return
  let px = spriteScreenPx(mesh)
  if (px !== null && px[0] > 0) setCulling(mesh, { frustumCulled: false })
}

/** The local box picking and sorting work from: explicit instance bounds
 * when the mesh is instanced (null without them - no leaf, no hits), the
 * unit box for a sprite, the geometry's own bounds otherwise. */
export function localBounds(mesh: Mesh): Float32Array | null {
  if (mesh._instances !== null) return mesh._instances.bounds
  return mesh._sprite ? SPRITE_BOUNDS : geometryBounds(mesh.geometry)
}

/** Floats per instance record on an instanced mesh: one column-major
 * mat4, the instance's placement inside the mesh (the material reads it
 * through INSTANCE_MATRIX_ATTRIBUTES / INSTANCE_MATRIX in ./glsl). */
export const INSTANCE_FLOATS = 16
const INSTANCE_BYTES = INSTANCE_FLOATS * Float32Array.BYTES_PER_ELEMENT
// The matrix layout by key: what an instanced material's first buffer
// must be, and what binds the core's buffer in any material's list.
const MATRIX_KEY = layoutKey(INSTANCE_MATRIX_ATTRIBUTES)
// Instance slots an instanced mesh reserves without a capacity; growth
// doubles past it.
const DEFAULT_CAPACITY = 64

// Values per record of a layout: its attributes' components summed - what
// setInstanceStyle and a material's instanceStyle count in.
function layoutComponents(layout: VertexAttribute[]): number {
  let n = 0
  for (let a of layout) n += VERTEX_FORMATS[a.format].components
  return n
}

// Records in a byte view of `layout`: 4-aligned and a whole number of
// strides, or a throw naming `site`.
function recordCount(records: ArrayBufferView, layout: VertexAttribute[], site: string): number {
  let stride = layoutStride(layout)
  if (records.byteOffset % Float32Array.BYTES_PER_ELEMENT !== 0) throw new Error(site + ": records must start on a 4-byte boundary")
  if (records.byteLength % stride !== 0) {
    throw new Error(site + ": " + records.byteLength + " record bytes is not a whole number of " + stride + "-byte (" + layoutKey(layout) + ") records")
  }
  return records.byteLength / stride
}

// The accessors of a stream's attributes, reading through the stream's
// current view so they survive growth.
function streamFields(stream: InstanceStream): AttributeAccess[] {
  let offset = 0
  return stream.layout.map(attr => {
    let codec = VERTEX_FORMATS[attr.format]
    let at = offset
    offset += codec.bytes
    return {
      format: attr.format,
      components: codec.components,
      get: (i, k) => codec.get(stream._view, i * stream.stride + at, k),
      set: (i, k, v) => codec.set(stream._view, i * stream.stride + at, k, v),
    }
  })
}

// Point a stream's mirror at `bytes` (a fresh ArrayBuffer of whole
// records): the handed-out view and the accessors' view.
function viewStream(stream: InstanceStream, bytes: ArrayBuffer): void {
  stream.data = vertexView(stream.layout, bytes)
  stream._view = new DataView(bytes)
}

function streamLabel(label: string | undefined, index: number): string | undefined {
  return label === undefined ? undefined : label + "-" + index
}

// A stream of `capacity` zeroed records over `layout`; its GPU buffer is
// sized and unwritten (the mirror is the truth, publishes follow dirty).
function makeStream(layout: VertexAttribute[], capacity: number, blank: Uint8Array | null, label: string | undefined): InstanceStream {
  let stride = layoutStride(layout)
  let stream: InstanceStream = {
    layout,
    stride,
    buffer: createBuffer(capacity * stride, { autoFree: false, label }),
    data: new Uint8Array(0),
    blank: blank ?? new Uint8Array(stride),
    dirty: null,
    _view: new DataView(new ArrayBuffer(0)),
    _floats: isFloatLayout(layout),
    _components: layoutComponents(layout),
    _fields: [],
  }
  viewStream(stream, new ArrayBuffer(capacity * stride))
  stream._fields = streamFields(stream)
  return stream
}

// The indexed order's stream: the id buffer the core writes whole with the
// sorted record indices (ceil(capacity / group) instance records of
// `group` u32 ids each; it starts zeroed and the entry's attach seeds the
// first sort), the records handed to the core's order mirror as the key
// source. No JS mirror exists, as under transfer.
function makeIndexStream(layout: VertexAttribute[], capacity: number, group: number, records: Uint8Array, label: string | undefined): InstanceStream {
  let stride = layoutStride(layout)
  let stream: InstanceStream = {
    layout,
    stride,
    buffer: createBuffer(Math.ceil(capacity / group) * stride, { autoFree: false, label }),
    data: new Uint8Array(0),
    blank: new Uint8Array(stride),
    dirty: null,
    _view: new DataView(new ArrayBuffer(0)),
    _floats: false,
    _components: layoutComponents(layout),
    _fields: [],
  }
  transferRecords(stream.buffer, records)
  return stream
}

// The transfer form's stream: records go to the engine at creation and no
// JS mirror exists (data stays a zero-length view; the operations that
// would read or write it throw on the transfer flag first). An ordered
// stream hands the records to the core's order mirror - the entry's
// attach publishes the first gather, so the buffer starts zeroed; an
// unordered one is its own upload, the buffer created with the records.
function makeTransferredStream(layout: VertexAttribute[], records: Uint8Array, ordered: boolean, label: string | undefined): InstanceStream {
  let stride = layoutStride(layout)
  let stream: InstanceStream = {
    layout,
    stride,
    buffer: ordered
      ? createBuffer(records.byteLength, { autoFree: false, label })
      : createBuffer(records, { autoFree: false, label }),
    data: new Uint8Array(0),
    blank: new Uint8Array(stride),
    dirty: null,
    _view: new DataView(new ArrayBuffer(0)),
    _floats: isFloatLayout(layout),
    _components: layoutComponents(layout),
    _fields: [],
  }
  if (ordered) transferRecords(stream.buffer, records)
  return stream
}

// Encode `values` (one per component, in layout order, as the shader
// sees them) into record `i` of a stream: one typed-array set over an
// all-float layout (the values ARE the bytes), the codecs otherwise.
function writeRecord(stream: InstanceStream, i: number, values: ArrayLike<number>, site: string): void {
  let n = stream._components
  if (values.length !== n) throw new Error(site + ": a " + layoutKey(stream.layout) + " record is " + n + " values, got " + values.length)
  if (stream._floats) {
    // Indexed stores beat TypedArray.set over a plain array under QuickJS,
    // which walks the array-like generically.
    let data = stream.data as Float32Array
    let at = i * n
    for (let k = 0; k < n; k++) data[at + k] = values[k]!
    return
  }
  let j = 0
  for (let f of stream._fields) for (let k = 0; k < f.components; k++) f.set(i, k, values[j++]!)
}

// One record of `layout` encoded from `values`, the bytes a blank slot copies.
function encodeRecord(layout: VertexAttribute[], values: ArrayLike<number>, site: string): Uint8Array {
  let stride = layoutStride(layout)
  let one: InstanceStream = { layout, stride, buffer: 0 as BufferId, data: new Uint8Array(0), blank: new Uint8Array(0), dirty: null, _view: new DataView(new ArrayBuffer(0)), _floats: isFloatLayout(layout), _components: layoutComponents(layout), _fields: [] }
  viewStream(one, new ArrayBuffer(stride))
  one._fields = streamFields(one)
  writeRecord(one, 0, values, site)
  return vertexBytes(one.data)
}

// Extend a stream's dirty range over [lo, hi) bytes of the mirror and ask
// the scene to publish it at the next sync.
function markRecords(mesh: Mesh, stream: InstanceStream, lo: number, hi: number): void {
  if (stream.dirty === null) stream.dirty = [lo, hi]
  else {
    if (lo < stream.dirty[0]) stream.dirty[0] = lo
    if (hi > stream.dirty[1]) stream.dirty[1] = hi
  }
  mesh._scene?._setRecords(mesh)
}

/**
 * Draw a population's records in key order, decided core-side - the
 * engine's instance order riding the mesh's record streams
 * (createInstancedMesh and createRecordMesh both take it). Record slots
 * never move: the app keeps writing slot i forever, and the engine
 * gathers the records into key order as they publish.
 *
 * `position` names an instance attribute (float32x3 or float32x4) whose
 * first three floats are a position in the mesh's local space: records
 * draw by their depth along the scene camera's view direction, which the
 * core feeds itself whenever the camera or the mesh turns meaningfully -
 * zero per-frame JS, and a parked camera costs nothing. `descending:
 * true` draws farthest first, the transparent blend order. The order
 * follows the scene's OWN camera: one buffer holds one order, so views
 * draw the same records in that order, and a mesh masked out of the
 * scene target keeps its last order. `retain: true` opts a write-once
 * population (a splat cloud) into the engine's retained copy: a camera
 * turn re-sorts and republishes core-side with no publish from here, and
 * an order-preserving turn uploads nothing.
 *
 * `field` names a float32 instance attribute: records draw by that value
 * as the app wrote it (an age, an explicit sort key), re-ordered when
 * the records publish.
 *
 * An ordered mesh gives up partial record publishes: any write sends the
 * live record set whole, gathered (a byte range has no stable position
 * under a permutation).
 *
 * The INDEXED form (`records`) is for a population whose records live in
 * a data texture the shader fetches by record index (a splat cloud):
 * `records` is the layout of the records createRecordMesh received -
 * `position` names a float32x3/x4 attribute IN THAT LAYOUT - and the
 * material's one instance buffer is the ID STREAM, uint32/uint32x4
 * attributes the engine fills with the sorted record indices, K ids per
 * instance record so one drawn instance covers K records (the
 * geometry repeats K times; `gl_VertexID` picks the id). The records go
 * to the engine at creation (transfer implied, retain by nature) as the
 * sort's key source; the app uploads the same bytes as its texture. A
 * re-sort then uploads 4 bytes per record instead of the record, and
 * the last instance's group past `count` carries 0xFFFFFFFF, which the
 * shader culls before fetching.
 */
export type InstanceOrderOptions =
  | (({ position: string; retain?: boolean } | { field: string }) & {
      /** Largest key first - back-to-front for a position key. Default false. */
      descending?: boolean
    })
  | {
      /** The position attribute of the record layout below. */
      position: string
      /** The layout of the handed-off records the shader fetches by
       * index; the material's instance buffer is the id stream. */
      records: VertexAttribute[]
      /** Largest key first - back-to-front. Default false. */
      descending?: boolean
    }

/** The instanceOrder resolved against the material's instance layouts at
 * creation: the key stream's layout key (the entry's pipeline buffer is
 * found by it at attach, exactly as buffer bindings are), the key's
 * float offset inside one record, and the declared knobs. */
export type ResolvedInstanceOrder = {
  key: string
  floats: number
  position: boolean
  descending: boolean
  retain: boolean
  /** The index materialization (InstanceOrderOptions' `records` form):
   * the handed-off record's byte stride; null for gathered records. */
  indices: { stride: number } | null
}

// Resolve an instanceOrder option against the material's instance
// layouts: the named attribute must exist in one of them, hold floats of
// the key's shape, and start on a 4-byte boundary of its record. The
// indexed form resolves the key in the given record layout instead, and
// the material's one instance buffer must be whole u32 ids.
function resolveInstanceOrder(order: InstanceOrderOptions | undefined, buffers: VertexBufferLayout[], site: string): ResolvedInstanceOrder | null {
  if (order === undefined) return null
  if ("records" in order) {
    if (buffers.length !== 1) {
      throw new Error(site + ": an indexed instanceOrder takes a material with one instance buffer (the id stream); this one declares " + buffers.length)
    }
    let ids = buffers[0]!.attributes
    for (let a of ids) {
      if (!a.format.startsWith("uint32")) throw new Error(site + ": the id stream's attribute '" + a.name + "' must be uint32, uint32x2, uint32x3 or uint32x4 (whole record ids), got " + a.format)
    }
    let stride = layoutStride(order.records)
    if (stride === 0 || stride % 4 !== 0) throw new Error(site + ": the indexed record layout is " + stride + " bytes; records must be whole floats")
    let at = 0
    for (let a of order.records) {
      if (a.name === order.position) {
        if (a.format !== "float32x3" && a.format !== "float32x4") {
          throw new Error(site + ": instanceOrder position attribute '" + a.name + "' must be float32x3 or float32x4, got " + a.format)
        }
        if (at % 4 !== 0) throw new Error(site + ": instanceOrder key attribute '" + a.name + "' starts at byte " + at + " of its record; the key must start on a 4-byte boundary")
        return { key: layoutKey(ids), floats: at / 4, position: true, descending: order.descending === true, retain: true, indices: { stride } }
      }
      at += VERTEX_FORMATS[a.format].bytes
    }
    throw new Error(site + ": instanceOrder names attribute '" + order.position + "', which the record layout does not declare")
  }
  let position = "position" in order
  let name = position ? (order as { position: string }).position : (order as { field: string }).field
  if (!position && (order as { retain?: boolean }).retain !== undefined) {
    throw new Error(site + ": instanceOrder retain applies to a position key; a field key re-orders when its records publish")
  }
  for (let b of buffers) {
    let at = 0
    for (let a of b.attributes) {
      if (a.name === name) {
        if (position && a.format !== "float32x3" && a.format !== "float32x4") {
          throw new Error(site + ": instanceOrder position attribute '" + name + "' must be float32x3 or float32x4, got " + a.format)
        }
        if (!position && a.format !== "float32") {
          throw new Error(site + ": instanceOrder field attribute '" + name + "' must be float32, got " + a.format)
        }
        if (at % 4 !== 0) {
          throw new Error(site + ": instanceOrder key attribute '" + name + "' starts at byte " + at + " of its record; the key must start on a 4-byte boundary")
        }
        return {
          key: layoutKey(b.attributes),
          floats: at / 4,
          position,
          descending: order.descending === true,
          retain: position && (order as { retain?: boolean }).retain === true,
          indices: null,
        }
      }
      at += VERTEX_FORMATS[a.format].bytes
    }
  }
  throw new Error(site + ": instanceOrder names attribute '" + name + "', which no instance buffer of the material declares")
}

/**
 * Whether a material still carries the layout an ordered mesh keys on
 * (the key stream must be one of the buffers the entry binds). Throws
 * naming `site`; checked beside the instance pairing at add().
 */
export function checkOrderPairing(material: Material, inst: MeshInstances, site: string): void {
  if (inst.order === null) return
  let buffers = material.instanceBuffers ?? []
  let key = inst.order.key
  if (!buffers.some(b => layoutKey(b.attributes) === key)) {
    throw new Error(site + ": the material declares no instance buffer with the mesh's instanceOrder key layout (" + key + ")")
  }
}

/** What both population kinds take. */
type PopulationOptions = {
  /** LOCAL bounds covering the population ([minX, minY, minZ, maxX, maxY,
   * maxZ] - geometryBounds' shape), copied in: the mesh node's own box,
   * what the frustum test and the transparent sort use. On an instanced
   * mesh it is a cull box ONLY - the instances are what picks, never the
   * mesh's box; on a record mesh (records opaque to picking) it is also
   * the box the queries test, twelve triangles. Absent, an instanced
   * mesh culls by the union of its live instances' boxes (the core
   * follows them) and sorts by its node position; a record mesh is never
   * culled and never picked. */
  bounds?: ArrayLike<number>
  /** Draw the records in key order, decided core-side (see
   * InstanceOrderOptions): back-to-front for a transparent population, an
   * explicit sort field for a JS-stepped one. Fixed at creation. */
  instanceOrder?: InstanceOrderOptions
  /** Debug label for the record buffer. */
  label?: string
}

export type InstancedMeshOptions = PopulationOptions & {
  /** Instance slots reserved up front (default 64). addInstance past it
   * doubles the buffer: a replacement whose live records the core moves
   * in one call and republishes at the next flush - amortized like a
   * dynamic array; reserve realistically to skip the copies. */
  capacity?: number
  /** The node the instance records are relative to (default: the mesh
   * itself). Set it to an ANCESTOR the mesh sits under at identity and
   * instances may be placed under any node of that ancestor's subtree,
   * not only under the mesh - a population whose copies ride a hierarchy
   * the mesh is not the root of (createModel's shared parts: the mesh
   * hangs at identity under the model root, the anchor, and each
   * placement node in the model's tree carries an instance). The core
   * writes each record as the instance's world relative to the anchor,
   * so the draw's `uModel * instanceMatrix()` places it correctly only
   * while the mesh's world equals the anchor's: keep the chain between
   * them identity. Checked at scene enter (the anchor must be an
   * ancestor of the mesh, or add throws); the identity is the caller's
   * contract, like an instanced LOD's levels. */
  anchor?: SceneNode
}

/** A record mesh's bounds are also its ONLY picking leaf: records are
 * opaque data, so without them the mesh never picks and pointer events
 * never target it. */
export type RecordMeshOptions = PopulationOptions & {
  /** Transfer the records to the engine at creation - the write-once
   * form for a large population (a baked splat cloud): the engine keeps
   * the one CPU-side copy, and the mesh keeps NO mirror, so the records
   * are resident once instead of twice and the creation bytes are free
   * to drop the moment this returns. The population is write-once by
   * contract: setRecords, updateRecords, instanceAttribute and growth
   * throw; setRecordCount (an ordered mesh republishes the prefix
   * core-side), setVisible, transforms, re-attach and disposeInstances
   * all still work. Takes a single-stream material; with an
   * instanceOrder the engine's retained copy is implied (`retain` is
   * forced on). Default false. */
  transfer?: boolean
}

function copyBounds(bounds: ArrayLike<number> | undefined, site: string): Float32Array | null {
  if (bounds === undefined) return null
  if (bounds.length !== 6) throw new Error(site + ": bounds must be [minX, minY, minZ, maxX, maxY, maxZ]")
  let out = new Float32Array(6)
  for (let i = 0; i < 6; i++) out[i] = bounds[i]!
  return out
}

function instancedBuffers(material: Material, site: string): VertexBufferLayout[] {
  let buffers = material.instanceBuffers
  if (buffers === undefined) {
    throw new Error(site + ": the material declares no instanceBuffers - build it with shaderMaterialClass({ instanceBuffers: [...] }) or a stock material's `instanced`")
  }
  return buffers
}

// The mesh buffer a declared instance layout binds to: the core's matrix
// buffer for the matrix layout, else the stream with the same attribute
// list (names and formats - a byte-equal layout with other formats would
// decode differently). Throws naming `site` when the mesh has none.
function streamBuffer(inst: MeshInstances, layout: VertexBufferLayout, site: string): BufferId {
  let key = layoutKey(layout.attributes)
  if (inst.matrix !== null && key === MATRIX_KEY) return inst.matrix
  let stream = inst.streams.find(s => layoutKey(s.layout) === key)
  if (stream === undefined) {
    let carried = [...(inst.matrix !== null ? [MATRIX_KEY] : []), ...inst.streams.map(s => layoutKey(s.layout))]
    throw new Error(site + ": the material's instance buffer " + key + " is none of the mesh's record layouts (" + carried.join("; ") + ")")
  }
  return stream.buffer
}

/**
 * Whether a material's instance buffers fit a mesh's population: every
 * layout it declares must be one the mesh carries (the matrix on an
 * instanced mesh, else a stream with the same attribute list). Throws
 * otherwise - at creation for the mesh's own material, at add() for a
 * swapped one, at a shadow view's attach for a shadow variant.
 */
export function checkInstancePairing(material: Material, inst: MeshInstances, site: string): void {
  for (let layout of instancedBuffers(material, site)) streamBuffer(inst, layout, site)
}

/** The instance buffers a draw entry binds for a material over a
 * population, one per declared layout in the pipeline's order after the
 * geometry's: the matrix buffer or the stream with that layout. */
export function instanceBinding(material: Material, inst: MeshInstances): BufferId[] {
  return (material.instanceBuffers ?? []).map(b => streamBuffer(inst, b, "instanceBinding"))
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
  let buffers = instancedBuffers(material, "createInstancedMesh")
  let first = buffers[0]
  if (first === undefined || layoutKey(first.attributes) !== MATRIX_KEY) {
    throw new Error(
      "createInstancedMesh: the material's first instance buffer must be the instance matrix the core writes (INSTANCE_MATRIX_ATTRIBUTES); got " + (first === undefined ? "none" : layoutKey(first.attributes)),
    )
  }
  let capacity = opts?.capacity ?? DEFAULT_CAPACITY
  if (!(Number.isInteger(capacity) && capacity > 0)) {
    throw new Error("createInstancedMesh: capacity must be a positive integer, got " + capacity)
  }
  let streams = buffers.slice(1).map((b, i) => {
    let blank = i === 0 && material.instanceStyle !== undefined ? encodeRecord(b.attributes, material.instanceStyle, "createInstancedMesh: instanceStyle") : null
    return makeStream(b.attributes, capacity, blank, streamLabel(opts?.label, i))
  })
  let mesh = createMesh(geometry, material) as InstancedMesh
  mesh._instances = {
    matrix: createBuffer(capacity * INSTANCE_BYTES, { autoFree: false, label: opts?.label }),
    streams,
    capacity,
    label: opts?.label,
    count: 0,
    bounds: copyBounds(opts?.bounds, "createInstancedMesh"),
    nodes: { slots: [], free: [] },
    anchor: opts?.anchor ?? null,
    levels: null,
    morph: populationMorph(geometry, capacity, opts?.label),
    order: resolveInstanceOrder(opts?.instanceOrder, buffers, "createInstancedMesh"),
    transfer: false,
    group: 1,
    textures: [],
  }
  return mesh
}

// The population's weights texture over `geometry`'s targets, `rows`
// slots deep; null for geometry without targets.
function populationMorph(geometry: Geometry, rows: number, label: string | undefined): { texture: TextureId; rows: number } | null {
  if (geometry.morphs === undefined) return null
  return { texture: createWeightsTexture(geometry.morphs.names.length, rows, label === undefined ? undefined : label + "-weights"), rows }
}

// Replace the population's weights texture: grown (`sameTargets`), every
// live instance's register moves to the new texture's row, its weights
// and any running track intact; the geometry changed, every instance
// drops its state (a fresh, zeroed one binds the new row at its next
// activation - now, for one in a scene). The entries re-point, then the
// old texture goes.
function replacePopulationMorph(mesh: InstancedMesh | RecordMesh, next: { texture: TextureId; rows: number } | null, sameTargets: boolean): void {
  let inst: MeshInstances = mesh._instances
  let previous = inst.morph
  inst.morph = next
  if (inst.nodes !== null) {
    for (let n of inst.nodes.slots) {
      if (n === null) continue
      if (sameTargets && next !== null) rebindMorph(n, next.texture)
      else {
        resetMorph(n)
        if (next !== null && n._scene !== null) activateMorph(n)
      }
    }
  }
  mesh._scene?._setMorphTexture(mesh)
  if (previous !== null) destroyTexture(previous.texture)
}

/** The meshes sharing a population's instance slots: the LOD levels of an
 * instanced LOD, else the mesh alone. */
function populationLevels(mesh: InstancedMesh): InstancedMesh[] {
  return mesh._instances.levels ?? [mesh]
}

/**
 * Write an instance's style record: the values of the material's second
 * instance buffer (the first stream), one per attribute component in
 * order, as the shader sees them - `[r, g, b, a]` for a stock material's
 * instanceColors, whatever the format stores them as. The app's per-copy
 * data beside the matrix the core writes; any number of instances styled
 * between two frames cost one coalesced buffer write at the scene's
 * sync, so this is a frame-rate path like setTransform. Throws on a
 * material without a second instance buffer or a record of the wrong
 * length; a no-op on a removed instance. instanceAttribute + updateRecords
 * is the same write by attribute name, and the only way into a third
 * stream.
 */
export function setInstanceStyle(instance: InstanceNode, values: ArrayLike<number>): void {
  let mesh = instance.mesh
  if (mesh === null) return
  if (mesh._instances.streams[0] === undefined) throw new Error("setInstanceStyle: the mesh's material declares no instance buffer beyond the matrix (no style record)")
  // Every level of an instanced LOD takes the style; a level whose
  // material carries no style record is left alone.
  for (let l of populationLevels(mesh)) {
    let stream = l._instances.streams[0]
    if (stream === undefined) continue
    writeRecord(stream, instance._slot, values, "setInstanceStyle")
    markRecords(l, stream, instance._slot * stream.stride, (instance._slot + 1) * stream.stride)
  }
}

/**
 * The accessor for per-instance attribute `name` over the mesh's record
 * streams - geometryAttribute one step up: record index in, component
 * values as the shader sees them (a unorm8x4 tint reads and writes as
 * 0..1), encoded through the format's codec. Write through it, then
 * updateRecords the range; the accessor stays valid across growth. null
 * when no stream carries the name (the matrix columns are the core's,
 * not reachable here); throws on a disposed mesh.
 */
export function instanceAttribute(mesh: InstancedMesh | RecordMesh, name: string): AttributeAccess | null {
  let inst: MeshInstances | null = mesh._instances
  if (inst === null) throw new Error("instanceAttribute: the mesh's instances are disposed")
  if (inst.transfer) throw new Error("instanceAttribute: the records were transferred at creation (transfer: true) - there is no mirror to read or write")
  for (let s of inst.streams) {
    let i = s.layout.findIndex(a => a.name === name)
    if (i >= 0) return s._fields[i]!
  }
  return null
}

/** Options of `updateRecords`: the stream (default 0) and the record
 * range (default the whole stream, capacity wide). */
export type UpdateRecordsOptions = { stream?: number; first?: number; count?: number }

/**
 * Publish records `[first, first + count)` of one record stream from its
 * mirror at the scene's next sync: updateVertices for instance data, and
 * the partial rewrite a population stepped in JS wants (ten moved records
 * of ten thousand cost ten). Write the mirror first, through
 * instanceAttribute, then call this. The range is against the stream's
 * capacity, not the drawn count: a record mesh writes ahead and dials
 * setRecordCount after.
 */
export function updateRecords(mesh: InstancedMesh | RecordMesh, options: UpdateRecordsOptions = {}): void {
  let inst: MeshInstances | null = mesh._instances
  if (inst === null) throw new Error("updateRecords: the mesh's instances are disposed")
  if (inst.transfer) throw new Error("updateRecords: the records were transferred at creation (transfer: true) - there is no mirror to publish from")
  let index = options.stream ?? 0
  let stream = inst.streams[index]
  if (!Number.isInteger(index) || stream === undefined) {
    throw new Error("updateRecords: stream " + index + " is out of range; the mesh has record streams 0.." + (inst.streams.length - 1))
  }
  let first = options.first ?? 0
  let count = options.count ?? inst.capacity - first
  if (!Number.isInteger(first) || !Number.isInteger(count) || first < 0 || count < 0 || first + count > inst.capacity) {
    throw new Error("updateRecords: range [" + first + ", " + (first + count) + ") is outside the mesh's " + inst.capacity + " records")
  }
  if (count > 0) markRecords(mesh, stream, first * stream.stride, (first + count) * stream.stride)
}

/** Publish a mesh's pending record writes, one buffer write per dirty
 * stream (the scene calls it from its sync). An ordered mesh publishes
 * the LIVE record set whole through the write lease - the engine gathers
 * it into key order during the copy, and a byte range has no stable
 * position under a permutation (writeBuffer throws on an ordered buffer). */
export function publishRecords(mesh: Mesh): void {
  let inst = mesh._instances
  if (inst === null) return
  if (inst.order !== null) {
    for (let s of inst.streams) {
      if (s.dirty === null) continue
      s.dirty = null
      let bytes = inst.count * s.stride
      if (bytes === 0) continue
      let block = beginBufferWrite(s.buffer)
      new Uint8Array(block.buffer, 0, bytes).set(vertexBytes(s.data).subarray(0, bytes))
      endBufferWrite(s.buffer, bytes)
    }
    return
  }
  for (let s of inst.streams) {
    if (s.dirty === null) continue
    let [lo, hi] = s.dirty
    s.dirty = null
    writeBuffer(s.buffer, vertexBytes(s.data).subarray(lo, hi), lo)
  }
}

/** @internal Mark every stream's live records dirty, so the next sync
 * republishes the population whole: an ordered mesh (re-)entering a
 * scene seeds the entry's freshly built engine-side order registry from
 * the JS mirrors. */
export function markLiveRecords(mesh: Mesh): void {
  let inst = mesh._instances
  // A transferred mesh has no mirror; the engine's own (buffer-lifetime)
  // mirror seeds a re-attached entry instead.
  if (inst === null || inst.count === 0 || inst.transfer) return
  for (let s of inst.streams) markRecords(mesh, s, 0, inst.count * s.stride)
}

/**
 * Add an instance to an instanced mesh: a scene node placed by `update`
 * (position/rotation/quaternion/scale; absent keys at their identity)
 * under `parent` - the mesh itself by default, or a node inside the
 * mesh's subtree (a squad group within a fleet: the record stays
 * mesh-relative through it) - inside the ANCHOR's subtree when the mesh
 * was created with one (see InstancedMeshOptions.anchor). Its record
 * slot is fixed for its life (a
 * removed instance's slot recycles to the next add); past the
 * reservation the buffers double. From here on it is a node like any
 * other: setTransform/setTransition/setVisible, lookAt, worldPosition,
 * pointer handlers, children of its own (a headlight mesh under a car
 * instance) - and `destroy` (its slot hides at the next flush and recycles
 * to the next addInstance; with an `exit` it animates out first), never
 * remove: an instance cannot exist outside its mesh.
 */
export function addInstance(mesh: InstancedMesh, update?: TransformUpdate, parent: SceneNode = mesh): InstanceNode {
  let inst: (MeshInstances & { nodes: InstanceSlots }) | null = mesh._instances
  if (inst === null) throw new Error("addInstance: the mesh's instances are disposed")
  let root: SceneNode = inst.anchor ?? mesh
  for (let p: SceneNode | null = parent; p !== root; p = p.parent) {
    if (p === null) {
      throw new Error(inst.anchor === null ? "addInstance: parent must be the mesh or a node inside its subtree" : "addInstance: parent must be the mesh's anchor or a node inside its subtree")
    }
  }
  let slot = inst.nodes.free.pop() ?? inst.nodes.slots.length
  // The levels of an instanced LOD grow, count and blank together: one
  // slot, one record per level.
  let levels = populationLevels(mesh)
  if (slot >= inst.capacity) for (let l of levels) growInstances(l, inst.capacity * 2)
  let instance = makeNode("instance") as InstanceNode
  instance.mesh = mesh
  ;(instance as { _slot: number })._slot = slot
  if (update !== undefined) setTransform(instance, update)
  inst.nodes.slots[slot] = instance
  instance.parent = parent
  parent.children.push(instance)
  for (let l of levels) {
    let li = l._instances
    if (slot >= li.count) {
      li.count = slot + 1
      l._scene?._setCount(l)
    }
    // A recycled slot must not wear its last occupant's records.
    for (let s of li.streams) {
      vertexBytes(s.data).set(s.blank, slot * s.stride)
      markRecords(l, s, slot * s.stride, (slot + 1) * s.stride)
    }
  }
  if (parent._scene) enterScene(instance, parent._scene)
  return instance
}

/**
 * @internal Move a live instance under another node of its population's
 * frame (the mesh, or the anchor's subtree): its slot and record are
 * kept, its local pose is now read against the new parent, and in a
 * scene it re-enters there so the core rebinds the record against the
 * current anchor. The generic add/remove refuse instances (slot-bound);
 * this is the one reparent, for bindSkeleton's grafts.
 */
export function reparentInstance(instance: InstanceNode, parent: SceneNode): void {
  let mesh = instance.mesh
  if (mesh === null || instance._destroyed) throw new Error("reparentInstance: the instance is gone")
  if (parent._destroyed) throw new Error("reparentInstance: the parent was destroyed")
  let root: SceneNode = mesh._instances?.anchor ?? mesh
  for (let p: SceneNode | null = parent; p !== root; p = p.parent) {
    if (p === null) throw new Error("reparentInstance: parent must be inside the population's anchor subtree")
  }
  if (instance._scene) leaveScene(instance)
  let old = instance.parent
  if (old !== null) {
    let at = old.children.indexOf(instance)
    if (at >= 0) old.children.splice(at, 1)
  }
  instance.parent = parent
  parent.children.push(instance)
  if (parent._scene) enterScene(instance, parent._scene)
}

// Grow the population's buffers to `next` records: replacements (never a
// resize). An instanced mesh's live matrix records move to the new buffer
// in one core call and republish at the next flush; every stream's mirror
// is copied over and marked whole, so the scene republishes it; the entry
// re-points, then the old buffers, which the entry held alive until now,
// are freed.
function growInstances(mesh: InstancedMesh | RecordMesh, next: number): void {
  let inst: MeshInstances = mesh._instances
  if (inst.transfer) throw new Error("growInstances: a transferred population is fixed at its creation records")
  let previous = inst.capacity
  inst.capacity = next
  let freed: BufferId[] = []
  if (inst.matrix !== null) {
    freed.push(inst.matrix)
    inst.matrix = createBuffer(next * INSTANCE_BYTES, { autoFree: false, label: inst.label })
    // Out of a scene nothing is bound, and retargeting an empty source throws.
    if (inst.nodes !== null && inst.nodes.slots.some(n => n !== null && n._node !== null)) spatial.retargetRecords(freed[0]!, inst.matrix)
  }
  inst.streams.forEach((s, i) => {
    freed.push(s.buffer)
    s.buffer = createBuffer(next * s.stride, { autoFree: false, label: streamLabel(inst.label, i) })
    let held = vertexBytes(s.data)
    viewStream(s, new ArrayBuffer(next * s.stride))
    vertexBytes(s.data).set(held)
    s.dirty = null
    markRecords(mesh, s, 0, previous * s.stride)
  })
  mesh._scene?._setBuffer(mesh)
  for (let b of freed) destroyBuffer(b)
  if (inst.morph !== null) replacePopulationMorph(mesh, populationMorph(mesh.geometry, next, inst.label), true)
}

/**
 * A mesh drawing `geometry` once per record of `records`: one draw entry,
 * one uModel write, N instances whose per-copy data is a few JS-written
 * floats - the raw escape hatch for motion only JS can compute at scale
 * (a particle sim, a crowd stepped in a worker), Three's InstancedMesh
 * and Godot's MultiMesh as plain data. The material must declare
 * `instanceBuffers` (shaderMaterialClass); its vertex stage reads each
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
  records: ArrayBufferView,
  count?: number,
  opts?: RecordMeshOptions,
): RecordMesh {
  let buffers = instancedBuffers(material, "createRecordMesh")
  let order = resolveInstanceOrder(opts?.instanceOrder, buffers, "createRecordMesh")
  let indices = order?.indices ?? null
  if (indices !== null && opts?.transfer === false) {
    throw new Error("createRecordMesh: an indexed instanceOrder hands the records to the engine by nature; transfer cannot be false")
  }
  let transfer = opts?.transfer === true || indices !== null
  if (transfer && buffers.length !== 1) {
    throw new Error(
      "createRecordMesh: transfer takes a single-stream material; this one declares " + buffers.length + " instance buffers (further streams need the partial writes transfer gives up)",
    )
  }
  // Under an indexed order the records are laid out by the order's record
  // layout, not the material's (whose one buffer is the id stream).
  let capacity: number
  if (indices !== null) {
    if (records.byteLength === 0 || records.byteLength % indices.stride !== 0) {
      throw new Error("createRecordMesh: records hold " + records.byteLength + " bytes, not a positive whole number of " + indices.stride + "-byte records")
    }
    capacity = records.byteLength / indices.stride
  } else {
    capacity = recordCount(records, buffers[0]!.attributes, "createRecordMesh")
  }
  // A transferred order always retains: the engine's mirror is the ONE
  // record copy, what re-attaches seed from and count changes republish
  // (a field key gets the same retention, engine-side only).
  if (transfer && order !== null && !order.retain) order = { ...order, retain: true }
  let mesh = createMesh(geometry, material) as RecordMesh
  let group = indices !== null ? layoutStride(buffers[0]!.attributes) / 4 : 1
  mesh._instances = {
    matrix: null,
    streams:
      indices !== null
        ? [makeIndexStream(buffers[0]!.attributes, capacity, group, vertexBytes(records), streamLabel(opts?.label, 0))]
        : transfer
          ? [makeTransferredStream(buffers[0]!.attributes, vertexBytes(records), order !== null, streamLabel(opts?.label, 0))]
          : buffers.map((b, i) => makeStream(b.attributes, capacity, null, streamLabel(opts?.label, i))),
    capacity,
    label: opts?.label,
    count: Math.max(0, Math.min(Math.floor(count ?? capacity), capacity)),
    bounds: copyBounds(opts?.bounds, "createRecordMesh"),
    nodes: null,
    anchor: null,
    levels: null,
    morph: null,
    order,
    transfer,
    group,
    textures: [],
  }
  if (!transfer) copyRecords(mesh, records, capacity)
  return mesh
}

// Overwrite the first `written` records of a record mesh's first stream
// from `records` and mark them for publish.
function copyRecords(mesh: RecordMesh, records: ArrayBufferView, written: number): void {
  let stream = mesh._instances.streams[0]!
  vertexBytes(stream.data).set(vertexBytes(records))
  if (written > 0) markRecords(mesh, stream, 0, written * stream.stride)
}

/**
 * Overwrite a record mesh's records (its first stream) from the start
 * and (by default) draw exactly the records written - pass `count` to
 * draw fewer, or to keep more previously written ones alive past a
 * partial rewrite. `records` is laid out in the material's first
 * instance layout (a Float32Array over an all-float layout, bytes
 * through instanceAttribute's codecs otherwise); a length that is not
 * whole records throws. More records than the buffer holds grow it:
 * capacity doubles (or jumps to the records written when that is more)
 * into replacement buffers, the entry is re-pointed and the old ones
 * freed - so a population grows without a new mesh, with the copies
 * amortized like any dynamic array (size the initial records to skip
 * them). The write publishes at the scene's next sync, one buffer write;
 * for a few records of many, write the mirror through instanceAttribute
 * and updateRecords the range instead.
 */
export function setRecords(mesh: RecordMesh, records: ArrayBufferView, count?: number): void {
  let inst = mesh._instances
  if (inst.transfer) throw new Error("setRecords: the records were transferred at creation (transfer: true) - the population is write-once; mount a new mesh for new records")
  let written = recordCount(records, inst.streams[0]!.layout, "setRecords")
  if (written > inst.capacity) growInstances(mesh, Math.max(written, inst.capacity * 2))
  copyRecords(mesh, records, written)
  setRecordCount(mesh, count ?? written)
}

/** Set how many records draw (clamped to [0, capacity]). The visibility
 * switch composes: a hidden mesh stores the count and draws it on unhide. */
export function setRecordCount(mesh: RecordMesh, count: number): void {
  let inst = mesh._instances
  let n = Math.max(0, Math.min(Math.floor(count), inst.capacity))
  if (n === inst.count) return
  inst.count = n
  // Ordered records draw the first n GATHERED records, so a count change
  // republishes the live set - a shrink would otherwise keep drawing the
  // old population's nearest records, a growth its stale tail. A
  // transferred mesh has no mirror to publish from; the engine follows
  // the entry's instance count into its own mirror's prefix instead.
  if (inst.order !== null && !inst.transfer && n > 0) for (let s of inst.streams) markRecords(mesh, s, 0, n * s.stride)
  mesh._scene?._setCount(mesh)
}

/**
 * Detach the mesh (if attached) and free its record buffers (the matrix
 * buffer and every stream). The buffers are mesh-owned with no
 * reference count (unlike geometry buffers they are never shared), so
 * this is the one explicit free; the mesh cannot be re-added afterwards.
 * An instanced mesh's instances go inert with it. On a mesh `destroy`
 * just let go of, with an exit still playing (its own or an instance's),
 * the free waits for the last of them - the components unmount as
 * `destroy(mesh)` then `disposeInstances(mesh)`, so an `<Instance>`'s
 * exit plays through its mesh's unmount.
 */
export function disposeInstances(mesh: InstancedMesh | RecordMesh): void {
  let inst: MeshInstances | null = mesh._instances
  if (inst === null) return
  // An instanced LOD frees every level's buffers; the levels are untied
  // first so each disposes as a plain population.
  if (inst.levels !== null) {
    let levels = inst.levels
    for (let l of levels) if (l._instances !== null) l._instances.levels = null
    for (let l of levels) disposeInstances(l)
    return
  }
  // A destroyed mesh still animating out (its own exit, or its instances'
  // - it waits for them) keeps the buffers until it is gone: the
  // component unmount is destroy then dispose, and an exit that dispose
  // cut short would be no exit at all.
  if (afterFree(mesh, () => disposeInstances(mesh))) return
  if (mesh._scene) remove(mesh)
  if (inst.nodes !== null) {
    for (let n of inst.nodes.slots) if (n !== null) n.mesh = null
    // The destroyed instance nodes' hiding writes land now, while the
    // buffer still exists (a write into a freed buffer warns).
    spatial.flush()
  }
  if (inst.matrix !== null) destroyBuffer(inst.matrix)
  for (let s of inst.streams) destroyBuffer(s.buffer)
  if (inst.morph !== null) destroyTexture(inst.morph.texture)
  for (let t of inst.textures) destroyTexture(t)
  ;(mesh as Mesh)._instances = null
}

/** Set a mesh's explicit draw-order key (see Mesh.renderOrder). */
export function setRenderOrder(mesh: Mesh, order: number): void {
  if (mesh.renderOrder === order) return
  mesh.renderOrder = order
  mesh._scene?._reorder(mesh)
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
  // An instanced LOD's levels draw where the population draws.
  for (let l of mesh._instances?.levels ?? []) if (l !== mesh) setLayers(l, layers)
}

/** Draw the mesh into the scene's shadow map, or stop (see Mesh.castShadow). */
export function setCastShadow(mesh: Mesh, cast: boolean): void {
  if (mesh.castShadow === cast) return
  mesh.castShadow = cast
  mesh._scene?._setCast(mesh)
  for (let l of mesh._instances?.levels ?? []) if (l !== mesh) setCastShadow(l, cast)
}

/**
 * Give the mesh an environment of its own, or none: `null` removes the
 * environment term entirely - a sealed interior under an outdoor sky,
 * where light `layers` mask the sun but cannot mask image lighting -
 * `{ cube, intensity?, rotation? }` samples that cube instead of the
 * scene's (an interior probe's chain, a baked room), and `undefined`
 * (the default) follows the scene's, live through scene.setEnvironment.
 * Unity's reflectionProbeUsage Off and Three's `material.envMap`, per
 * MESH, because materials are shared and interiors are mesh-grained
 * here. Applied where the material reflects at all (Material.env); a
 * no-op on any other material. The mesh's entries are rebuilt, so this
 * is a state change (a camera crossing a doorway), not a frame-rate
 * dial. A reflection probe drawing this mesh never samples its own cube
 * through it (the black placeholder stands in, the same-pass rule).
 */
export function setEnvironment(mesh: Mesh, environment: EnvironmentOptions | null | undefined): void {
  let next = environment == null ? environment : { cube: environment.cube, ...checkCubeKnobs(environment, "setEnvironment") }
  let a = mesh.environment
  let same = next == null || a == null ? next === a : next.cube === a.cube && next.intensity === a.intensity && next.rotation === a.rotation
  if (same) return
  mesh.environment = next
  rebuildEntry(mesh)
  // An instanced LOD's levels reflect what the population reflects.
  for (let l of mesh._instances?.levels ?? []) if (l !== mesh) setEnvironment(l, environment)
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
  mesh._range = null
  // A standalone mesh owns its own weights: a geometry with different
  // targets (or none) replaces or drops them; a model part keeps its
  // node as the owner, whatever the geometry.
  if (mesh._morphOwner === mesh || mesh._morphOwner === null) {
    mesh._morphOwner = geometry.morphs === undefined ? null : mesh
    resetMorph(mesh)
  }
  // A population's weights texture follows its geometry's target list.
  let inst = mesh._instances
  if (inst !== null && inst.nodes !== null && (inst.morph !== null || geometry.morphs !== undefined)) {
    replacePopulationMorph(mesh as InstancedMesh, populationMorph(geometry, inst.capacity, inst.label), false)
  }
  rebuildEntry(mesh)
}

/**
 * Draw indices `[first, first + count)` of the mesh's geometry instead of
 * the whole list (Three's `geometry.setDrawRange`; on the mesh here
 * because the scene owns the entries): the reveal-and-hide dial for
 * geometry built once - a trail growing along its buffer, a level-of-
 * detail cut, a strip drawn up to a moving end. `count` absent draws to
 * the end. Applied to the mesh's entry and every view's; setGeometry
 * resets it to the whole list. Throws on a range outside the index list;
 * a partial primitive at the cut draws nothing, like GL.
 */
export function setDrawRange(mesh: Mesh, first: number, count?: number): void {
  if (mesh._sprite) throw new Error("setDrawRange: a sprite draws the shared unit quad")
  let total = mesh.geometry.indices.length
  let n = count ?? total - first
  if (!Number.isInteger(first) || !Number.isInteger(n) || first < 0 || n < 0 || first + n > total) {
    throw new Error("setDrawRange: range [" + first + ", " + (first + n) + ") is outside the geometry's " + total + " indices")
  }
  mesh._range = first === 0 && n === total ? null : { first, count: n }
  mesh._scene?._setRange(mesh)
}

/** Swap a mesh's material: its draw entry is rebuilt. */
export function setMaterial(mesh: Mesh, material: Material): void {
  if (mesh.material === material) return
  mesh.material = material
  uncullFlooredSprite(mesh)
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
  if (params.uScreenPx !== undefined) uncullFlooredSprite(mesh)
  mesh._scene?._setParams(mesh, params)
}