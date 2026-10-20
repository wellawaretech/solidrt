// Inverse kinematics for models: `createIk(model, { solvers })` declares
// a stack of solvers over the model's joints - a look-at (a head on the
// camera, a torso on a target), a two-bone limb (a hand on a handle, a
// weapon on a cursor), a chain (a tail, a tentacle, a rope arm) and foot
// placement (feet on a slope, the hips lowered) - and the spatial core
// steps them every frame AFTER your onFrame and its transitions and
// BEFORE the spring bones: the pose post-pass, where Unity's
// OnAnimatorIK runs (then LateUpdate), Godot's SkeletonModifier3D stack
// and Unreal's Control Rig nodes. The vocabulary is theirs: Unity's
// Animation Rigging (TwoBoneIKConstraint, ChainIKConstraint,
// MultiAimConstraint), Godot's LookAtModifier3D and SkeletonIK3D, Unreal's
// TwoBoneIK, FABRIK, CCDIK and foot placement. Nothing to call per frame:
// a target that is a node is read in the core at solve time, the ground
// under a foot is a ray the core casts itself, a weight ramps in core.
//
// Every frame each solver sees the PRIMARY pose (the players, the
// transitions, the frame's own writes), not its own last result, and
// the solvers run in declared order, each over the previous one's
// result - a look-at on the spine before a two-bone on the arm is the
// normal case, and the only way a hand reaches a point the turned torso
// moved it toward. The declaration lives on the model node and
// re-applies on every scene enter, like setTransition. The stack owns
// its joints: a setTransform on one in the frame is solved over, and
// getTransform/worldPosition on one during the frame read the previous
// frame's solve.

import * as spatial from "flux:spatial"
import type { IkId, IkLimit as CoreLimit, IkPoint as CorePoint, IkSolverSpec as CoreSpec, IkTarget as CoreTarget, NodeId } from "flux:spatial"
import type { SceneNode } from "./node.ts"
import type { Model } from "./model.ts"
import type { Quat, Vec3 } from "./math.ts"

/** The bones' forward and up axes a look-at turns, in their own space
 * (a glTF rig's head usually looks down +z). */
const DEFAULT_FORWARD: Vec3 = [0, 0, 1]
const DEFAULT_UP_AXIS: Vec3 = [0, 1, 0]
const WORLD_UP: Vec3 = [0, 1, 0]
/** The look-at's time-based transition (a target switch, loss or flip
 * through the back), ms: Godot's LookAtModifier3D duration. A quarter
 * of a second reads as a head whip on a swing between symmetric yaw
 * limits; this reads as a turn. */
const LOOK_AT_EASE_MS = 600
/** Godot's damp threshold default: a hard clamp at the limit. */
const LIMIT_DAMP = 1
/** An absent yaw or pitch limit: the whole half turn either way. */
const UNLIMITED: [number, number] = [-Math.PI, Math.PI]
/** Godot's SkeletonIK3D max_iterations and Unity's ChainIK tolerance. */
const CHAIN_ITERATIONS = 10
const CHAIN_TOLERANCE = 0.001
/** The ground ray's reach above and below the animated foot, world
 * units: Unreal's foot placement traces half a metre each way. */
const FEET_RISE = 0.5
const FEET_DROP = 0.5
/** A foot within this of its rest height is planted; over the next
 * band it lifts (the height recipe every engine's foot IK uses). */
const PLANT_HEIGHT = 0.05
const PLANT_BAND = 0.1
/** The hips offset's time constant, ms. */
const HIPS_EASE_MS = 150

/** Where a solver aims: a scene node (any node - a socket, a pickup, the
 * camera, another character's joint; read in the core at solve time),
 * or a point in world space or in `space`'s frame. */
export type IkPointSpec = { node: SceneNode } | { point: Vec3; space?: SceneNode }
/** A target: a node (its position and rotation), or a point with an
 * optional rotation in the same space. */
export type IkTargetSpec = { node: SceneNode } | { point: Vec3; rotation?: Quat; space?: SceneNode }
/** A roll reference for a look-at: a direction in world space or in
 * `space`'s frame. */
export type IkUpSpec = { vector: Vec3; space?: SceneNode }
/** A chain joint's limit, in the joint's own frame relative to its
 * animated rotation, radians: `hinge` turns about that local axis within
 * min..max; `cone` bounds the swing away from the bone (toward the next
 * joint) and `twist` the turn about it (default 0). */
export type IkLimitSpec = { hinge: Vec3; min: number; max: number } | { cone: number; twist?: number }

/** A bone or a chain of bones turns its forward axis to a target (Unity's
 * SetLookAtPosition and MultiAimConstraint, Godot's LookAtModifier3D).
 * `bones` root first (spine, neck, head), each with a `share` of the aim
 * (default 1; the shares are fractions of what remains and the last bone
 * takes all of it, so the chain reaches the target exactly - spine 0.2,
 * neck 0.3, head 0.5 reads as Unity's body/head split). `forward` and
 * `upAxis` are the bones' own axes (defaults +z, +y); `up` a roll
 * reference (default the shortest arc). `limit` is yaw and pitch about
 * the last bone's rest forward - a number for symmetric, `[min, max]`
 * for asymmetric, absent for free - with `damp` the fraction of the
 * limit past which the approach eases toward it instead of clamping
 * (default 1, a hard clamp). `easeMs` (default 600) is the time-based
 * transition when the target switches, is lost or found, or flips
 * through the back: the bones glide through rest instead of snapping
 * across the limit. */
export type IkLookAtSpec = {
  kind: "lookAt"
  bones: (SceneNode | { node: SceneNode; share?: number })[]
  target?: IkTargetSpec | null
  forward?: Vec3
  upAxis?: Vec3
  up?: IkUpSpec
  limit?: { yaw?: number | [number, number]; pitch?: number | [number, number]; damp?: number }
  easeMs?: number
  weight?: number
}

/** A limb of two bones (Unity's hand and foot goals and
 * TwoBoneIKConstraint, Unreal's TwoBoneIK, Godot's two-bone modifier):
 * `root`, `mid`, `tip` a descendant path (shoulder, elbow, wrist),
 * solved in closed form. `pole` (the hint) turns the bend plane toward
 * it; absent, the animated bend direction holds (the rest bend when the
 * limb is straight), and `poleAngle` twists about the root-target axis
 * on top. `positionWeight` (default 1) is the reach, `rotationWeight`
 * (default 0) how much the tip takes the target's rotation (a hand on a
 * handle's orientation). `soft` (default 0, off) is the reach band as a
 * fraction of the limb within which the tip eases toward full extension
 * instead of popping straight (Unreal's soft IK); `stretch` (default 0,
 * off) the most the limb may lengthen past full reach. */
export type IkTwoBoneSpec = {
  kind: "twoBone"
  root: SceneNode
  mid: SceneNode
  tip: SceneNode
  target?: IkTargetSpec | null
  pole?: IkPointSpec
  poleAngle?: number
  positionWeight?: number
  rotationWeight?: number
  soft?: number
  stretch?: number
  weight?: number
}

/** A chain of any length from `root` to `tip` (every node between is a
 * joint): `method` "fabrik" (default; natural for long chains, a
 * `magnet` turns the interior toward it) or "ccd" (per-joint limits
 * exact), up to `iterations` (default 10) sweeps until the tip is within
 * `tolerance` (default 0.001). `limits` per joint; `rotationWeight`
 * (default 0) the tip's. Godot's SkeletonIK3D, three's CCDIKSolver,
 * Unreal's FABRIK and CCDIK, Unity's ChainIKConstraint. */
export type IkChainSpec = {
  kind: "chain"
  root: SceneNode
  tip: SceneNode
  target?: IkTargetSpec | null
  method?: "fabrik" | "ccd"
  iterations?: number
  tolerance?: number
  magnet?: IkPointSpec
  limits?: { node: SceneNode; limit: IkLimitSpec }[]
  rotationWeight?: number
  weight?: number
}

/** Foot placement (Unity's foot IK pass, Unreal's foot placement):
 * `legs` two-bone limbs (hip, knee, ankle) under `hips`, whose targets
 * are GROUND HITS the core takes itself - a ray from `rise` (default
 * 0.5) above each animated foot down to `drop` (default 0.5) below it
 * against the meshes on `layers` (REQUIRED: a collider layer the model's
 * own meshes are not on, as scene.raycast's layers). `height` per leg is
 * the sole's distance below the ankle (default: measured from the
 * model's ground plane at the declaration, so declare at rest). A foot
 * within `plantHeight` (default 0.05) of its rest height is planted and
 * lifts over the next `plantBand` (default 0.1), following the clip; the
 * hips move along `up` (default world +y) by the planted foot that needs
 * it most, so both feet reach, eased by `hipsEaseMs` (default 150); each
 * planted foot aligns to its ground normal by `rotationWeight` (default
 * 1). No JS raycast per foot per frame. */
export type IkFeetSpec = {
  kind: "feet"
  hips: SceneNode
  legs: { root: SceneNode; mid: SceneNode; tip: SceneNode; height?: number }[]
  layers: number
  up?: Vec3
  rise?: number
  drop?: number
  plantHeight?: number
  plantBand?: number
  hipsEaseMs?: number
  rotationWeight?: number
  weight?: number
}

export type IkSolverSpec = IkLookAtSpec | IkTwoBoneSpec | IkChainSpec | IkFeetSpec

export type IkOptions = {
  /** The solvers in order: each sees the previous one's result. */
  solvers: IkSolverSpec[]
}

/** The settings of one solver kind `setSolver` can change: anything but
 * the kind and the weight (which has setWeight). */
export type IkSolverUpdate = IkSolverSpec extends infer S ? (S extends IkSolverSpec ? Partial<Omit<S, "kind" | "weight">> : never) : never

export type Ik = {
  /** Ramp solver `index`'s weight to `weight` over `fadeMs` (default 0,
   * a snap) of the model's time: the aim lifts off and the hand lets go
   * smoothly, no per-frame JS. Weight 0 costs no solve. */
  setWeight(index: number, weight: number, opts?: { fadeMs?: number }): void
  /** Move solver `index`'s target; null for none (a look-at then eases
   * back to the animated pose). A point moved to another point tracks
   * (feed a cursor every frame); a node switch, a loss or a find starts
   * the look-at's transition. Throws for a feet solver. */
  setTarget(index: number, target: IkTargetSpec | null): void
  /** Change solver `index`'s settings live: the fields given replace
   * the solver's (limits, pole, shares, method parameters, the joints
   * themselves). The weight carries over, the eases restart. */
  setSolver(index: number, update: IkSolverUpdate): void
  /** The last solve's residual: the distance left to the target (world
   * units; a look-at's angle left, radians; a feet solver's largest over
   * its planted feet) - "can it reach" without a second solve. */
  reach(index: number): number
  /** Drop the stack, the joints put back to the pose the players and
   * your frame wrote. */
  dispose(): void
}

type Stack = {
  model: Model
  solvers: IkSolverSpec[]
  id: IkId | null
}

/**
 * Declare the model's IK stack. The model must be in a scene (the stack
 * binds to live core nodes; the declaration re-binds on every scene
 * enter). Every joint must be a node under the model; targets, poles and
 * up references may be any node in the scene. Throws on a tip that is
 * not under its root, a mid not between them, a feet solver without
 * `layers`, a setting out of range, or a node out of the scene.
 */
export function createIk(model: Model, opts: IkOptions): Ik {
  if (model._node === null) throw new Error("createIk: the model must be in a scene (add() it first)")
  let solvers = opts.solvers.map((spec, i) => {
    let copy = { ...spec } as IkSolverSpec
    checkSolver(copy, model, "createIk: solvers[" + i + "]")
    return copy
  })
  let stack: Stack = { model, solvers, id: null }
  let at = (index: number, what: string): IkSolverSpec => {
    let s = solvers[index]
    if (s === undefined) throw new Error("createIk." + what + ": no solver " + index + " (the stack has " + solvers.length + ")")
    return s
  }

  let binding = {
    enter() {
      bind(stack)
    },
    leave() {
      if (stack.id !== null) spatial.destroyIk(stack.id)
      stack.id = null
    },
  }
  binding.enter()
  if (model._bindings === null) model._bindings = []
  model._bindings.push(binding)

  return {
    setWeight(index, weight, o) {
      let s = at(index, "setWeight")
      if (!(weight >= 0 && weight <= 1)) throw new Error("createIk.setWeight: the weight must be within 0..1")
      s.weight = weight
      if (stack.id !== null) spatial.setIkWeight(stack.id, index, weight, o?.fadeMs ?? 0)
    },
    setTarget(index, target) {
      let s = at(index, "setTarget")
      if (s.kind === "feet") throw new Error("createIk.setTarget: a feet solver's targets are the ground")
      s.target = target
      if (stack.id !== null) spatial.setIkTarget(stack.id, index, target === null ? null : packTarget(target, "createIk.setTarget"))
    },
    setSolver(index, update) {
      let s = at(index, "setSolver")
      let next = { ...s, ...(update as object), kind: s.kind, weight: s.weight } as IkSolverSpec
      checkSolver(next, model, "createIk.setSolver: solvers[" + index + "]")
      solvers[index] = next
      if (stack.id !== null) spatial.setIkSolver(stack.id, index, packSolver(next, model, "createIk.setSolver"))
    },
    reach(index) {
      at(index, "reach")
      return stack.id === null ? 0 : spatial.readIkReach(stack.id, index)
    },
    dispose() {
      binding.leave()
      let list = model._bindings
      if (list !== null) {
        let i = list.indexOf(binding)
        if (i >= 0) list.splice(i, 1)
      }
    },
  }
}

/** Create the stack in the core over the nodes' live ids. */
function bind(stack: Stack): void {
  let clock = stack.model._node
  if (clock === null) return
  stack.id = spatial.createIk(clock, stack.solvers.map((s) => packSolver(s, stack.model, "createIk")))
}

/** The structural checks that need no live core node: the joints under
 * the model and on descendant paths, the feet's layer; the joints marked
 * core-posed. */
function checkSolver(s: IkSolverSpec, model: Model, where: string): void {
  switch (s.kind) {
    case "lookAt": {
      if (s.bones.length === 0) throw new Error(where + " has no bones")
      for (let b of s.bones) {
        let node = "node" in b ? b.node : b
        own(node, model, where + " bone")
      }
      break
    }
    case "twoBone":
      limb(s.root, s.mid, s.tip, model, where)
      break
    case "chain": {
      let path = pathBetween(s.root, s.tip, where)
      if (path.length < 2) throw new Error(where + ": a chain needs a tip below its root")
      for (let n of path) own(n, model, where + " joint")
      break
    }
    case "feet": {
      if (s.legs.length === 0) throw new Error(where + " has no legs")
      if (typeof s.layers !== "number") throw new Error(where + ": feet need `layers`, the collider layer the ground rays see (the model's own meshes must not be on it)")
      own(s.hips, model, where + " hips")
      s.legs.forEach((l, i) => limb(l.root, l.mid, l.tip, model, where + ".legs[" + i + "]"))
      break
    }
    default:
      throw new Error(where + ": unknown solver kind " + String((s as { kind: unknown }).kind))
  }
}

function limb(root: SceneNode, mid: SceneNode, tip: SceneNode, model: Model, where: string): void {
  let path = pathBetween(root, tip, where)
  let at = path.indexOf(mid)
  if (at <= 0 || at >= path.length - 1) throw new Error(where + ": the mid must lie strictly between the root and the tip")
  for (let n of path) own(n, model, where + " joint")
}

/** A joint must be a live node under the model; the core poses it from
 * here on, so its JS mirror is stale. */
function own(node: SceneNode, model: Model, what: string): void {
  if (node._destroyed) throw new Error(what + " was destroyed")
  if (!under(node, model)) throw new Error(what + " is not a node of the model")
  node._native = true
}

/** Whether `node` is `model` itself or sits somewhere under it. */
function under(node: SceneNode, model: Model): boolean {
  for (let n: SceneNode | null = node; n !== null; n = n.parent) if (n === model) return true
  return false
}

/** The nodes from `root` down to `tip`, both included. */
function pathBetween(root: SceneNode, tip: SceneNode, where: string): SceneNode[] {
  let path: SceneNode[] = []
  for (let n: SceneNode | null = tip; n !== null; n = n.parent) {
    path.push(n)
    if (n === root) return path.reverse()
  }
  throw new Error(where + ": the tip is not under the root")
}

function idOf(node: SceneNode, what: string): NodeId {
  if (node._node === null) throw new Error(what + " is not in the scene")
  return node._node
}

function packPoint(p: IkPointSpec, what: string): CorePoint {
  if ("node" in p) return { node: idOf(p.node, what + " node") }
  return { position: p.point, space: p.space === undefined ? undefined : idOf(p.space, what + " space") }
}

function packTarget(t: IkTargetSpec, what: string): CoreTarget {
  if ("node" in t) return { node: idOf(t.node, what + " target node") }
  return { position: t.point, rotation: t.rotation, space: t.space === undefined ? undefined : idOf(t.space, what + " target space") }
}

function packLimit(l: IkLimitSpec): CoreLimit {
  if ("hinge" in l) return { kind: "hinge", axis: l.hinge, min: l.min, max: l.max }
  return { kind: "cone", swing: l.cone, twist: l.twist ?? 0 }
}

function pair(v: number | [number, number] | undefined): [number, number] {
  if (v === undefined) return UNLIMITED
  if (typeof v === "number") return [-v, v]
  return v
}

/** The scene root above the model: what the ground rays are scoped to. */
function sceneRoot(model: Model): NodeId {
  let top: SceneNode = model
  while (top.parent !== null) top = top.parent
  return idOf(top, "createIk: the scene root")
}

function packSolver(s: IkSolverSpec, model: Model, where: string): CoreSpec {
  let target = s.kind === "feet" ? undefined : s.target === null || s.target === undefined ? null : packTarget(s.target, where)
  switch (s.kind) {
    case "lookAt":
      return {
        kind: "lookAt",
        bones: s.bones.map((b) => ("node" in b ? { node: idOf(b.node, where + " bone"), share: b.share ?? 1 } : { node: idOf(b, where + " bone"), share: 1 })),
        target,
        forward: s.forward ?? DEFAULT_FORWARD,
        upAxis: s.upAxis ?? DEFAULT_UP_AXIS,
        up: s.up === undefined ? undefined : { vector: s.up.vector, space: s.up.space === undefined ? undefined : idOf(s.up.space, where + " up space") },
        limit: s.limit === undefined ? undefined : { yaw: pair(s.limit.yaw), pitch: pair(s.limit.pitch), damp: s.limit.damp ?? LIMIT_DAMP },
        easeMs: s.easeMs ?? LOOK_AT_EASE_MS,
        weight: s.weight ?? 1,
      }
    case "twoBone":
      return {
        kind: "twoBone",
        root: idOf(s.root, where + " root"),
        mid: idOf(s.mid, where + " mid"),
        tip: idOf(s.tip, where + " tip"),
        target,
        pole: s.pole === undefined ? undefined : packPoint(s.pole, where + " pole"),
        poleAngle: s.poleAngle ?? 0,
        positionWeight: s.positionWeight ?? 1,
        rotationWeight: s.rotationWeight ?? 0,
        soft: s.soft ?? 0,
        stretch: s.stretch ?? 0,
        weight: s.weight ?? 1,
      }
    case "chain":
      return {
        kind: "chain",
        root: idOf(s.root, where + " root"),
        tip: idOf(s.tip, where + " tip"),
        target,
        method: s.method ?? "fabrik",
        iterations: s.iterations ?? CHAIN_ITERATIONS,
        tolerance: s.tolerance ?? CHAIN_TOLERANCE,
        magnet: s.magnet === undefined ? undefined : packPoint(s.magnet, where + " magnet"),
        limits: (s.limits ?? []).map((l) => ({ node: idOf(l.node, where + " limit node"), limit: packLimit(l.limit) })),
        rotationWeight: s.rotationWeight ?? 0,
        weight: s.weight ?? 1,
      }
    case "feet":
      return {
        kind: "feet",
        hips: idOf(s.hips, where + " hips"),
        legs: s.legs.map((l) => ({ root: idOf(l.root, where + " leg root"), mid: idOf(l.mid, where + " leg mid"), tip: idOf(l.tip, where + " leg tip"), height: l.height })),
        up: s.up ?? WORLD_UP,
        filter: { root: sceneRoot(model), layers: s.layers },
        rise: s.rise ?? FEET_RISE,
        drop: s.drop ?? FEET_DROP,
        plantHeight: s.plantHeight ?? PLANT_HEIGHT,
        plantBand: s.plantBand ?? PLANT_BAND,
        hipsEaseMs: s.hipsEaseMs ?? HIPS_EASE_MS,
        rotationWeight: s.rotationWeight ?? 1,
        weight: s.weight ?? 1,
      }
  }
}
