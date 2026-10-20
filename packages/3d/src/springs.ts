// Spring bones: `createSpringBones(root, { colliders, springs })` declares
// the chains under `root` - a model, or any node whose subtree is the rig
// (a group of plain nodes, as createMixer's rig over nodes) - that lag
// behind the animated bones they hang from - hair, a hood, a skirt, a
// tail, a cable - and the spatial core solves them: Unity's SpringBone, Godot's SpringBoneSimulator3D, three-vrm's
// VRMSpringBoneManager, in VRM's vocabulary (per joint: stiffness, drag,
// gravity power and direction, hit radius; colliders as spheres, capsules
// and planes, shared by index; an optional center). Nothing to call per
// frame: the core steps every chain once per frame AFTER the frame's JS
// and its flush - the pose post-pass - so the chains react to the final
// primary pose, the players and the frame's own writes (a look-at, an IK,
// a controller's move) included, where Unity runs its LateUpdate and
// Godot its skeleton modifiers. The solve is frame-rate exact (the free
// motion is the closed-form flow of three-vrm's 60 Hz step, the
// projections run at 240 Hz substeps), so authored values mean what they
// mean in three-vrm and a 50, 60 or 120 Hz display draws the same motion.
//
// The declaration lives on the root node and re-applies on every scene
// enter, like setTransition: the chains bind when the root is in a scene
// (its joints are live core nodes), drop with it, and come back when it
// is added again. The chain owns its joints' rotations: a setTransform
// on one is overwritten at the step, and getTransform/worldPosition on
// one during the frame read the previous frame's solve (a node attached
// under a joint follows through the hierarchy and needs no read).

import * as spatial from "flux:spatial"
import type { NodeId, SpringId } from "flux:spatial"
import type { SceneNode } from "./node.ts"
import type { Vec3 } from "./math.ts"

/** VRM's virtual tail for a chain's last bone: 7 cm along the bone's own
 * offset from its parent (UniVRM and three-vrm give it the same). */
const VIRTUAL_TAIL = 0.07
/** three-vrm's defaults (VRMSpringBoneJointSettings). */
const DEFAULT_STIFFNESS = 1
const DEFAULT_DRAG = 0.4
const DEFAULT_GRAVITY_POWER = 0
const DEFAULT_GRAVITY_DIR: Vec3 = [0, -1, 0]
const DEFAULT_HIT_RADIUS = 0
/** The flux:spatial payload strides (createSpring). */
const JOINT_FLOATS = 10
const COLLIDER_FLOATS = 9
const SHAPE_KIND = { sphere: 0, capsule: 1, plane: 2 } as const

/** One simulated bone of a chain. */
export type SpringJointSpec = {
  /** The bone this joint turns: a node under the root. */
  node: SceneNode
  /** Where the chain continues, in the bone's own space. Default: the
   * rest position of the next listed joint that is this bone's child,
   * else VRM's virtual tail (7 cm along the bone's own rest offset). */
  tail?: Vec3
  /** The pull back toward the rest direction (default 1). */
  stiffness?: number
  /** The damping, 0 none .. 1 total (default 0.4). */
  drag?: number
  /** Gravity's strength (default 0) and direction (default -y), the
   * direction in world space. */
  gravityPower?: number
  gravityDir?: Vec3
  /** The tail's radius against the colliders (default 0). */
  hitRadius?: number
}

/** A collider the tails are kept out of (or, `inside`, within), in its
 * node's space. */
export type SpringColliderSpec = { node: SceneNode } & (
  | { shape: "sphere"; offset?: Vec3; radius: number; inside?: boolean }
  | { shape: "capsule"; a: Vec3; b: Vec3; radius: number; inside?: boolean }
  | { shape: "plane"; offset?: Vec3; normal?: Vec3 }
)

/** One chain (VRM's spring): its joints PARENTS FIRST, the colliders it
 * collides with as indices into the options' list, and the node whose
 * space it is solved in (its own motion then does not swing the chain -
 * VRM's center; default world space). */
export type SpringChainSpec = {
  joints: SpringJointSpec[]
  colliders?: number[]
  center?: SceneNode
}

export type SpringBonesOptions = {
  springs: SpringChainSpec[]
  colliders?: SpringColliderSpec[]
}

/** The authored settings `setJoint` can change (the tail is structure
 * and stays). */
export type SpringJointUpdate = Pick<SpringJointSpec, "stiffness" | "drag" | "gravityPower" | "gravityDir" | "hitRadius">

export type SpringBones = {
  /** Put every chain back at rest now: the tails along their rest
   * directions under the current pose, no motion carried over
   * (three-vrm's reset) - what a teleport wants. */
  reset(): void
  /** Change one joint's authored settings (`springs[chain].joints[joint]`):
   * the fields given replace the joint's. The chain goes on from where it
   * is under the new settings; a scene re-enter keeps them. For tuning
   * and states (wet hair raises drag), not for wind: that is setForce. */
  setJoint(chain: number, joint: number, update: SpringJointUpdate): void
  /** Set the runtime force on every tail - the wind: one world-space
   * vector in the units of gravityPower along a direction, added to each
   * joint's authored gravity (Godot's external_force, UniVRM's
   * ExternalForce). On every chain, or on `chain` alone. Write it per
   * frame for gusts; a constant force settles like gravity does. */
  setForce(force: Vec3, chain?: number): void
  /** Drop the chains, the joints put back at rest (the rest rotations
   * written, as `reset` writes them): hair switched off hangs as
   * authored, not frozen mid-swing. To switch it back on, declare the
   * chains again - they start at rest, which is where dispose left them. */
  dispose(): void
}

/** One chain's packed payload and its core binding while in a scene. */
type Chain = {
  nodes: SceneNode[]
  /** The packed joints, kept current with setJoint so a re-enter binds
   * the tuned values. */
  joints: Float32Array
  colliderNodes: SceneNode[]
  colliders: Float32Array
  center: SceneNode | null
  /** The runtime force, re-applied at every bind. */
  force: Float32Array
  id: SpringId | null
}

/** Where a setting sits in a joint's packed floats. */
const JOINT_OFFSET = { stiffness: 3, drag: 4, gravityPower: 5, gravityDir: 6, hitRadius: 9 } as const

/**
 * Declare the spring bones under `root`: a model, or any node whose
 * subtree holds the joints (a group of plain nodes, the rig-over-nodes
 * form of createMixer). The root must be in a scene (the chains bind to
 * live core nodes; the declaration re-binds on every scene enter). The
 * chains start at rest - each bone's declared rest rotation (a model's
 * file transform; else the rotation as it stands at the call) - under
 * the pose as it stands at the call. Every joint must be a node under
 * the root; colliders and centers may be any node in the scene. Throws
 * on a joint without a tail (no child joint and a zero rest offset), a
 * chain listing a child before its parent joint, a setting out of range,
 * or a node out of the scene.
 */
export function createSpringBones(root: SceneNode, opts: SpringBonesOptions): SpringBones {
  if (root._node === null) throw new Error("createSpringBones: the root must be in a scene (add() it first)")
  let colliders = opts.colliders ?? []
  let chains: Chain[] = opts.springs.map((spec, ci) => {
    let where = "createSpringBones: springs[" + ci + "]"
    if (spec.joints.length === 0) throw new Error(where + " has no joints")
    let nodes = spec.joints.map((j) => j.node)
    let joints = new Float32Array(nodes.length * JOINT_FLOATS)
    spec.joints.forEach((j, i) => {
      if (!under(j.node, root)) throw new Error(where + ".joints[" + i + "] is not under the root")
      if (j.node._destroyed) throw new Error(where + ".joints[" + i + "] was destroyed")
      let tail = j.tail ?? defaultTail(spec.joints, i)
      if (tail === null) throw new Error(where + ".joints[" + i + "] needs a tail: it has no child joint and a zero rest offset")
      let drag = j.drag ?? DEFAULT_DRAG
      if (!(drag >= 0 && drag <= 1)) throw new Error(where + ".joints[" + i + "].drag must be within 0..1")
      let stiffness = j.stiffness ?? DEFAULT_STIFFNESS
      let hitRadius = j.hitRadius ?? DEFAULT_HIT_RADIUS
      if (!(stiffness >= 0) || !(hitRadius >= 0)) throw new Error(where + ".joints[" + i + "]: stiffness and hitRadius must be >= 0")
      let g = j.gravityDir ?? DEFAULT_GRAVITY_DIR
      let at = i * JOINT_FLOATS
      joints.set([tail[0], tail[1], tail[2], stiffness, drag, j.gravityPower ?? DEFAULT_GRAVITY_POWER, g[0], g[1], g[2], hitRadius], at)
      // The core poses the joint: its JS mirror is stale from here on.
      j.node._native = true
    })
    let chosen = (spec.colliders ?? []).map((k) => {
      let c = colliders[k]
      if (c === undefined) throw new Error(where + " names collider " + k + ", of " + colliders.length)
      return c
    })
    let packed = new Float32Array(chosen.length * COLLIDER_FLOATS)
    chosen.forEach((c, i) => {
      let at = i * COLLIDER_FLOATS
      packed[at] = SHAPE_KIND[c.shape]
      if (c.shape === "sphere") {
        if (!(c.radius >= 0)) throw new Error(where + ": a sphere collider needs a radius >= 0")
        let o = c.offset ?? [0, 0, 0]
        packed.set([c.inside ? 1 : 0, o[0], o[1], o[2], 0, 0, 0, c.radius], at + 1)
      } else if (c.shape === "capsule") {
        if (!(c.radius >= 0)) throw new Error(where + ": a capsule collider needs a radius >= 0")
        packed.set([c.inside ? 1 : 0, c.a[0], c.a[1], c.a[2], c.b[0], c.b[1], c.b[2], c.radius], at + 1)
      } else {
        let o = c.offset ?? [0, 0, 0]
        let n = c.normal ?? [0, 1, 0]
        if (Math.hypot(n[0], n[1], n[2]) === 0) throw new Error(where + ": a plane collider needs a non-zero normal")
        packed.set([0, o[0], o[1], o[2], n[0], n[1], n[2], 0], at + 1)
      }
    })
    return { nodes, joints, colliderNodes: chosen.map((c) => c.node), colliders: packed, center: spec.center ?? null, force: new Float32Array(3), id: null }
  })
  let chainAt = (index: number, what: string): Chain => {
    let chain = chains[index]
    if (chain === undefined) throw new Error("createSpringBones." + what + ": no chain " + index + " (the declaration has " + chains.length + ")")
    return chain
  }

  let binding = {
    enter() {
      for (let chain of chains) bind(chain)
    },
    leave() {
      for (let chain of chains) {
        if (chain.id !== null) spatial.destroySpring(chain.id)
        chain.id = null
      }
    },
  }
  binding.enter()
  if (root._bindings === null) root._bindings = []
  root._bindings.push(binding)

  return {
    reset() {
      for (let chain of chains) if (chain.id !== null) spatial.resetSpring(chain.id)
    },
    setJoint(chainIndex, joint, update) {
      let chain = chainAt(chainIndex, "setJoint")
      if (!Number.isInteger(joint) || joint < 0 || joint >= chain.nodes.length) throw new Error("createSpringBones.setJoint: no joint " + joint + " (the chain has " + chain.nodes.length + ")")
      if (chain.id !== null) spatial.setSpringJoint(chain.id, joint, update)
      let at = joint * JOINT_FLOATS
      if (update.stiffness !== undefined) chain.joints[at + JOINT_OFFSET.stiffness] = update.stiffness
      if (update.drag !== undefined) chain.joints[at + JOINT_OFFSET.drag] = update.drag
      if (update.gravityPower !== undefined) chain.joints[at + JOINT_OFFSET.gravityPower] = update.gravityPower
      if (update.gravityDir !== undefined) chain.joints.set(update.gravityDir, at + JOINT_OFFSET.gravityDir)
      if (update.hitRadius !== undefined) chain.joints[at + JOINT_OFFSET.hitRadius] = update.hitRadius
    },
    setForce(force, chainIndex) {
      let targets = chainIndex === undefined ? chains : [chainAt(chainIndex, "setForce")]
      for (let chain of targets) {
        chain.force.set(force)
        if (chain.id !== null) spatial.setSpringForce(chain.id, chain.force)
      }
    },
    dispose() {
      binding.leave()
      let list = root._bindings
      if (list !== null) {
        let i = list.indexOf(binding)
        if (i >= 0) list.splice(i, 1)
      }
    },
  }
}

/** Create the chain in the core over the nodes' live ids. */
function bind(chain: Chain): void {
  let clock = chain.nodes[0]!._node
  if (clock === null) return
  let ids: NodeId[] = []
  for (let n of chain.nodes) {
    if (n._node === null) {
      console.warn("createSpringBones: a joint is no longer in the scene; its chain is not simulated")
      return
    }
    ids.push(n._node)
  }
  let colliderIds: NodeId[] = []
  for (let n of chain.colliderNodes) {
    if (n._node === null) throw new Error("createSpringBones: a collider node is not in the scene")
    colliderIds.push(n._node)
  }
  let center = chain.center === null ? undefined : chain.center._node
  if (center === null) throw new Error("createSpringBones: the center node is not in the scene")
  chain.id = spatial.createSpring(clock, ids, chain.joints, colliderIds, chain.colliders, center)
  if (chain.force[0] !== 0 || chain.force[1] !== 0 || chain.force[2] !== 0) spatial.setSpringForce(chain.id, chain.force)
}

/** Whether `node` is `owner` itself or sits somewhere under it. */
function under(node: SceneNode, owner: SceneNode): boolean {
  for (let n: SceneNode | null = node; n !== null; n = n.parent) if (n === owner) return true
  return false
}

/** The rest position of the next listed joint under this bone, else the
 * virtual tail along the bone's own rest offset, else null. */
function defaultTail(joints: SpringJointSpec[], i: number): Vec3 | null {
  let bone = joints[i]!.node
  for (let k = i + 1; k < joints.length; k++) {
    let child = joints[k]!.node
    if (child.parent === bone) return [child.position[0], child.position[1], child.position[2]]
  }
  let p = bone.position
  let len = Math.hypot(p[0], p[1], p[2])
  if (len === 0) return null
  return [(p[0] / len) * VIRTUAL_TAIL, (p[1] / len) * VIRTUAL_TAIL, (p[2] / len) * VIRTUAL_TAIL]
}
