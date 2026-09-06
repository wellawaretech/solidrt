// Meshes: the drawable nodes - plain, sprite and instanced - with their
// create-time state (geometry, material, per-mesh bindings) and the
// setter write paths that keep a live scene's draw entries in step. The
// scene side is reached through the node's SceneHooks (node.ts).

import { createBuffer, destroyBuffer, writeBuffer } from "@solidrt/core/gpu"
import type { BufferId, DrawId, ShaderParams, TextureBindings, VertexAttribute } from "@solidrt/core/gpu"
import { geometryBounds, plane } from "./geometry.ts"
import type { Geometry } from "./geometry.ts"
import type { GeometryBuffers } from "./geometry-gpu.ts"
import type { Material } from "./material.ts"
import type { Vec3 } from "./math.ts"
import * as spatial from "flux:spatial"
import { makeNode, remove } from "./node.ts"
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
 * Read the public fields freely; write through setInstances /
 * setInstanceCount so the draw range follows. */
export type MeshInstances = {
  /** The GPU record buffer, owned by the mesh (disposeInstances frees it). */
  buffer: BufferId
  /** Floats per record - the material's instanceAttributes summed. */
  stride: number
  /** Records the buffer has room for; doubles when setInstances writes
   * more (a replacement buffer, never a resize). */
  capacity: number
  /** The buffer label, carried to replacement buffers on growth. */
  label: string | undefined
  /** Records currently drawn (the entry's instanceCount while visible). */
  count: number
  /** Explicit LOCAL bounds covering the whole population ([minX, minY,
   * minZ, maxX, maxY, maxZ]), or null: the mesh then has no picking leaf -
   * records are opaque data, so the library cannot derive where the
   * instances are. */
  bounds: Float32Array | null
}

/** A mesh from createInstancedMesh: an ordinary Mesh whose entry draws
 * `instances.count` copies of the geometry, one record each. */
export type InstancedMesh = Mesh & { _instances: MeshInstances }

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

export function instanceStride(attributes: VertexAttribute[]): number {
  let stride = 0
  for (let a of attributes) stride += ATTRIBUTE_FLOATS[a.format]
  return stride
}

export type InstancedMeshOptions = {
  /** LOCAL bounds covering every instance the records place ([minX, minY,
   * minZ, maxX, maxY, maxZ] - geometryBounds' shape), copied in. Records
   * are opaque data, so only the app knows where its instances are: with
   * bounds the mesh picks and transparent-sorts like any other
   * (conservatively - one box around the whole population); without, it
   * has no picking leaf and pointer events never target it. */
  bounds?: ArrayLike<number>
  /** Debug label for the record buffer. */
  label?: string
}

/**
 * A mesh drawing `geometry` once per record of `records`: one draw entry,
 * one uModel write, N instances - the shape for forests, particles, and
 * every fleet whose per-copy data is a few floats rather than a merged
 * vertex buffer. The material must declare `instanceAttributes`
 * (shaderMaterialClass); its vertex stage reads each record through those
 * `in` variables. `records` is the interleaved attribute data (stride =
 * the attributes' floats summed) and is uploaded here; its length is the
 * buffer's initial capacity, which setInstances grows past on demand.
 * `count` limits how many records draw (default all), up to capacity.
 *
 * The result is an ordinary Mesh: add/remove, setTransform (uModel places
 * the whole population), setVisible (hiding zeroes the drawn count,
 * unhiding restores it), setMeshParams and renderOrder all apply. Update
 * records with setInstances, the drawn count with setInstanceCount, and
 * free the record buffer with disposeInstances when done for good.
 */
export function createInstancedMesh(
  geometry: Geometry,
  material: Material,
  records: Float32Array,
  count?: number,
  opts?: InstancedMeshOptions,
): InstancedMesh {
  let attributes = material.instanceAttributes
  if (attributes === undefined) {
    throw new Error(
      "createInstancedMesh: the material declares no instanceAttributes - build it with shaderMaterialClass({ instanceAttributes: [...] })",
    )
  }
  let stride = instanceStride(attributes)
  if (records.length % stride !== 0) {
    throw new Error(
      "createInstancedMesh: " + records.length + " floats is not a whole number of " + stride + "-float records",
    )
  }
  let bounds: Float32Array | null = null
  if (opts?.bounds !== undefined) {
    if (opts.bounds.length !== 6) {
      throw new Error("createInstancedMesh: bounds must be [minX, minY, minZ, maxX, maxY, maxZ]")
    }
    bounds = new Float32Array(6)
    for (let i = 0; i < 6; i++) bounds[i] = opts.bounds[i]!
  }
  let capacity = records.length / stride
  let mesh = createMesh(geometry, material) as InstancedMesh
  mesh._instances = {
    buffer: createBuffer(records, { autoFree: false, label: opts?.label }),
    stride,
    capacity,
    label: opts?.label,
    count: Math.max(0, Math.min(Math.floor(count ?? capacity), capacity)),
    bounds,
  }
  return mesh
}

/**
 * Overwrite an instanced mesh's records from the start of its buffer and
 * (by default) draw exactly the records written - pass `count` to draw
 * fewer, or to keep more previously written ones alive past a partial
 * rewrite. More records than the buffer holds grow it: capacity doubles
 * (or jumps to the records written when that is more), a new buffer is
 * created and written, the mesh's entry is re-pointed at it and the old
 * buffer freed - so a population grows without a new mesh, with the
 * copies amortized like any dynamic array (size the initial records to
 * skip them). Frame-rate-safe like setMeshParams when no growth happens.
 */
export function setInstances(mesh: InstancedMesh, records: Float32Array, count?: number): void {
  let inst = mesh._instances
  if (records.length % inst.stride !== 0) {
    throw new Error("setInstances: " + records.length + " floats is not a whole number of " + inst.stride + "-float records")
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
  setInstanceCount(mesh, count ?? written)
}

/** Set how many records draw (clamped to [0, capacity]). The visibility
 * switch composes: a hidden mesh stores the count and draws it on unhide. */
export function setInstanceCount(mesh: InstancedMesh, count: number): void {
  let inst = mesh._instances
  let n = Math.max(0, Math.min(Math.floor(count), inst.capacity))
  if (n === inst.count) return
  inst.count = n
  mesh._scene?._setCount(mesh)
}

/**
 * Detach the mesh (if attached) and free its record buffer. The buffer is
 * mesh-owned with no reference count (unlike geometry buffers it is never
 * shared), so this is the one explicit free; the mesh cannot be re-added
 * afterwards.
 */
export function disposeInstances(mesh: InstancedMesh): void {
  let inst: MeshInstances | null = mesh._instances
  if (inst === null) return
  if (mesh._scene) remove(mesh)
  destroyBuffer(inst.buffer)
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
 * unless a raycast passes its own mask (RaycastOptions.layers), which is
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