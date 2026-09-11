// Check rig for the geometry-as-data ops (src/geometry.ts): transformGeometry
// against hand-computed points and normals (non-uniform scale included),
// mergeGeometries offsets, uint32 widening and the mixed-layout rejection,
// the exported bounds/ray helpers, and the debug helper builders (counts,
// bounds, the color channel). Pure-module inputs only, so it runs
// headless on flux, bundled from the repo root:
//
//   bunx srt bundle -f --stdout packages/3d/checks/geometry-check.ts | target/release/flux -
//
// A failure prints FAIL lines and throws at the end, so the run exits nonzero.

import { arrowHelper, axesHelper, box, box3Helper, cylinder, dodecahedron, edgesGeometry, validateGeometry, fillAttribute, fillColors, gridHelper, icosahedron, octahedron, packGeometry, planeHelper, polyhedron, sphere, tetrahedron, torus, torusKnot, geometryAttribute, geometryBounds, geometryKey, geometrySlot, geometryStreams, geometryVertexCount, layoutKey, layoutSlot, layoutStride, mergeGeometries, plane, transformGeometry, wireframeGeometry, vertexBytes, vertexCount, withAttribute, withColors, STANDARD_FLOATS, VERTEX_FORMATS, VERTEX_LAYOUTS } from "../src/geometry.ts"
import { cross, normalize, rayBoxDistance, sub } from "../src/math.ts"
import type { Geometry, PolyhedronOptions } from "../src/geometry.ts"
import type { VertexFormat } from "@solidrt/core/gpu"
import type { Vec3 } from "../src/math.ts"

let failures = 0
let fail = (msg: string): void => {
  failures++
  console.log("FAIL:", msg)
}
let near = (a: number, b: number, eps = 1e-5): boolean => Math.abs(a - b) <= eps
let expectVec = (label: string, got: ArrayLike<number>, want: ArrayLike<number>): void => {
  for (let i = 0; i < want.length; i++) {
    if (!near(got[i]!, want[i]!)) {
      fail(`${label}: got [${Array.from(got as number[]).map((v) => v.toFixed(4))}] want [${Array.from(want as number[]).map((v) => v.toFixed(4))}]`)
      return
    }
  }
}
let throws = (label: string, fn: () => unknown): void => {
  try {
    fn()
    fail(`${label}: did not throw`)
  } catch {
    // expected
  }
}

// Vertex i of channel `name` as floats, through the accessor.
let read = (g: Geometry, name: string, i: number): number[] => {
  let a = geometryAttribute(g, name)
  if (a === null) {
    fail("read: no " + name + " channel")
    return []
  }
  let out: number[] = []
  for (let k = 0; k < a.components; k++) out.push(a.get(i, k))
  return out
}

// One-triangle geometry with a known position and normal.
let tri = (): Geometry => ({
  vertices: new Float32Array([
    1, 0, 0, 0, 0, 1, 0, 0,
    0, 1, 0, 0, 0, 1, 1, 0,
    0, 0, 0, 0, 0, 1, 0, 1,
  ]),
  indices: new Uint16Array([0, 1, 2]),
  label: "tri",
})

// Translation moves positions, leaves normals and uvs alone.
{
  let g = transformGeometry(tri(), { position: [10, 20, 30] })
  expectVec("translate pos", g.vertices.subarray(0, 3), [11, 20, 30])
  expectVec("translate normal", g.vertices.subarray(3, 6), [0, 0, 1])
  expectVec("translate uv", g.vertices.subarray(14, 16), [1, 0])
  if (g.label !== "tri-transformed") fail("label default: " + g.label)
  if (g.indices.length !== 3) fail("indices carried")
}

// 90 degrees about y: +x -> -z, the +z normal -> +x.
{
  let g = transformGeometry(tri(), { rotation: [0, Math.PI / 2, 0] })
  expectVec("rotate pos", g.vertices.subarray(0, 3), [0, 0, -1])
  expectVec("rotate normal", g.vertices.subarray(3, 6), [1, 0, 0])
}

// Quaternion form agrees with the euler form.
{
  let s = Math.sin(0.4), c = Math.cos(0.4)
  let b = transformGeometry(tri(), { quaternion: [0, s, 0, c] })
  let e = transformGeometry(tri(), { rotation: [0, 0.8, 0] })
  expectVec("quat vs euler", b.vertices.subarray(0, 6), e.vertices.subarray(0, 6))
}

// Non-uniform scale: a tilted normal must go through the inverse transpose.
// Normal (1,1,0)/sqrt2 on a surface scaled by (2,1,1): the plane x+y=c
// becomes x/2+y=c, whose normal is (0.5,1,0) normalized, NOT (2,1,0).
{
  let g: Geometry = {
    vertices: new Float32Array([0, 0, 0, Math.SQRT1_2, Math.SQRT1_2, 0, 0, 0]),
    indices: new Uint16Array([0, 0, 0]),
  }
  let t = transformGeometry(g, { scale: [2, 1, 1] })
  let l = Math.hypot(0.5, 1)
  expectVec("non-uniform normal", t.vertices.subarray(3, 6), [0.5 / l, 1 / l, 0])
  let u = transformGeometry(g, { scale: 3 })
  expectVec("uniform scale normal", u.vertices.subarray(3, 6), [Math.SQRT1_2, Math.SQRT1_2, 0])
}

// Colored layout: stride 12, color slots copy through untouched.
{
  let c = withColors(tri(), [1, 0, 0, 1, 0, 1, 0, 1, 0, 0, 1, 1])
  let g = transformGeometry(c, { position: [1, 0, 0] })
  if (g.layout !== "colored") fail("colored layout kept")
  expectVec("colored pos", g.vertices.subarray(12, 15), [1, 1, 0])
  expectVec("colored color", g.vertices.subarray(20, 24), [0, 1, 0, 1])
}

throws("rotation and quaternion", () => transformGeometry(tri(), { rotation: [0, 0, 0], quaternion: [0, 0, 0, 1] }))

// Merge: offsets and counts.
{
  let a = box()
  let b = transformGeometry(box(), { position: [3, 0, 0] })
  let m = mergeGeometries([a, b], "pair")
  let va = a.vertices.length / STANDARD_FLOATS
  if (m.vertices.length !== a.vertices.length + b.vertices.length) fail("merge vertex count")
  if (m.indices.length !== a.indices.length + b.indices.length) fail("merge index count")
  if (m.indices[a.indices.length]! !== b.indices[0]! + va) fail("merge index offset")
  if (!(m.indices instanceof Uint16Array)) fail("merge stays uint16")
  if (m.label !== "pair") fail("merge label")
  expectVec("merge bounds", geometryBounds(m), [-0.5, -0.5, -0.5, 3.5, 0.5, 0.5])
}

// Merge past 64k vertices widens the index array.
{
  let parts: Geometry[] = []
  for (let i = 0; i < 70000 / 4 + 1; i++) parts.push(plane())
  let m = mergeGeometries(parts)
  if (!(m.indices instanceof Uint32Array)) fail("merge widens to uint32")
  let last = parts.length - 1
  let lastPart = parts[last]!
  if (m.indices[m.indices.length - 1]! !== lastPart.indices[lastPart.indices.length - 1]! + last * 4) fail("uint32 offset")
}

throws("merge mixed layouts", () => mergeGeometries([tri(), withColors(tri(), () => [1, 1, 1, 1])]))
throws("merge empty", () => mergeGeometries([]))

// Open layouts: withAttribute appends a channel after the standard prefix,
// stride and slots follow the list, and withColors is the aColor spelling
// (preset name kept, identical bytes).
{
  let t = withAttribute(tri(), { name: "aTangent", format: "float32x3" }, (_i, pos) => [pos[0], pos[1], 9])
  if (layoutStride(t.layout) !== 44) fail("tangent stride: " + layoutStride(t.layout))
  if (layoutKey(t.layout) !== "aPos:float32x3,aNormal:float32x3,aUV:float32x2,aTangent:float32x3") fail("tangent key: " + layoutKey(t.layout))
  expectVec("tangent slot", t.vertices.subarray(11 + 8, 11 + 11), [0, 1, 9])
  expectVec("tangent prefix kept", t.vertices.subarray(11, 11 + 8), [0, 1, 0, 0, 0, 1, 1, 0])
  if (t.label !== "tri-aTangent") fail("tangent label: " + t.label)
  let slot = layoutSlot(t.layout, "aTangent")
  if (slot === null || slot.offset !== 32 || slot.components !== 3) fail("tangent slot lookup")
  if (layoutSlot(t.layout, "aColor") !== null) fail("absent slot is null")

  let two = withAttribute(t, { name: "aColor", format: "float32x4" }, [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12])
  if (layoutStride(two.layout) !== 60) fail("two-channel stride")
  expectVec("second channel", two.vertices.subarray(15 + 11, 15 + 15), [5, 6, 7, 8])
  expectVec("first channel kept", two.vertices.subarray(15 + 8, 15 + 11), [0, 1, 9])
  fillAttribute(two, "aTangent", () => [7, 7, 7], 1, 1)
  expectVec("fillAttribute range", two.vertices.subarray(15 + 8, 15 + 11), [7, 7, 7])
  expectVec("fillAttribute outside range untouched", two.vertices.subarray(8, 11), [1, 0, 9])

  let c = withColors(tri(), [1, 0, 0, 1, 0, 1, 0, 1, 0, 0, 1, 1])
  let viaAttr = withAttribute(tri(), { name: "aColor", format: "float32x4" }, [1, 0, 0, 1, 0, 1, 0, 1, 0, 0, 1, 1])
  if (c.layout !== "colored") fail("withColors keeps preset name")
  if (layoutKey(c.layout) !== layoutKey(viaAttr.layout)) fail("colored key equals explicit list")
  expectVec("colored equals withAttribute", c.vertices, viaAttr.vertices)
  let m = mergeGeometries([c, viaAttr])
  if (m.vertices.length !== 72) fail("preset and explicit layouts merge")
  let t2 = transformGeometry(t, { position: [1, 0, 0] })
  expectVec("transform keeps extra channel", t2.vertices.subarray(8, 11), [1, 0, 9])
  expectVec("transform on wide stride", t2.vertices.subarray(11, 14), [1, 1, 0])

  throws("duplicate attribute", () => withAttribute(t, { name: "aTangent", format: "float32x3" }, () => [0, 0, 0]))
  throws("duplicate prefix name", () => withAttribute(tri(), { name: "aUV", format: "float32x2" }, () => [0, 0]))
  throws("fill size mismatch", () => withAttribute(tri(), { name: "aW", format: "float32" }, [1, 2]))
  throws("callback size mismatch", () => withAttribute(tri(), { name: "aW", format: "float32" }, () => [1, 2]))
  throws("fillAttribute unknown name", () => fillAttribute(t, "aNope", () => [0]))
}

// Prefixless layouts: a geometry may drop the normal and uv and carry
// [aPos, aData] at 4 floats a vertex, whatever its topology; transform
// and the fill callback read the standard channels by slot, so aData is
// never touched and an absent normal/uv arrives as zeros. Only aPos
// first is required, and only the generators demand the prefix.
{
  let cloud: Geometry = {
    vertices: new Float32Array([0, 0, 0, 7, 1, 2, 3, 8]),
    indices: new Uint16Array([0, 1]),
    topology: "points",
    layout: [
      { name: "aPos", format: "float32x3" },
      { name: "aData", format: "float32" },
    ],
    label: "cloud",
  }
  validateGeometry(cloud)
  if (layoutStride(cloud.layout) !== 16) fail("cloud stride: " + layoutStride(cloud.layout))
  expectVec("cloud bounds", geometryBounds(cloud), [0, 0, 0, 1, 2, 3])
  let moved = transformGeometry(cloud, { position: [1, 0, 0], rotation: [0, Math.PI / 2, 0] })
  expectVec("cloud transform moves positions", moved.vertices.subarray(4, 7), [4, 2, -1])
  expectVec("cloud transform keeps aData", [moved.vertices[3]!, moved.vertices[7]!], [7, 8])
  let tagged = withAttribute(cloud, { name: "aTag", format: "float32x2" }, (i, pos, normal, uv) => [pos[2] + normal[0] + uv[1], i])
  if (layoutKey(tagged.layout) !== "aPos:float32x3,aData:float32,aTag:float32x2") fail("cloud withAttribute key: " + layoutKey(tagged.layout))
  expectVec("cloud fill callback sees zero normal/uv", tagged.vertices.subarray(6 + 4, 6 + 6), [3, 1])
  expectVec("cloud withAttribute keeps aData", [tagged.vertices[3]!, tagged.vertices[9]!], [7, 8])
  validateGeometry({ ...cloud, topology: undefined, indices: new Uint16Array([0, 1, 0]) })
  throws("layout without aPos first", () => validateGeometry({ ...cloud, layout: [{ name: "aData", format: "float32" }, { name: "aPos", format: "float32x3" }] }))
  throws("prefixless generator layout", () => plane({ layout: [{ name: "aPos", format: "float32x3" }] }))
}

// Generators emitting a wider layout in one pass: identical bytes to
// generate-then-repack, and the string tail still means label.
{
  let check = (name: string, std: Geometry, wide: Geometry) => {
    let viaColors = withColors(std, () => [0, 0, 0, 0])
    if (layoutKey(wide.layout) !== layoutKey("colored")) fail(name + ": wide layout key")
    if (wide.vertices.length !== viaColors.vertices.length) fail(name + ": wide length")
    expectVec(name + " wide bytes", wide.vertices, viaColors.vertices)
    expectVec(name + " wide indices", wide.indices, std.indices)
    fillColors(wide, (_i, pos) => [pos[0], pos[1], pos[2], 1])
    expectVec(name + " filled color", wide.vertices.subarray(8, 12), [wide.vertices[0]!, wide.vertices[1]!, wide.vertices[2]!, 1])
  }
  check("box", box({ width: 1, height: 2, depth: 3 }), box({ width: 1, height: 2, depth: 3, layout: "colored" }))
  check("sphere", sphere({ radius: 0.7, widthSegments: 6, heightSegments: 4 }), sphere({ radius: 0.7, widthSegments: 6, heightSegments: 4, layout: "colored" }))
  check("cylinder", cylinder({ radiusTop: 0.2, radialSegments: 5 }), cylinder({ radiusTop: 0.2, radialSegments: 5, layout: "colored" }))
  check("torus", torus({ radius: 1, tube: 0.3, radialSegments: 4, tubularSegments: 6 }), torus({ radius: 1, tube: 0.3, radialSegments: 4, tubularSegments: 6, layout: "colored" }))
  check("torusKnot", torusKnot({ tube: 0.3, tubularSegments: 8, radialSegments: 4 }), torusKnot({ tube: 0.3, tubularSegments: 8, radialSegments: 4, layout: "colored" }))
  check("icosahedron", icosahedron({ detail: 1 }), icosahedron({ detail: 1, layout: "colored" }))
  let custom = sphere({ radius: 1, widthSegments: 4, heightSegments: 3, layout: [{ name: "aPos", format: "float32x3" }, { name: "aNormal", format: "float32x3" }, { name: "aUV", format: "float32x2" }, { name: "aW", format: "float32" }], label: "w" })
  if (layoutStride(custom.layout) !== 36 || custom.label !== "w") fail("custom generator layout")
  expectVec("custom generator prefix", custom.vertices.subarray(9, 17), sphere({ radius: 1, widthSegments: 4, heightSegments: 3 }).vertices.subarray(8, 16))
  if (box({ label: "named" }).label !== "named") fail("label option")
  if (box().layout !== undefined) fail("default layout stays absent")
  throws("generator bad layout", () => box({ layout: [{ name: "aColor", format: "float32x4" }] }))
  throws("torus bad layout", () => torus({ layout: [{ name: "aColor", format: "float32x4" }] }))
  throws("packGeometry ragged", () => packGeometry([1, 2, 3], [0]))
}

// The polyhedron family: counts per detail, every corner on the
// circumsphere, CCW winding seen from outside, face normals at detail 0
// and radial normals above, UVs in range with no triangle left straddling
// the seam and the y-axis corners taking their triangle's azimuth, the
// closed solids' edge counts through the position weld, and the generic
// builder over an open face list.
{
  // After the seam patch a u lifted by a full turn stays under 1 + the
  // lift threshold (0.2).
  let U_MAX = 1.2
  let inspect = (name: string, g: Geometry, faces: number, radius: number, detail: number): void => {
    let tris = faces * (detail + 1) * (detail + 1)
    if (g.vertices.length !== tris * 3 * STANDARD_FLOATS) fail(name + ": vertex count " + g.vertices.length / STANDARD_FLOATS)
    if (g.indices.length !== tris * 3) fail(name + ": index count " + g.indices.length)
    for (let i = 0; i < g.indices.length; i++) {
      if (g.indices[i] !== i) {
        fail(name + ": indices are not 0..n-1")
        break
      }
    }
    let v = g.vertices
    let at = (i: number, k: number): Vec3 => [v[i * STANDARD_FLOATS + k]!, v[i * STANDARD_FLOATS + k + 1]!, v[i * STANDARD_FLOATS + k + 2]!]
    for (let t = 0; t < tris; t++) {
      let p = [at(t * 3, 0), at(t * 3 + 1, 0), at(t * 3 + 2, 0)]
      let n = normalize(cross(sub(p[1]!, p[0]!), sub(p[2]!, p[0]!)))
      let cx = (p[0]![0] + p[1]![0] + p[2]![0]) / 3
      let cy = (p[0]![1] + p[1]![1] + p[2]![1]) / 3
      let cz = (p[0]![2] + p[1]![2] + p[2]![2]) / 3
      if (n[0] * cx + n[1] * cy + n[2] * cz <= 0) {
        fail(name + ": triangle " + t + " winds inward")
        return
      }
      let us: number[] = []
      for (let k = 0; k < 3; k++) {
        let q = p[k]!
        if (!near(Math.hypot(q[0], q[1], q[2]), radius)) {
          fail(name + ": corner off the circumsphere: " + Math.hypot(q[0], q[1], q[2]))
          return
        }
        let want = detail === 0 ? n : normalize(q)
        let got = at(t * 3 + k, 3)
        if (!near(got[0], want[0]) || !near(got[1], want[1]) || !near(got[2], want[2])) {
          fail(name + ": normal of triangle " + t + " corner " + k)
          return
        }
        let u = v[(t * 3 + k) * STANDARD_FLOATS + 6]!
        let vv = v[(t * 3 + k) * STANDARD_FLOATS + 7]!
        if (u < 0 || u > U_MAX || vv < 0 || vv > 1) {
          fail(name + ": uv out of range " + u + "," + vv)
          return
        }
        us.push(u)
      }
      if (Math.max(...us) > 0.9 && Math.min(...us) < 0.1) {
        fail(name + ": triangle " + t + " straddles the seam: " + us.join(","))
        return
      }
      for (let k = 0; k < 3; k++) {
        let q = p[k]!
        if (q[0] !== 0 || q[2] !== 0) continue
        let others = us.filter((_u, i) => i !== k)
        if (us[k]! < Math.min(...others) - 1e-5 || us[k]! > Math.max(...others) + 1e-5) {
          fail(name + ": y-axis corner u " + us[k] + " outside its triangle's " + others.join(","))
          return
        }
      }
    }
  }
  // Triangles, feature edges and wireframe edges: the dodecahedron's fan
  // diagonals are coplanar, so edgesGeometry drops them (the twelve
  // pentagons) while wireframeGeometry lists the triangulation.
  let solids: [string, (o?: PolyhedronOptions) => Geometry, number, number, number][] = [
    ["tetrahedron", tetrahedron, 4, 6, 6],
    ["octahedron", octahedron, 8, 12, 12],
    ["icosahedron", icosahedron, 20, 30, 30],
    ["dodecahedron", dodecahedron, 36, 30, 54],
  ]
  for (let [name, build, faces, edges, wires] of solids) {
    inspect(name, build(), faces, 0.5, 0)
    inspect(name + " r2 d1", build({ radius: 2, detail: 1 }), faces, 2, 1)
    inspect(name + " d3", build({ detail: 3 }), faces, 0.5, 3)
    if (edgesGeometry(build()).indices.length !== edges * 2) fail(name + ": edges " + edgesGeometry(build()).indices.length / 2 + " want " + edges)
    if (wireframeGeometry(build()).indices.length !== wires * 2) fail(name + ": wireframe edges " + wireframeGeometry(build()).indices.length / 2 + " want " + wires)
    validateGeometry(build({ detail: 2 }))
  }
  // Detail 1 on the icosahedron: every original edge halves and each face
  // gains three interior edges, all of them creases once projected.
  if (edgesGeometry(icosahedron({ detail: 1 })).indices.length !== 120 * 2) fail("icosphere d1 edges " + edgesGeometry(icosahedron({ detail: 1 })).indices.length / 2)
  if (icosahedron({ label: "ico" }).label !== "ico") fail("polyhedron label")
  // The generic builder over one open triangle: F * (detail + 1)^2.
  let open = polyhedron([1, 0, 0, 0, 1, 0, 0, 0, 1], [0, 1, 2], { detail: 2 })
  if (open.vertices.length !== 9 * 3 * STANDARD_FLOATS) fail("open polyhedron count " + open.vertices.length / STANDARD_FLOATS)
  expectVec("open polyhedron corner", open.vertices.subarray(2 * STANDARD_FLOATS, 2 * STANDARD_FLOATS + 3), [0.5, 0, 0])
  throws("polyhedron fractional detail", () => icosahedron({ detail: 1.5 }))
  throws("polyhedron negative detail", () => icosahedron({ detail: -1 }))
  throws("polyhedron ragged vertices", () => polyhedron([1, 0], [0, 1, 2]))
  throws("polyhedron ragged indices", () => polyhedron([1, 0, 0, 0, 1, 0, 0, 0, 1], [0, 1]))
}

// validateGeometry: the add()-time structural check.
{
  validateGeometry(box())
  validateGeometry(withColors(box(), () => [0, 0, 0, 0]))
  throws("validate ragged colored", () => validateGeometry({ vertices: new Float32Array(16), indices: new Uint16Array([0, 1]), layout: "colored", label: "ragged" }))
  throws("validate bad layout", () => validateGeometry({ vertices: new Float32Array(8), indices: new Uint16Array([0]), layout: [{ name: "aColor", format: "float32x4" }] }))
  throws("validate no indices", () => validateGeometry({ vertices: new Float32Array(24), indices: new Uint16Array(0) }))
}

// Public ray helper: hit from outside, inside, miss.
{
  if (!near(rayBoxDistance(-2, 0, 0, 1, 0, 0, -1, -1, -1, 1, 1, 1), 1)) fail("ray enters at 1")
  if (rayBoxDistance(0, 0, 0, 1, 0, 0, -1, -1, -1, 1, 1, 1) !== 0) fail("ray inside is 0")
  if (rayBoxDistance(-2, 5, 0, 1, 0, 0, -1, -1, -1, 1, 1, 1) !== -1) fail("ray misses")
}

// Topology: the count rule per topology at validate, the wireframe and
// edge builders over the source's own vertices (welded by position, so a
// split vertex never draws an edge twice), and what carries it through.
{
  let b = box()
  let wire = wireframeGeometry(b)
  if (wire.topology !== "lines") fail(`wireframe topology: ${String(wire.topology)}`)
  if (wire.vertices !== b.vertices || wire.layout !== b.layout) fail("wireframe: vertices and layout are not shared by reference")
  if (wire.indices.length !== 36) fail(`wireframe box: ${wire.indices.length / 2} lines, expected 18 (12 edges + 6 diagonals)`)
  validateGeometry(wire)
  // No welded edge twice: the box's 24 split vertices name each cube
  // edge from two faces, and only one may survive.
  let posKey = (i: number): string => Array.from(b.vertices.subarray(i * STANDARD_FLOATS, i * STANDARD_FLOATS + 3)).join()
  let seen = new Set<string>()
  for (let i = 0; i < wire.indices.length; i += 2) {
    let key = [posKey(wire.indices[i]!), posKey(wire.indices[i + 1]!)].sort().join("|")
    if (seen.has(key)) fail(`wireframe: edge ${key} listed twice`)
    seen.add(key)
  }
  let edges = edgesGeometry(b)
  if (edges.topology !== "lines" || edges.indices.length !== 24) fail(`edges box: ${edges.indices.length / 2} lines, expected 12`)
  if (edgesGeometry(plane()).indices.length !== 8) fail("edges plane: expected the 4 border lines")
  // A generated round shape is faceted: past its facet angle only the
  // borders and creases stay.
  if (edgesGeometry(cylinder(), 16).indices.length !== 96) fail(`edges cylinder at 16 degrees: ${edgesGeometry(cylinder(), 16).indices.length / 2} lines, expected the two 24-segment rims`)
  if (edgesGeometry(cylinder()).indices.length <= 96) fail("edges cylinder at the default threshold: expected the side seams too")
  throws("wireframe of lines", () => wireframeGeometry(wire))
  throws("edges of lines", () => edgesGeometry(wire))
  // Carry-through and merge.
  if (transformGeometry(wire, { position: [1, 0, 0] }).topology !== "lines") fail("transformGeometry drops topology")
  if (withColors(wire, () => [1, 1, 1, 1]).topology !== "lines") fail("withColors drops topology")
  let merged = mergeGeometries([wire, wireframeGeometry(cylinder())])
  if (merged.topology !== "lines" || merged.indices.length !== wire.indices.length + wireframeGeometry(cylinder()).indices.length) fail("mergeGeometries of lines")
  throws("merge mixed topologies", () => mergeGeometries([b, wire]))
  throws("merge strips", () => mergeGeometries([{ vertices: b.vertices, indices: new Uint16Array([0, 1, 2, 3]), topology: "triangle-strip" }]))
  // validateGeometry's count rule per topology.
  let v = b.vertices
  validateGeometry({ vertices: v, indices: new Uint16Array([0, 1, 2, 3]), topology: "triangle-strip" })
  validateGeometry({ vertices: v, indices: new Uint16Array([0, 1, 2]), topology: "line-strip" })
  validateGeometry({ vertices: v, indices: new Uint16Array([0]), topology: "points" })
  validateGeometry({ vertices: v, indices: new Uint16Array(0), topology: "lines" })
  validateGeometry({ vertices: v, indices: new Uint16Array(0), topology: "points" })
  if (edgesGeometry(sphere(), 16).indices.length !== 0) fail("edges sphere at 16 degrees: expected no lines")
  throws("validate triangles not a multiple of 3", () => validateGeometry({ vertices: v, indices: new Uint16Array([0, 1, 2, 3]) }))
  throws("validate lines not a multiple of 2", () => validateGeometry({ vertices: v, indices: new Uint16Array([0, 1, 2]), topology: "lines" }))
  throws("validate short line strip", () => validateGeometry({ vertices: v, indices: new Uint16Array([0]), topology: "line-strip" }))
  throws("validate short triangle strip", () => validateGeometry({ vertices: v, indices: new Uint16Array([0, 1]), topology: "triangle-strip" }))
  throws("validate unknown topology", () => validateGeometry({ vertices: v, indices: new Uint16Array([0]), topology: "fans" as never }))
}

// The debug helpers: lines topology, counts, bounds, and the color channel
// where there is one (pure primaries survive the sRGB decode exactly).
{
  let count = (g: Geometry): number => vertexCount(g.vertices, g.layout, "rig")
  let slot = layoutSlot("colored", "aColor")!
  let colorAt = (g: Geometry, i: number): number[] => read(g, "aColor", i)
  let posAt = (g: Geometry, i: number): number[] => read(g, "aPos", i)

  let grid = gridHelper({ size: 2, divisions: 2, color: [1, 0, 0], centerColor: [0, 0, 1] })
  if (grid.topology !== "lines" || grid.layout !== "colored") fail("gridHelper shape")
  if (count(grid) !== 12 || grid.indices.length !== 12) fail("gridHelper counts")
  validateGeometry(grid)
  expectVec("gridHelper bounds", geometryBounds(grid), [-1, 0, -1, 1, 0, 1])
  expectVec("gridHelper first line start", posAt(grid, 0), [-1, 0, -1])
  expectVec("gridHelper first line end", posAt(grid, 1), [1, 0, -1])
  expectVec("gridHelper edge color", colorAt(grid, 0), [1, 0, 0, 1])
  expectVec("gridHelper center color x", colorAt(grid, 4), [0, 0, 1, 1])
  expectVec("gridHelper center color z", colorAt(grid, 7), [0, 0, 1, 1])
  let dflt = gridHelper()
  if (count(dflt) !== 44 || dflt.indices.length !== 44) fail("gridHelper default counts")
  expectVec("gridHelper default bounds", geometryBounds(dflt), [-5, 0, -5, 5, 0, 5])
  let odd = gridHelper({ divisions: 3, color: [1, 0, 0], centerColor: [0, 0, 1] })
  for (let i = 0; i < 16; i++) if (colorAt(odd, i)[2] !== 0) fail("gridHelper odd divisions has a center line at vertex " + i)
  throws("gridHelper standard layout", () => gridHelper({ layout: "standard" }))
  throws("gridHelper zero divisions", () => gridHelper({ divisions: 0 }))

  let axes = axesHelper({ size: 2 })
  if (axes.topology !== "lines" || axes.layout !== "colored") fail("axesHelper shape")
  if (count(axes) !== 6 || axes.indices.length !== 6) fail("axesHelper counts")
  validateGeometry(axes)
  expectVec("axesHelper x tip", posAt(axes, 1), [2, 0, 0])
  expectVec("axesHelper y tip", posAt(axes, 3), [0, 2, 0])
  expectVec("axesHelper z tip", posAt(axes, 5), [0, 0, 2])
  expectVec("axesHelper x red", colorAt(axes, 1), [1, 0, 0, 1])
  expectVec("axesHelper y green", colorAt(axes, 3), [0, 1, 0, 1])
  expectVec("axesHelper z blue", colorAt(axes, 5), [0, 0, 1, 1])
  // A wider layout that still carries aColor takes the colors at its slot.
  let wide = axesHelper({ layout: [...VERTEX_LAYOUTS.colored, { name: "aW", format: "float32" }] })
  expectVec("axesHelper wide layout color", read(wide, "aColor", 1), [1, 0, 0, 1])
  throws("axesHelper skinned layout", () => axesHelper({ layout: "skinned" }))

  let bounds = [-1, -2, -3, 4, 5, 6]
  let b3 = box3Helper(bounds, { label: "b3" })
  if (b3.topology !== "lines" || b3.layout !== undefined || b3.label !== "b3") fail("box3Helper shape")
  if (b3.vertices.length !== 8 * STANDARD_FLOATS || b3.indices.length !== 24) fail("box3Helper counts")
  validateGeometry(b3)
  expectVec("box3Helper bounds", geometryBounds(b3), bounds)
  for (let e = 0; e < 24; e += 2) {
    let a = b3.indices[e]! * STANDARD_FLOATS, b = b3.indices[e + 1]! * STANDARD_FLOATS
    let differ = 0
    for (let k = 0; k < 3; k++) if (b3.vertices[a + k] !== b3.vertices[b + k]) differ++
    if (differ !== 1) fail("box3Helper edge " + e / 2 + " is not axis-aligned")
  }
  throws("box3Helper short bounds", () => box3Helper([1, 2, 3]))

  let pl = planeHelper({ size: 4 })
  if (pl.topology !== "lines" || pl.layout !== undefined) fail("planeHelper shape")
  if (pl.vertices.length !== 6 * STANDARD_FLOATS || pl.indices.length !== 14) fail("planeHelper counts")
  validateGeometry(pl)
  expectVec("planeHelper bounds", geometryBounds(pl), [-2, -2, 0, 2, 2, 1])
  expectVec("planeHelper normal tick", pl.vertices.subarray(5 * STANDARD_FLOATS, 5 * STANDARD_FLOATS + 3), [0, 0, 1])

  // Head a fifth of the length (0.4) and a fifth as wide (0.08): the base
  // sits at y 1.6 with a half-width of 0.04.
  let arrow = arrowHelper({ length: 2 })
  if (arrow.topology !== "lines" || arrow.layout !== undefined) fail("arrowHelper shape")
  if (arrow.vertices.length !== 7 * STANDARD_FLOATS || arrow.indices.length !== 18) fail("arrowHelper counts")
  validateGeometry(arrow)
  expectVec("arrowHelper bounds", geometryBounds(arrow), [-0.04, 0, -0.04, 0.04, 2, 0.04])
  expectVec("arrowHelper tip", arrow.vertices.subarray(2 * STANDARD_FLOATS, 2 * STANDARD_FLOATS + 3), [0, 2, 0])
  expectVec("arrowHelper base corner", arrow.vertices.subarray(3 * STANDARD_FLOATS, 3 * STANDARD_FLOATS + 3), [0.04, 1.6, 0])
  let custom = arrowHelper({ length: 1, headLength: 0.5, headWidth: 1 })
  expectVec("arrowHelper custom head", geometryBounds(custom), [-0.5, 0, -0.5, 0.5, 1, 0.5])
}

// Packed formats: every codec round-trips through the bytes the engine
// reads, the packed channel survives transform and merge, and the
// float32 view rule holds (a packed layout is a Uint8Array, an all-float
// one a Float32Array).
{
  // The raw bytes each format encodes a known value to, little-endian.
  let cases: { format: VertexFormat; value: number[]; bytes: number[]; back?: number[] }[] = [
    { format: "float32", value: [1.5], bytes: [0, 0, 0xc0, 0x3f] },
    { format: "float16x2", value: [0.5, -2], bytes: [0x00, 0x38, 0x00, 0xc0] },
    { format: "unorm8x4", value: [1, 0.5, 0, 0.2], bytes: [255, 128, 0, 51], back: [1, 128 / 255, 0, 51 / 255] },
    { format: "snorm8x4", value: [1, -1, 0.5, -2], bytes: [127, 0x81, 64, 0x81], back: [1, -1, 64 / 127, -1] },
    { format: "unorm16x2", value: [1, 0.25], bytes: [0xff, 0xff, 0x00, 0x40], back: [1, 0x4000 / 65535] },
    { format: "snorm16x2", value: [-1, 0.5], bytes: [0x01, 0x80, 0x00, 0x40], back: [-1, 0x4000 / 32767] },
    { format: "uint8x4", value: [3, 200, 255, 0], bytes: [3, 200, 255, 0] },
    { format: "uint16x2", value: [7, 65535], bytes: [7, 0, 0xff, 0xff] },
  ]
  for (let c of cases) {
    let g = withAttribute(tri(), { name: "aX", format: c.format }, () => c.value)
    let stride = 32 + VERTEX_FORMATS[c.format].bytes
    if (layoutStride(g.layout) !== stride) fail(c.format + " stride: " + layoutStride(g.layout))
    if (!(g.vertices instanceof (c.format.startsWith("float32") ? Float32Array : Uint8Array))) fail(c.format + " view type")
    expectVec(c.format + " bytes", Array.from(vertexBytes(g.vertices).subarray(32, stride)), c.bytes)
    expectVec(c.format + " decoded", read(g, "aX", 1), c.back ?? c.value)
    expectVec(c.format + " prefix kept", read(g, "aPos", 1), [0, 1, 0])
    validateGeometry(g)
  }
  if (!(withAttribute(tri(), { name: "aW", format: "float32x2" }, () => [1, 2]).vertices instanceof Float32Array)) fail("float layout view is not floats")
  throws("unknown format", () => withAttribute(tri(), { name: "aW", format: "vec3" as VertexFormat }, () => [0, 0, 0]))

  // A quantized normal beside a byte color: transform rotates the packed
  // normal in place (re-encoded), the color rides through, and the merge
  // concatenates bytes.
  let packed = withAttribute(withAttribute(tri(), { name: "aColor", format: "unorm8x4" }, (_i, pos) => [pos[0], 1, 0, 1]), { name: "aN2", format: "snorm16x4" }, (_i, _p, n) => [n[0], n[1], n[2], 0])
  let turned = transformGeometry(packed, { rotation: [0, Math.PI / 2, 0] })
  expectVec("packed transform normal", read(turned, "aNormal", 0), [1, 0, 0])
  expectVec("packed transform keeps color", read(turned, "aColor", 0), [1, 1, 0, 1])
  expectVec("packed transform keeps snorm", read(turned, "aN2", 2), [0, 0, 1, 0])
  let merged = mergeGeometries([packed, turned])
  if (vertexCount(merged.vertices, merged.layout, "rig") !== 6 || !(merged.vertices instanceof Uint8Array)) fail("packed merge shape")
  expectVec("packed merge second part", read(merged, "aPos", 3), read(turned, "aPos", 0))
  expectVec("packed merge color", read(merged, "aColor", 3), [1, 1, 0, 1])
  expectVec("packed bounds", geometryBounds(merged), [0, 0, -1, 1, 1, 0])
  // The two triangles share two welded positions, so one edge is shared:
  // five edges, not six.
  let wire = wireframeGeometry(merged)
  if (wire.indices.length !== 10) fail("packed wireframe edges: " + wire.indices.length)
  throws("packed byte count", () => validateGeometry({ ...packed, vertices: vertexBytes(packed.vertices).subarray(0, 50) }))
  throws("unaligned view", () => validateGeometry({ ...packed, vertices: new Uint8Array(new ArrayBuffer(packed.vertices.byteLength + 1), 1) }))
  throws("packed generator layout", () => box({ layout: [...VERTEX_LAYOUTS.standard, { name: "aColor", format: "unorm8x4" }] }))
}

// Streams: a channel in a buffer of its own, found by name across streams,
// carried through transform, merge and the edge builders, and the stream
// rules (equal counts, unique names, valid indices).
{
  let base = tri()
  let waved = withAttribute(base, { name: "aWave", format: "float32" }, (i) => [i * 0.5], { stream: 1 })
  if (waved.streams?.length !== 1) fail("stream count: " + waved.streams?.length)
  if (geometryKey(waved) !== "aPos:float32x3,aNormal:float32x3,aUV:float32x2|aWave:float32") fail("stream key: " + geometryKey(waved))
  if (waved.vertices !== base.vertices) fail("stream 0 shared when the channel opens a stream")
  expectVec("stream read by name", [read(waved, "aWave", 0)[0]!, read(waved, "aWave", 2)[0]!], [0, 1])
  let slot = geometrySlot(waved, "aWave")
  if (slot === null || slot.stream !== 1 || slot.offset !== 0) fail("stream slot lookup")
  if (geometryVertexCount(waved, "rig") !== 3) fail("stream vertex count")
  validateGeometry(waved)
  // Append to the extra stream: its layout grows, stream 0 stays shared.
  let tagged = withAttribute(waved, { name: "aTag", format: "unorm8x4" }, (i, pos) => [pos[0], 0, 0, 1], { stream: 1 })
  if (geometryKey(tagged) !== "aPos:float32x3,aNormal:float32x3,aUV:float32x2|aWave:float32,aTag:unorm8x4") fail("stream append key: " + geometryKey(tagged))
  if (tagged.vertices !== base.vertices || layoutStride(tagged.streams![0]!.layout) !== 8) fail("stream append shape")
  expectVec("stream append keeps aWave", read(tagged, "aWave", 2), [1])
  expectVec("stream append fill sees stream-0 pos", read(tagged, "aTag", 0), [1, 0, 0, 1])
  // fillAttribute writes into the stream and returns its buffer.
  let written = fillAttribute(tagged, "aWave", [7, 8, 9])
  if (written !== tagged.streams![0]!.vertices) fail("fillAttribute returns the stream's vertices")
  expectVec("fillAttribute into a stream", read(tagged, "aWave", 1), [8])
  // Transform rewrites stream 0 and shares the rest; merge concatenates
  // every stream; the edge builders share every stream.
  let moved = transformGeometry(tagged, { position: [0, 0, 5] })
  if (moved.streams !== tagged.streams) fail("transform shares extra streams")
  expectVec("transform moves stream 0", read(moved, "aPos", 0), [1, 0, 5])
  let merged = mergeGeometries([tagged, moved])
  if (geometryKey(merged) !== geometryKey(tagged) || geometryVertexCount(merged, "rig") !== 6) fail("stream merge shape")
  expectVec("stream merge second part", read(merged, "aWave", 4), [8])
  expectVec("stream merge second pos", read(merged, "aPos", 3), [1, 0, 5])
  let wire = wireframeGeometry(merged)
  if (wire.streams !== merged.streams) fail("wireframe shares streams")
  throws("stream count mismatch", () => validateGeometry({ ...tagged, streams: [{ layout: tagged.streams![0]!.layout, vertices: new Float32Array(4) }] }))
  throws("name in two streams", () => validateGeometry({ ...tagged, streams: [{ layout: [{ name: "aPos", format: "float32x3" }], vertices: new Float32Array(9) }] }))
  throws("stream index past the next", () => withAttribute(waved, { name: "aX", format: "float32" }, () => [0], { stream: 3 }))
  throws("duplicate across streams", () => withAttribute(waved, { name: "aWave", format: "float32" }, () => [0]))
  throws("mixed stream layouts merge", () => mergeGeometries([waved, base]))
  if (geometryStreams(base).length !== 1) fail("a plain geometry is one stream")
}

if (failures > 0) throw new Error(failures + " geometry check(s) failed")
console.log("PASS: geometry ops")
