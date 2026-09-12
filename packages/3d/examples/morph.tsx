// Morph targets (blend shapes): a geometry carries named per-vertex
// deltas (`withMorphTargets`; a glTF's targets arrive the same way
// through the loader) and a `morph: true` material displaces its
// vertices by the weights the mesh owns - `setMorphWeights(mesh, {
// bulge: 0.7 })` by name, or every weight in target order, or the
// `morphWeights` prop. The weights live in the spatial core (one write
// lands at the next flush, a clip's weights track writes the same
// register), the deltas in one texture per geometry packed sparse by
// vertex, so a target that moves the top of the blob costs nothing at
// the bottom. The middle blob morphs, its shadow morphs with it (the
// depth pass walks the same targets); the left one draws the same
// geometry under a plain lit material - the base shape, targets
// ignored; the right one flips between two weight sets through the
// `morphWeights` prop, and its `transition` declares a `weights` lane,
// so each flip springs there natively - a write is a target, like a
// transform write under a transition. The back row is ONE instanced
// mesh: every `<Instance>` owns its own weights (a row of the
// population's weights texture, read by the instance index in the
// shader), so six copies at six blends are one draw entry - and their
// shadows morph too.

import { createSignal, onCleanup, onFrame, pct, render } from "@solidrt/core"
import { registerDebug } from "srt:dev"
import { DirectionalLight, geometryAttribute, geometryVertexCount, getMorphNames, getMorphWeights, HemisphereLight, Instance, InstancedMesh, phong, Mesh, PerspectiveCamera, plane, Scene, setMorphWeights, sphere, withMorphTargets } from "@solidrt/3d"
import type { InstanceNode, MeshNode } from "@solidrt/3d"

// The blob's authored shape: a sphere with two targets - "bulge", a
// spike growing out of the top half only (the sparse case), and
// "squash", the whole sphere flattened to half its height. Each target
// carries NORMAL deltas too, the way a modeller exports them: the
// normals of the displaced shape (smooth normals recomputed from its
// faces) minus the base normals. Without them the lighting and the
// shadow bias would read the base surface on a moved one. Normals
// blend LINEARLY between targets (every engine's morph rule), which is
// exact at the targets and an approximation between them: keep deltas
// modest, as a modeller's blend shapes are, or a half-blended extreme
// target shows a ragged terminator against its own shadow.
const RADIUS = 0.6
const BULGE_REACH = 0.45
const SQUASH_TO = 0.6
// The right blob's two poses and how often it flips between them; its
// spring takes about a second to settle, with a little bounce.
const POSE_A = { squash: 0.8, bulge: 0.1 }
const POSE_B = { squash: 0.1, bulge: 0.8 }
const FLIP_MS = 1800
const FLIP_SPRING = { duration: 900, bounce: 0.25 }
// The back row: six instances of one mesh, each at its own blend - the
// bulge rising left to right, the squash falling.
const ROW_COUNT = 6
const ROW_SPACING = 1.1
const ROW_Z = -2.2
const ROW_SCALE = 0.7

// Smooth vertex normals of `positions` over the geometry's triangles.
function smoothNormals(indices: ArrayLike<number>, positions: Float32Array): Float32Array {
  let normals = new Float32Array(positions.length)
  for (let t = 0; t + 2 < indices.length; t += 3) {
    let a = indices[t]! * 3, b = indices[t + 1]! * 3, c = indices[t + 2]! * 3
    let abx = positions[b]! - positions[a]!, aby = positions[b + 1]! - positions[a + 1]!, abz = positions[b + 2]! - positions[a + 2]!
    let acx = positions[c]! - positions[a]!, acy = positions[c + 1]! - positions[a + 1]!, acz = positions[c + 2]! - positions[a + 2]!
    let nx = aby * acz - abz * acy, ny = abz * acx - abx * acz, nz = abx * acy - aby * acx
    for (let v of [a, b, c]) {
      normals[v] = normals[v]! + nx
      normals[v + 1] = normals[v + 1]! + ny
      normals[v + 2] = normals[v + 2]! + nz
    }
  }
  for (let v = 0; v < normals.length; v += 3) {
    let len = Math.hypot(normals[v]!, normals[v + 1]!, normals[v + 2]!) || 1
    normals[v] = normals[v]! / len
    normals[v + 1] = normals[v + 1]! / len
    normals[v + 2] = normals[v + 2]! / len
  }
  return normals
}

function blob() {
  let base = sphere({ radius: RADIUS, label: "blob" })
  let count = geometryVertexCount(base, "blob")
  let pos = geometryAttribute(base, "aPos")!
  let nrm = geometryAttribute(base, "aNormal")!
  let basePos = new Float32Array(count * 3)
  let baseNormal = new Float32Array(count * 3)
  let bulge = new Float32Array(count * 3)
  let squash = new Float32Array(count * 3)
  for (let i = 0; i < count; i++) {
    let y = pos.get(i, 1)
    for (let k = 0; k < 3; k++) {
      basePos[i * 3 + k] = pos.get(i, k)
      baseNormal[i * 3 + k] = nrm.get(i, k)
    }
    let up = Math.max(0, y / RADIUS)
    let reach = BULGE_REACH * up * up
    for (let k = 0; k < 3; k++) bulge[i * 3 + k] = nrm.get(i, k) * reach
    squash[i * 3 + 1] = -(1 - SQUASH_TO) * y
  }
  // A target's normal deltas: the displaced shape's smooth normals minus
  // the base's (the sphere's seam vertices are duplicated, so their
  // recomputed normals differ a little across the seam - fine here).
  let normalDeltas = (deltas: Float32Array): Float32Array => {
    let moved = basePos.map((v, i) => v + deltas[i]!)
    let normals = smoothNormals(base.indices, moved)
    return normals.map((n, i) => n - baseNormal[i]!)
  }
  return withMorphTargets(base, [
    { name: "bulge", position: bulge, normal: normalDeltas(bulge) },
    { name: "squash", position: squash, normal: normalDeltas(squash) },
  ])
}

function App() {
  let geometry = blob()
  let morphing = phong({ color: [0.9, 0.55, 0.3], morph: true, specular: 0.4, shininess: 40 })
  let plain = phong({ color: [0.35, 0.6, 0.85] })
  let ground = phong({ color: [0.75, 0.75, 0.72] })
  let live: MeshNode | undefined
  let right: MeshNode | undefined
  let row: InstanceNode[] = []
  let instanced = phong({ color: [0.45, 0.7, 0.5], morph: true, instanced: true, specular: 0.4, shininess: 40 })
  let rowWeights = (i: number) => ({ bulge: i / (ROW_COUNT - 1), squash: 1 - i / (ROW_COUNT - 1) })
  // The per-frame driver, until a debug command pins the weights:
  // `POST /__control__/debug?name=morph` with `{ "bulge": 1, "squash": 0 }`
  // (or the MCP call_debug tool) holds the middle blob at a weight set
  // and reads the weights back (from the core: mid-flight under a
  // transition); with `"duration": 500` the pin animates there on a
  // one-off spring; `{ "drive": true }` resumes the driver; `{}` only
  // reads.
  let driving = true
  onFrame(tick => {
    let t = tick / 1000
    if (live && driving) setMorphWeights(live, { bulge: (Math.sin(t * 1.7) + 1) / 2, squash: (Math.sin(t * 0.9 + 1) + 1) / 2 })
  })
  registerDebug("morph", (args?: { bulge?: number; squash?: number; duration?: number; drive?: boolean }) => {
    if (!live) return null
    if (args?.drive === true) driving = true
    else {
      let weights: Record<string, number> = {}
      if (typeof args?.bulge === "number") weights.bulge = args.bulge
      if (typeof args?.squash === "number") weights.squash = args.squash
      if (Object.keys(weights).length > 0) {
        driving = false
        setMorphWeights(live, weights, typeof args?.duration === "number" ? { duration: args.duration } : undefined)
      }
    }
    return { names: getMorphNames(live), weights: Array.from(getMorphWeights(live)), right: right ? Array.from(getMorphWeights(right)) : null, row: row.map(n => Array.from(getMorphWeights(n))) }
  })
  // The right blob's pose flips on a timer; the spring is the node's.
  let [pose, setPose] = createSignal(POSE_A)
  let flip = setInterval(() => setPose(p => (p === POSE_A ? POSE_B : POSE_A)), FLIP_MS)
  onCleanup(() => clearInterval(flip))

  return (
    <window>
      <view width={pct(100)} height={pct(100)}>
        <Scene clearColor={[0.08, 0.08, 0.11, 1]} samples={4} label="morph">
          <PerspectiveCamera fov={45} position={[0, 2.4, 5.5]} lookAt={[0, 0.5, 0]} />
          <HemisphereLight sky={[0.35, 0.38, 0.45]} ground={[0.12, 0.1, 0.08]} />
          <DirectionalLight position={[-2, 5, -3]} direction={[2, -5, 3]} color={[1, 0.95, 0.85]} intensity={1} castShadow shadow={{ normalBias: 0.02, camera: { near: 1, far: 20 } }} />
          <Mesh geometry={plane({ width: 8, height: 8 })} material={ground} rotation={[-Math.PI / 2, 0, 0]} />
          <Mesh geometry={geometry} material={plain} position={[-1.8, RADIUS, 0]} castShadow />
          <Mesh geometry={geometry} material={morphing} position={[0, RADIUS, 0]} castShadow ref={m => (live = m)} />
          <Mesh geometry={geometry} material={morphing} position={[1.8, RADIUS, 0]} castShadow morphWeights={pose()} transition={{ weights: FLIP_SPRING }} ref={m => (right = m)} />
          <InstancedMesh geometry={geometry} material={instanced} capacity={ROW_COUNT} castShadow label="morph-row">
            {Array.from({ length: ROW_COUNT }, (_, i) => (
              <Instance position={[(i - (ROW_COUNT - 1) / 2) * ROW_SPACING, RADIUS * ROW_SCALE, ROW_Z]} scale={ROW_SCALE} morphWeights={rowWeights(i)} ref={n => (row[i] = n)} />
            ))}
          </InstancedMesh>
        </Scene>
      </view>
    </window>
  )
}

render(() => <App />)
