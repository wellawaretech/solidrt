// Check rig for the profile kit's solids (src/sweep.ts, src/profile.ts):
// extrude / lathe / sweep / tube / shape structure - index ranges, unit
// normals, cap orientation and placement, bevel clamping, lathe angle
// rejection, the tube -> sweep pass-through - and the generator layout
// option's byte identity with withColors. Pure-module inputs only, so it
// runs headless on flux, bundled from the repo root:
//
//   bunx srt bundle -f --stdout packages/3d/checks/sweep-check.ts | target/release/flux -
//
// A failure prints FAIL lines and throws at the end, so the run exits nonzero.

import { extrude, lathe, pathFrames, sweep, tube } from "../src/sweep.ts"
import { roundRect, shape } from "../src/profile.ts"
import type { Profile } from "../src/profile.ts"
import { geometryBounds, layoutKey, layoutStride, validateGeometry, withColors, STANDARD_FLOATS } from "../src/geometry.ts"
import type { Geometry } from "../src/geometry.ts"
import type { Vec3 } from "../src/math.ts"

let failures = 0
let fail = (msg: string): void => {
  failures++
  console.log("FAIL:", msg)
}
let near = (a: number, b: number, eps = 1e-5): boolean => Math.abs(a - b) <= eps
let throws = (label: string, fn: () => unknown): void => {
  try {
    fn()
    fail(`${label}: did not throw`)
  } catch {
    // expected
  }
}

// Structural invariants every solid here must satisfy.
let structure = (name: string, g: Geometry): void => {
  try {
    validateGeometry(g)
  } catch (e) {
    fail(`${name}: ${String(e)}`)
    return
  }
  let stride = layoutStride(g.layout)
  let count = g.vertices.length / stride
  if (g.indices.length % 3 !== 0) fail(`${name}: index count ${g.indices.length} not triangles`)
  for (let i = 0; i < g.indices.length; i++) {
    if (g.indices[i]! >= count) {
      fail(`${name}: index ${g.indices[i]} out of range (${count} vertices)`)
      break
    }
  }
  for (let i = 0; i < count; i++) {
    let d = i * stride
    let len = Math.hypot(g.vertices[d + 3]!, g.vertices[d + 4]!, g.vertices[d + 5]!)
    if (!near(len, 1, 1e-4)) {
      fail(`${name}: vertex ${i} normal length ${len.toFixed(5)}`)
      break
    }
    let u = g.vertices[d + 6]!
    let v = g.vertices[d + 7]!
    if (u < -1e-6 || u > 1 + 1e-6 || v < -1e-6 || v > 1 + 1e-6) {
      fail(`${name}: vertex ${i} uv (${u}, ${v}) outside 0..1`)
      break
    }
  }
}

// Triangle winding check against an outward direction: for a convex solid
// centered near `center`, every face normal (CCW winding) should point
// away from the center.
let outward = (name: string, g: Geometry, center: Vec3): void => {
  let stride = layoutStride(g.layout)
  let at = (i: number): Vec3 => [g.vertices[i * stride]!, g.vertices[i * stride + 1]!, g.vertices[i * stride + 2]!]
  let bad = 0
  for (let i = 0; i < g.indices.length; i += 3) {
    let a = at(g.indices[i]!)
    let b = at(g.indices[i + 1]!)
    let c = at(g.indices[i + 2]!)
    let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]]
    let e2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]]
    let n = [e1[1]! * e2[2]! - e1[2]! * e2[1]!, e1[2]! * e2[0]! - e1[0]! * e2[2]!, e1[0]! * e2[1]! - e1[1]! * e2[0]!]
    let m = [(a[0] + b[0] + c[0]) / 3 - center[0], (a[1] + b[1] + c[1]) / 3 - center[1], (a[2] + b[2] + c[2]) / 3 - center[2]]
    let dot = n[0]! * m[0]! + n[1]! * m[1]! + n[2]! * m[2]!
    let area = Math.hypot(n[0]!, n[1]!, n[2]!)
    if (area > 1e-9 && dot < 0) bad++
  }
  if (bad > 0) fail(`${name}: ${bad} inward-facing triangles`)
}

let square: Profile = [[-0.5, -0.5], [0.5, -0.5], [0.5, 0.5], [-0.5, 0.5]]

// extrude: a plain box-like prism - 4 creased profile points make 8 ring
// entries plus the seam duplicate (u = 1), 2 slices, plus two 4-vertex caps.
{
  let g = extrude(square, { depth: 2 })
  structure("extrude plain", g)
  outward("extrude plain", g, [0, 0, 0])
  let count = g.vertices.length / STANDARD_FLOATS
  if (count !== 9 * 2 + 4 * 2) fail("extrude plain vertex count: " + count)
  let b = geometryBounds(g)
  if (!near(b[2]!, -1) || !near(b[5]!, 1)) fail("extrude depth centered: " + b[2] + ".." + b[5])
  if (!near(b[0]!, -0.5) || !near(b[3]!, 0.5)) fail("extrude profile extent")
}

// extrude with bevel: bounds shrink nowhere (bevel is inset into the
// prism), slices grow with bevelSegments, bevel clamps below half depth.
{
  let g = extrude(square, { depth: 1, bevel: 0.1, bevelSegments: 3 })
  structure("extrude bevel", g)
  outward("extrude bevel", g, [0, 0, 0])
  let b = geometryBounds(g)
  if (!near(b[5]!, 0.5) || !near(b[3]!, 0.5)) fail("extrude bevel bounds")
  let count = g.vertices.length / STANDARD_FLOATS
  if (count !== 9 * 8 + 4 * 2) fail("extrude bevel vertex count: " + count)
  let huge = extrude(square, { depth: 1, bevel: 5, bevelSegments: 2 })
  structure("extrude bevel clamp", huge)
  let hb = geometryBounds(huge)
  if (!near(hb[5]!, 0.5)) fail("extrude bevel clamp keeps depth")
}

// shape: one flat face, facing +z, UVs mapping the box.
{
  let g = shape(roundRect(1, 0.5, 0.1, 3))
  structure("shape", g)
  let stride = STANDARD_FLOATS
  let count = g.vertices.length / stride
  for (let i = 0; i < count; i++) {
    if (!near(g.vertices[i * stride + 5]!, 1)) {
      fail("shape normal +z")
      break
    }
  }
  if (g.indices.length !== (count - 2) * 3) fail("shape triangulation count: " + g.indices.length)
  if (shape(square, { label: "sq" }).label !== "sq") fail("shape label option")
}

// lathe: a closed ring profile (tube wall) revolved fully is watertight-ish:
// no caps, every vertex on a circle of its profile radius.
let wall: Profile = [[0.3, -0.5], [0.5, -0.5], [0.5, 0.5], [0.3, 0.5]]
{
  let g = lathe(wall, { segments: 12 })
  structure("lathe full", g)
  // A hollow ring is not convex (its inner wall faces the axis), so the
  // winding check uses a profile run to the axis: a solid cylinder.
  let solid = lathe([[0, -0.5], [0.5, -0.5], [0.5, 0.5], [0, 0.5]], { segments: 12 })
  structure("lathe solid", solid)
  outward("lathe solid", solid, [0, 0, 0])
  let b = geometryBounds(g)
  if (!near(b[0]!, -0.5) || !near(b[3]!, 0.5) || !near(b[2]!, -0.5) || !near(b[5]!, 0.5)) fail("lathe radius bounds")
  if (!near(b[1]!, -0.5) || !near(b[4]!, 0.5)) fail("lathe height bounds")
  let half = lathe(wall, { segments: 12, angle: Math.PI })
  structure("lathe half", half)
  if (half.vertices.length <= g.vertices.length / 2) fail("lathe half carries caps")
  let hb = geometryBounds(half)
  if (hb[5]! > 1e-6 && hb[2]! < -1e-6) fail("lathe half spans both z signs: " + hb[2] + ".." + hb[5])
  throws("lathe zero angle", () => lathe(wall, { angle: 0 }))
  throws("lathe over full", () => lathe(wall, { angle: Math.PI * 2 + 0.1 }))
}

// sweep: a straight path along +z equals an extrude in extent; a bent path
// keeps structure; creased vs smooth points change vertex counts.
let straight: Vec3[] = [[0, 0, -1], [0, 0, 1]]
{
  let g = sweep(square, straight)
  structure("sweep straight", g)
  outward("sweep straight", g, [0, 0, 0])
  let b = geometryBounds(g)
  if (!near(b[2]!, -1) || !near(b[5]!, 1)) fail("sweep straight extent")
  if (!near(b[0]!, -0.5) || !near(b[4]!, 0.5)) fail("sweep straight profile extent")

  let bent = sweep(square, [[0, 0, 0], [0, 0, 1], [1, 0, 2], [1, 1, 3]])
  structure("sweep bent", bent)
  let smooth = sweep(square, [[0, 0, 0], { p: [0, 0, 1], smooth: true }, [1, 0, 2]])
  let creased = sweep(square, [[0, 0, 0], [0, 0, 1], [1, 0, 2]])
  structure("sweep smooth", smooth)
  structure("sweep creased", creased)
  if (creased.vertices.length <= smooth.vertices.length) fail("creased joint duplicates its ring")
  throws("sweep one point", () => sweep(square, [[0, 0, 0]]))
}

// pathFrames: tangents unit, cross axes perpendicular, lengths cumulative.
{
  let f = pathFrames([[0, 0, 0], [0, 0, 2], [3, 0, 2]])
  if (f.lengths.length !== 3 || !near(f.lengths[2]!, 5)) fail("pathFrames lengths: " + f.lengths)
  for (let i = 0; i < f.tangents.length; i++) {
    let t = f.tangents[i]!
    let x = f.xAxes[i]!
    let y = f.yAxes[i]!
    if (!near(Math.hypot(...t), 1) || !near(Math.hypot(...x), 1) || !near(Math.hypot(...y), 1)) fail("pathFrames unit axes")
    if (!near(t[0] * x[0] + t[1] * x[1] + t[2] * x[2], 0) || !near(x[0] * y[0] + x[1] * y[1] + x[2] * y[2], 0)) fail("pathFrames orthogonal")
  }
}

// tube: the round-profile sweep - radius bounds and radialSegments ring.
{
  let g = tube(straight, { radius: 0.25, radialSegments: 8 })
  structure("tube", g)
  outward("tube", g, [0, 0, 0])
  let b = geometryBounds(g)
  if (!near(b[0]!, -0.25) || !near(b[4]!, 0.25)) fail("tube radius bounds")
  if (tube(straight, { label: "t" }).label !== "t") fail("tube label option")
  let d = tube(straight)
  let bd = geometryBounds(d)
  if (!near(bd[3]!, 0.5)) fail("tube default radius")
}

// Layout option: one-pass wide emission equals generate-then-withColors.
{
  let same = (name: string, std: Geometry, wide: Geometry) => {
    let via = withColors(std, () => [0, 0, 0, 0])
    if (layoutKey(wide.layout) !== layoutKey("colored")) fail(name + ": wide layout")
    if (via.vertices.length !== wide.vertices.length) fail(name + ": wide length")
    for (let i = 0; i < via.vertices.length; i++) {
      if (via.vertices[i] !== wide.vertices[i]) {
        fail(name + ": wide bytes differ at " + i)
        break
      }
    }
    if (via.indices.length !== wide.indices.length) fail(name + ": wide indices")
  }
  same("extrude", extrude(square, { bevel: 0.1 }), extrude(square, { bevel: 0.1, layout: "colored" }))
  same("lathe", lathe(wall, { segments: 6, angle: 2 }), lathe(wall, { segments: 6, angle: 2, layout: "colored" }))
  same("sweep", sweep(square, straight), sweep(square, straight, { layout: "colored" }))
  same("tube", tube(straight, { radialSegments: 5 }), tube(straight, { radialSegments: 5, layout: "colored" }))
  same("shape", shape(square), shape(square, { layout: "colored" }))
}

if (failures > 0) throw new Error(failures + " sweep check(s) failed")
console.log("PASS: profile kit solids")
