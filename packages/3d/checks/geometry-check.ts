// Check rig for the geometry-as-data ops (src/geometry.ts): transformGeometry
// against hand-computed points and normals (non-uniform scale included),
// mergeGeometries offsets, uint32 widening and the mixed-layout rejection,
// and the exported bounds/ray helpers. Pure-module inputs only, so it runs
// headless on flux, bundled from the repo root:
//
//   bunx srt bundle -f --stdout packages/3d/checks/geometry-check.ts | target/release/flux -
//
// A failure prints FAIL lines and throws at the end, so the run exits nonzero.

import { box, cylinder, edgesGeometry, validateGeometry, fillAttribute, fillColors, packGeometry, sphere, torus, torusKnot, geometryBounds, layoutKey, layoutSlot, layoutStride, mergeGeometries, plane, transformGeometry, wireframeGeometry, withAttribute, withColors, STANDARD_FLOATS } from "../src/geometry.ts"
import { rayBoxDistance } from "../src/math.ts"
import type { Geometry } from "../src/geometry.ts"

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
  let t = withAttribute(tri(), { name: "aTangent", format: "vec3" }, (_i, pos) => [pos[0], pos[1], 9])
  if (layoutStride(t.layout) !== 11) fail("tangent stride: " + layoutStride(t.layout))
  if (layoutKey(t.layout) !== "aPos:vec3,aNormal:vec3,aUV:vec2,aTangent:vec3") fail("tangent key: " + layoutKey(t.layout))
  expectVec("tangent slot", t.vertices.subarray(11 + 8, 11 + 11), [0, 1, 9])
  expectVec("tangent prefix kept", t.vertices.subarray(11, 11 + 8), [0, 1, 0, 0, 0, 1, 1, 0])
  if (t.label !== "tri-aTangent") fail("tangent label: " + t.label)
  let slot = layoutSlot(t.layout, "aTangent")
  if (slot === null || slot.offset !== 8 || slot.size !== 3) fail("tangent slot lookup")
  if (layoutSlot(t.layout, "aColor") !== null) fail("absent slot is null")

  let two = withAttribute(t, { name: "aColor", format: "vec4" }, [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12])
  if (layoutStride(two.layout) !== 15) fail("two-channel stride")
  expectVec("second channel", two.vertices.subarray(15 + 11, 15 + 15), [5, 6, 7, 8])
  expectVec("first channel kept", two.vertices.subarray(15 + 8, 15 + 11), [0, 1, 9])
  fillAttribute(two, "aTangent", () => [7, 7, 7], 1, 1)
  expectVec("fillAttribute range", two.vertices.subarray(15 + 8, 15 + 11), [7, 7, 7])
  expectVec("fillAttribute outside range untouched", two.vertices.subarray(8, 11), [1, 0, 9])

  let c = withColors(tri(), [1, 0, 0, 1, 0, 1, 0, 1, 0, 0, 1, 1])
  let viaAttr = withAttribute(tri(), { name: "aColor", format: "vec4" }, [1, 0, 0, 1, 0, 1, 0, 1, 0, 0, 1, 1])
  if (c.layout !== "colored") fail("withColors keeps preset name")
  if (layoutKey(c.layout) !== layoutKey(viaAttr.layout)) fail("colored key equals explicit list")
  expectVec("colored equals withAttribute", c.vertices, viaAttr.vertices)
  let m = mergeGeometries([c, viaAttr])
  if (m.vertices.length !== 72) fail("preset and explicit layouts merge")
  let t2 = transformGeometry(t, { position: [1, 0, 0] })
  expectVec("transform keeps extra channel", t2.vertices.subarray(8, 11), [1, 0, 9])
  expectVec("transform on wide stride", t2.vertices.subarray(11, 14), [1, 1, 0])

  throws("duplicate attribute", () => withAttribute(t, { name: "aTangent", format: "vec3" }, () => [0, 0, 0]))
  throws("duplicate prefix name", () => withAttribute(tri(), { name: "aUV", format: "vec2" }, () => [0, 0]))
  throws("fill size mismatch", () => withAttribute(tri(), { name: "aW", format: "f32" }, [1, 2]))
  throws("callback size mismatch", () => withAttribute(tri(), { name: "aW", format: "f32" }, () => [1, 2]))
  throws("fillAttribute unknown name", () => fillAttribute(t, "aNope", () => [0]))
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
  let custom = sphere({ radius: 1, widthSegments: 4, heightSegments: 3, layout: [{ name: "aPos", format: "vec3" }, { name: "aNormal", format: "vec3" }, { name: "aUV", format: "vec2" }, { name: "aW", format: "f32" }], label: "w" })
  if (layoutStride(custom.layout) !== 9 || custom.label !== "w") fail("custom generator layout")
  expectVec("custom generator prefix", custom.vertices.subarray(9, 17), sphere({ radius: 1, widthSegments: 4, heightSegments: 3 }).vertices.subarray(8, 16))
  if (box({ label: "named" }).label !== "named") fail("label option")
  if (box().layout !== undefined) fail("default layout stays absent")
  throws("generator bad layout", () => box({ layout: [{ name: "aColor", format: "vec4" }] }))
  throws("torus bad layout", () => torus({ layout: [{ name: "aColor", format: "vec4" }] }))
  throws("packGeometry ragged", () => packGeometry([1, 2, 3], [0]))
}

// validateGeometry: the add()-time structural check.
{
  validateGeometry(box())
  validateGeometry(withColors(box(), () => [0, 0, 0, 0]))
  throws("validate ragged colored", () => validateGeometry({ vertices: new Float32Array(16), indices: new Uint16Array([0, 1]), layout: "colored", label: "ragged" }))
  throws("validate bad layout", () => validateGeometry({ vertices: new Float32Array(8), indices: new Uint16Array([0]), layout: [{ name: "aColor", format: "vec4" }] }))
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

if (failures > 0) throw new Error(failures + " geometry check(s) failed")
console.log("PASS: geometry ops")
