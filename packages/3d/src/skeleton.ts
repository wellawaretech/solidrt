// Shared skeletons. Wardrobe pieces (a hood, a cape, cuffs) export with
// a skin over the body's joints - the same joint names, their own inverse
// binds - and no clips of their own, so a piece added beside an animated
// body holds its bind pose. bindSkeleton makes the piece's skin read the
// BODY's joints instead of its own: each matched joint's palette row is
// re-bound onto the body's node, with the piece's inverse bind as the
// row's post-multiply, so the frame's flush writes the body's pose into
// the piece's palette and no per-frame code exists anywhere. One
// skeleton, many skinned meshes: Three's SkinnedMesh.bind(skeleton),
// Unity's SkinnedMeshRenderer.bones, Godot's MeshInstance3D.skeleton.
//
// The piece's own node tree is deliberately NOT posed or composed:
// exporters truncate it above (a hat's table starts at the spine, the
// missing ancestors folded into its first local) or skip an ancestor in
// the middle (a cape parents Spine_1 straight to Root_0), and copying
// locals through such a tree hangs joints off the wrong place while
// looking right for every piece whose tree happens to match. A bone
// matrix needs the joint's WORLD matrix, which the body's node has, and
// the piece's inverse binds agree with the body's whenever the piece
// renders at rest beside it - checked at bind against the body's own.
// Joints with no body counterpart (a hat's internal bones) keep their own
// node, grafted under the nearest matched body joint so they ride the
// pose; so does any rigid part hanging off a matched node.

import * as spatial from "flux:spatial"
import type { TextureId } from "@solidrt/core/gpu"
import { add, destroy, remove, setTransform } from "./node.ts"
import type { SceneNode } from "./node.ts"
import { reparentInstance } from "./mesh.ts"
import type { InstancedMesh, InstanceNode } from "./mesh.ts"
import type { Model } from "./model.ts"

/** Inverse-bind entries of a piece and its body differ by exporter
 * rounding (1e-6 or so); a piece exported against another rest pose or
 * scale is off by centimetres. The gate between the two. */
const BIND_TOLERANCE = 1e-3

/** A piece bound onto a body's skeleton: the piece, and its matched
 * joints' influence boxes (in joint space) keyed by the body joint that
 * now drives them - the body's cull-bounds union takes them in. */
export type WornPiece = {
  piece: Model
  boxes: { joint: SceneNode; box: Float32Array }[]
}

export type BindSkeletonOptions = {
  /** Map a piece node's name to the body name to look up (still
   * case-insensitive), for a pipeline whose joint names differ by a
   * prefix or suffix: Mixamo's `mixamorig:`, a one-sided `_JNT`. Unity
   * matches exact names and leaves the rest to the app; this is that
   * hook. Default the identity. */
  match?: (pieceName: string) => string
}

/**
 * Drive `piece`'s skins from `body`'s skeleton: a wardrobe piece exported
 * over the body's joints (matched by name, case-insensitive, through
 * `match` when the names differ by a fixed prefix or suffix) with no
 * clips of its own. After this the piece hangs under the body at the
 * body's placement, its skinned parts follow every pose the body takes -
 * mixer-driven or hand-posed - and its joints with no body counterpart
 * ride the nearest matched body joint. The piece's own joint nodes are
 * no longer posed by anything; socket items on the body's joints. Throws
 * when the piece has no skin, shares no joint name with the body, or a
 * shared joint's bind pose differs (a piece exported against another
 * rest pose). A piece is taken off by disposing it; disposing the body
 * disposes what it wears.
 */
export function bindSkeleton(body: Model, piece: Model, opts: BindSkeletonOptions = {}): void {
  if (piece === body) throw new Error("bindSkeleton: a model cannot wear itself")
  if (opts.match !== undefined && typeof opts.match !== "function") throw new Error("bindSkeleton: match must be a function from a piece name to a body name")
  if (piece._body !== null) throw new Error("bindSkeleton: the piece is already bound to a body")
  if (body._body !== null) throw new Error("bindSkeleton: the body is itself a worn piece")
  if (piece._skins.length === 0) throw new Error("bindSkeleton: the piece has no skin")

  // Out of the scene first: the piece's rows must release before they
  // re-bind under a different anchor (one anchor per palette texture),
  // and the add() under the body re-enters everything below.
  remove(piece)
  // The piece's shared parts re-anchor on the BODY root: the piece sits at
  // identity under it (below), so the mesh's world still equals the
  // anchor's, and a copy grafted under a body joint stays inside the
  // anchor's subtree. The records rebind at the re-enter.
  for (let part of piece.parts) {
    if (part.instances !== undefined) (part.mesh as InstancedMesh)._instances.anchor = body
  }

  let bodyByName = new Map<string, SceneNode>()
  for (let n of body.nodes) {
    let key = n.name.toLowerCase()
    if (!bodyByName.has(key)) bodyByName.set(key, n.node)
  }
  let bodyBind = new Map<SceneNode, Float32Array>()
  for (let s of body._skins) s.joints.forEach((j, k) => bodyBind.set(j, s.inverseBind.subarray(k * 16, k * 16 + 16)))
  let match = new Map<SceneNode, SceneNode>()
  let names = new Map<SceneNode, string>()
  let bodyName = opts.match ?? ((name: string) => name)
  for (let n of piece.nodes) {
    names.set(n.node, n.name)
    let b = bodyByName.get(bodyName(n.name).toLowerCase())
    if (b !== undefined) match.set(n.node, b)
  }
  if (match.size === 0) throw new Error("bindSkeleton: no node of the piece is named like a node of the body" + (opts.match ? " (through match)" : ""))

  // Matched rows move onto the body's joints. Identical skins share one
  // texture, so its rows move with the first skin naming it; the check
  // and the boxes run for every skin regardless.
  let rebound = new Set<TextureId>()
  let boxes: WornPiece["boxes"] = []
  for (let s of piece._skins) {
    let first = !rebound.has(s.texture)
    rebound.add(s.texture)
    s.joints.forEach((joint, k) => {
      let b = match.get(joint)
      if (b === undefined) return
      let post = s.inverseBind.subarray(k * 16, k * 16 + 16)
      let own = bodyBind.get(b)
      if (own !== undefined && !sameMatrix(own, post)) {
        throw new Error("bindSkeleton: joint '" + names.get(joint) + "' has a different bind pose on the piece than on the body")
      }
      let box = s.jointBounds.subarray(k * 6, k * 6 + 6)
      if (validBox(box)) boxes.push({ joint: b, box: Float32Array.from(box) })
      if (!first || joint._palettes === null) return
      let at = joint._palettes.findIndex((p) => p.texture === s.texture)
      if (at < 0) return
      let row = joint._palettes.splice(at, 1)[0]!
      if (joint._palettes.length === 0) joint._palettes = null
      ;(b._palettes ??= []).push({ texture: s.texture, row: k, post: row.post, anchor: body })
      if (b._node !== null && body._node !== null) spatial.bindTextureSlot(b._node, s.texture, k, row.post, body._node)
    })
  }
  // The rows that stay on the piece's own nodes (unmatched joints) share
  // the texture's one anchor: the body root, an ancestor of every bound
  // node once the piece and its grafts hang under the body.
  for (let n of piece.nodes) {
    if (n.node._palettes === null) continue
    for (let p of n.node._palettes) if (p.anchor === piece) p.anchor = body
  }
  for (let part of piece.parts) {
    let joints = part.mesh._cullJoints
    if (joints !== null) part.mesh._cullJoints = joints.map((j) => match.get(j) ?? j)
  }
  // Graft: whatever hangs off a matched node that is not itself matched
  // (an unmatched joint subtree, a rigid part) moves under the body's
  // node with its local transform intact - the two nodes share a bind
  // pose, so the local means the same thing there.
  for (let [node, bodyNode] of match) {
    for (let child of node.children.slice()) {
      if (match.has(child)) continue
      // A shared part's copy under a matched node (its placement node IS
      // a joint) rides the body's joint like any rigid part: slot-bound,
      // so it moves by reparentInstance rather than add, against the
      // body-root anchor set above.
      if (child.kind === "instance") reparentInstance(child as InstanceNode, bodyNode)
      else add(bodyNode, child)
      piece._grafts.push(child)
    }
  }

  piece._body = body
  body._worn.push({ piece, boxes })
  refreshJointBounds(body)
  // A piece's skinned vertices are in the body's model space, so it takes
  // the body's placement and nothing else.
  setTransform(piece, { position: [0, 0, 0], quaternion: [0, 0, 0, 1], scale: [1, 1, 1] })
  add(body, piece)
}

/** @internal Take a worn piece off its body: its rows leave the body's
 * joints, its grafts detach, the body's joint boxes shrink back. The
 * piece's own bindings are not restored - dispose is the one caller, and
 * it destroys the piece's palette textures right after. */
export function unbindSkeleton(piece: Model): void {
  let body = piece._body
  if (body === null) return
  let mine = new Set(piece._skins.map((s) => s.texture))
  for (let n of body.nodes) {
    let rows = n.node._palettes
    if (rows === null) continue
    let keep = rows.filter((p) => !mine.has(p.texture))
    if (keep.length === rows.length) continue
    if (n.node._node !== null) {
      for (let p of rows) if (mine.has(p.texture)) spatial.unbindTextureSlot(n.node._node, p.texture)
    }
    n.node._palettes = keep.length > 0 ? keep : null
  }
  // A grafted group detaches; a grafted instance is slot-bound and goes
  // by destroy (the piece's populations are disposed right after, by the
  // one caller, dispose).
  for (let g of piece._grafts) {
    if (g.kind === "instance") destroy(g)
    else remove(g)
  }
  piece._grafts = []
  body._worn.splice(body._worn.findIndex((w) => w.piece === piece), 1)
  piece._body = null
  refreshJointBounds(body)
}

/** @internal Recompute every joint's culling box from the model's skins
 * and the pieces it wears: the union, per joint, of each skin's joint-
 * space influence box (a body/legs split and a worn hood all weight the
 * head), applied to the live core node where it changed. A joint no skin
 * reaches gets none and contributes nothing to a part's cull group. */
export function refreshJointBounds(model: Model): void {
  let boxes = new Map<SceneNode, Float32Array>()
  let grow = (joint: SceneNode, box: Float32Array): void => {
    if (!validBox(box)) return
    let have = boxes.get(joint)
    if (have === undefined) {
      boxes.set(joint, Float32Array.from(box))
      return
    }
    for (let i = 0; i < 3; i++) {
      have[i] = Math.min(have[i]!, box[i]!)
      have[i + 3] = Math.max(have[i + 3]!, box[i + 3]!)
    }
  }
  for (let s of model._skins) s.joints.forEach((j, k) => grow(j, s.jointBounds.subarray(k * 6, k * 6 + 6)))
  for (let w of model._worn) for (let b of w.boxes) grow(b.joint, b.box)
  for (let n of model.nodes) {
    let box = boxes.get(n.node) ?? null
    if (sameBox(n.node._cullBounds, box)) continue
    n.node._cullBounds = box
    if (n.node._node !== null) spatial.setCullBounds(n.node._node, box)
  }
}

/** A joint influencing no vertex carries an inverted box (min > max). */
function validBox(box: Float32Array): boolean {
  return box[0]! <= box[3]! && box[1]! <= box[4]! && box[2]! <= box[5]!
}

function sameBox(a: Float32Array | null, b: Float32Array | null): boolean {
  if (a === null || b === null) return a === b
  for (let i = 0; i < 6; i++) if (a[i] !== b[i]) return false
  return true
}

function sameMatrix(a: Float32Array, b: Float32Array): boolean {
  for (let i = 0; i < 16; i++) if (Math.abs(a[i]! - b[i]!) > BIND_TOLERANCE) return false
  return true
}
