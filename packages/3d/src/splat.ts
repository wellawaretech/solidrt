// The splat RUNTIME side: the stock splat material and createSplatMesh
// over a baked SplatData (parse and bake live in ./splat-data.ts, the
// runtime-free `@solidrt/3d/splat` entry; `sol tool 3d/splat` writes the
// .sol3s this loads). A splat cloud here is an ordinary record mesh in the
// INDEXED form - the records live in an rgba32ui data texture the vertex
// stage fetches by id, SPLAT_GROUP splats share one drawn instance (a
// merged-quad geometry; the instancing frontend was the measured wall),
// and the instance buffer is the id stream the core sorts back to front,
// the scene feeding the view direction: zero per-frame JS, a re-sort
// uploads 4 bytes per splat, a parked camera uploads nothing. Depth-tested
// against opaque geometry, placed by the transparent entry sort, picked by
// its bounds, transformed by its node. The spherical-harmonic bands a bake
// carries (`--sh`) ride a second data texture, evaluated per corner from
// the view direction.

import { file } from "flux:fs"
import { createTexture, limits } from "@solidrt/core/gpu"
import type { TextureBindings, TextureId, VertexAttribute } from "@solidrt/core/gpu"
import { mergeGeometries, plane } from "./geometry.ts"
import type { Geometry } from "./geometry.ts"
import { shaderMaterialClass } from "./material.ts"
import type { Material, ShaderMaterialClass } from "./material.ts"
import { createRecordMesh } from "./mesh.ts"
import type { RecordMesh } from "./mesh.ts"
import { decodeSplat, SPLAT_ATTRIBUTES, SPLAT_RECORD_TEXELS, SPLAT_SH_TEXELS, SPLAT_TEXEL_BYTES } from "./splat-data.ts"
import type { SplatData } from "./splat-data.ts"
import { SRGB } from "./glsl.ts"

// core/gpu's glsl tag, aliased locally like ./glsl.ts does.
let glsl = String.raw

/** Splats per drawn instance: the stock geometry is this many quads
 * merged, and an id-stream record is this many u32 ids. Measured on the
 * Pixel 7 at 1M splats: 4, 8 and 16 cost the same and all three collapse
 * the instancing frontend the one-quad form paid, so the fewest instances
 * it is (okf/done/gaussian-splats.md, stage D). */
export const SPLAT_GROUP = 16

/** The id stream's record: SPLAT_GROUP u32 splat ids as uvec4 attributes
 * (iIds0..), what the stock material's one instance buffer declares and
 * the core fills with the sorted record indices. */
export const SPLAT_ID_ATTRIBUTES: VertexAttribute[] = Array.from({ length: SPLAT_GROUP / 4 }, (_, j) => ({ name: `iIds${j}`, format: "uint32x4" }))

// The data textures' row width in texels, capped by the device: a texel
// index addresses (t & (width - 1), t >> log2 width). 4096 keeps a 1M
// cloud's records at 502 rows and its SH3 block at 1504.
const SPLAT_TEXTURE_WIDTH_MAX = 4096
// The id past the sorted population (the core's INDEX_NONE): culled
// before any fetch.
const SPLAT_NONE = 0xffffffff

// The data texture width this device gets, as a power of two with its
// log2 (compiled into the vertex source as constants).
let textureWidth = (): { width: number; log2: number } => {
  let log2 = Math.floor(Math.log2(Math.min(SPLAT_TEXTURE_WIDTH_MAX, limits.maxTextureSize)))
  return { width: 1 << log2, log2 }
}

const SPLAT_CONSTANTS = glsl`
  // Quad half-extent in standard deviations: the gaussian is cut at
  // exp(-0.5 * 8) = e^-4, where the reference viewers cut it.
  const float SPLAT_EXTENT = 2.8284271;
`

// The 3DGS spherical-harmonic basis constants, bands 1..3.
const SH_GLSL = glsl`
  const float SH_C1 = 0.4886025119029199;
  const float SH_C2_0 = 1.0925484305920792;
  const float SH_C2_1 = -1.0925484305920792;
  const float SH_C2_2 = 0.31539156525252005;
  const float SH_C2_3 = -1.0925484305920792;
  const float SH_C2_4 = 0.5462742152960396;
  const float SH_C3_0 = -0.5900435899266435;
  const float SH_C3_1 = 2.890611442640554;
  const float SH_C3_2 = -0.4570457994644658;
  const float SH_C3_3 = 0.3731763325901154;
  const float SH_C3_4 = -0.4570457994644658;
  const float SH_C3_5 = 1.445305721320277;
  const float SH_C3_6 = -0.5900435899266435;
`

// The SH evaluation for `degree`: the view direction in the model frame
// (uCamPos through the transposed model rotation - exact under rotation
// and uniform scale), the coefficients unpacked from the fetched texels
// at constant indices (coefficient-major, rgb interleaved, halves packed
// pairwise), the basis polynomials per band added to the base color.
let shBlock = (degree: number): string => {
  let texels = SPLAT_SH_TEXELS[degree]!
  let fetch = (j: number) => `    uvec4 t${j} = texelFetch(uSplatSh, splatTexel(s + ${j}u), 0);`
  let half = (i: number) => `unpackHalf2x16(t${i >> 3}[${(i >> 1) & 3}]).${i & 1 ? "y" : "x"}`
  let c = (k: number) => `vec3(${half(3 * k)}, ${half(3 * k + 1)}, ${half(3 * k + 2)})`
  let lines = [
    `    vec3 worldCenter = (uModel * vec4(center, 1.0)).xyz;`,
    `    vec3 d = normalize(transpose(mat3(uModel)) * (worldCenter - uCamPos));`,
    `    uint s = id * ${texels}u;`,
    ...Array.from({ length: texels }, (_, j) => fetch(j)),
    `    float x = d.x, y = d.y, z = d.z;`,
    `    vec3 sh = -SH_C1 * y * ${c(0)} + SH_C1 * z * ${c(1)} - SH_C1 * x * ${c(2)};`,
  ]
  if (degree > 1) {
    lines.push(
      `    float xx = x * x, yy = y * y, zz = z * z, xy = x * y, yz = y * z, xz = x * z;`,
      `    sh += SH_C2_0 * xy * ${c(3)} + SH_C2_1 * yz * ${c(4)} + SH_C2_2 * (2.0 * zz - xx - yy) * ${c(5)} + SH_C2_3 * xz * ${c(6)} + SH_C2_4 * (xx - yy) * ${c(7)};`,
    )
  }
  if (degree > 2) {
    lines.push(
      `    sh += SH_C3_0 * y * (3.0 * xx - yy) * ${c(8)} + SH_C3_1 * xy * z * ${c(9)} + SH_C3_2 * y * (4.0 * zz - xx - yy) * ${c(10)} + SH_C3_3 * z * (2.0 * zz - 3.0 * xx - 3.0 * yy) * ${c(11)} + SH_C3_4 * x * (4.0 * zz - xx - yy) * ${c(12)} + SH_C3_5 * z * (xx - yy) * ${c(13)} + SH_C3_6 * x * (xx - 3.0 * yy) * ${c(14)};`,
    )
  }
  // The reference rasterizer clamps below only; the blend saturates above.
  lines.push(`    color.rgb = max(color.rgb + sh, 0.0);`)
  return lines.join("\n") + "\n"
}

// The splat id of this vertex: quad q of the instance reads id q of the
// instance's SPLAT_GROUP ids.
let idSelect = (): string => {
  let lines = [`    uvec4 ids = iIds0;`]
  for (let j = 1; j < SPLAT_GROUP / 4; j++) lines.push(`    if (quad >= ${4 * j}u) ids = iIds${j};`)
  lines.push(`    uint id = ids[quad & 3u];`)
  return lines.join("\n") + "\n"
}

let checkDegree = (degree: number, site: string): void => {
  if (!(Number.isInteger(degree) && degree >= 0 && degree <= 3)) throw new Error(site + ": shDegree must be an integer 0..3, got " + degree)
}

/**
 * The stock splat vertex stage for a cloud baked at `shDegree`: the id
 * from the instance's id record (quad `gl_VertexID / 4` of the merged
 * geometry), the record's center, packed color and precomputed 3D
 * covariance fetched from `uSplatRecords` (two rgba32ui texels), the SH
 * bands from `uSplatSh` (evaluated per corner from uCamPos), the
 * covariance projected to a screen ellipse through the Jacobian of
 * `uViewProj * uModel`, the quad's corners placed along its axes. The
 * texture width is this device's, compiled in. For a forked class that
 * keeps the projection and swaps the shading: fork this source over
 * SPLAT_ID_ATTRIBUTES, and pass the class's instance as the mesh
 * material (the mesh binds the textures).
 */
export function splatVertexSource(shDegree = 0): string {
  checkDegree(shDegree, "splatVertexSource")
  let { width, log2 } = textureWidth()
  let attributes = SPLAT_ID_ATTRIBUTES.map(a => `  in uvec4 ${a.name};\n`).join("")
  return glsl`
  in vec3 aPos;
${attributes}  uniform mat4 uModel;
  uniform mat4 uViewProj;
  uniform vec2 uViewport;
  uniform float uBlendSpace;
  uniform highp usampler2D uSplatRecords;
${shDegree > 0 ? `  uniform highp usampler2D uSplatSh;\n  uniform vec3 uCamPos;\n${SH_GLSL}` : ""}  out vec4 vColor;
  out vec2 vOffset;
  ${SPLAT_CONSTANTS}
  ${SRGB}
  // The 3DGS anti-aliasing dilation: a variance in pixels squared added to
  // the projected footprint, so no splat is thinner than about a pixel.
  const float LOW_PASS = 0.3;
  // The largest on-screen standard deviation in pixels, capping the quad
  // of a splat the camera sits inside (the reference viewers' cap).
  const float MAX_SIGMA_PX = 1024.0;
  // Centers this far past the clip edge (in units of w) are culled; a
  // margin keeps big splats centered just off screen.
  const float FRUSTUM_MARGIN = 1.2;
  // Where a culled splat goes: outside the clip volume, so it rasterizes
  // nothing.
  const vec4 CULLED = vec4(0.0, 0.0, 2.0, 1.0);
  // The id past the sorted population (the core's sentinel).
  const uint SPLAT_NONE = ${SPLAT_NONE}u;
  // Texels per record, and the data textures' row addressing.
  const uint SPLAT_RECORD_TEXELS = ${SPLAT_RECORD_TEXELS}u;
  const uint SPLAT_TEXEL_MASK = ${width - 1}u;
  const uint SPLAT_TEXEL_SHIFT = ${log2}u;

  ivec2 splatTexel(uint t) {
    return ivec2(int(t & SPLAT_TEXEL_MASK), int(t >> SPLAT_TEXEL_SHIFT));
  }

  void main() {
    uint quad = uint(gl_VertexID) / 4u;
${idSelect()}    if (id == SPLAT_NONE) {
      gl_Position = CULLED;
      return;
    }
    uint t = id * SPLAT_RECORD_TEXELS;
    uvec4 recA = texelFetch(uSplatRecords, splatTexel(t), 0);
    uvec4 recB = texelFetch(uSplatRecords, splatTexel(t + 1u), 0);
    vec3 center = uintBitsToFloat(recA.xyz);
    // The packed rgba8 (r in the low byte); GLSL ES 3.00 has no
    // unpackUnorm4x8 (that is 3.10), so by hand.
    vec4 color = vec4(float(recA.w & 255u), float((recA.w >> 8u) & 255u), float((recA.w >> 16u) & 255u), float(recA.w >> 24u)) / 255.0;
    vec2 c01 = unpackHalf2x16(recB.x);
    vec2 c23 = unpackHalf2x16(recB.y);
    vec2 c45 = unpackHalf2x16(recB.z);
${shDegree > 0 ? shBlock(shDegree) : ""}    mat4 mvp = uViewProj * uModel;
    vec4 clip = mvp * vec4(center, 1.0);
    if (clip.w <= 0.0 || clip.z < -clip.w || any(greaterThan(abs(clip.xy), vec2(clip.w * FRUSTUM_MARGIN)))) {
      gl_Position = CULLED;
      return;
    }
    // The baked covariance, upper triangle back to the symmetric matrix.
    mat3 sigma = mat3(
      c01.x, c01.y, c23.x,
      c01.y, c23.y, c45.x,
      c23.x, c45.x, c45.y);
    // Jacobian of the pixel position with respect to the local position,
    // straight from the clip transform: d(clip.xy / clip.w) / dp, scaled
    // to pixels. Any clip flip in uViewProj flips center and footprint
    // alike, and any node transform in uModel shapes the footprint with
    // the same matrix that moves the center.
    vec3 row0 = vec3(mvp[0][0], mvp[1][0], mvp[2][0]);
    vec3 row1 = vec3(mvp[0][1], mvp[1][1], mvp[2][1]);
    vec3 row3 = vec3(mvp[0][3], mvp[1][3], mvp[2][3]);
    float w2 = clip.w * clip.w;
    vec3 jx = (row0 * clip.w - row3 * clip.x) / w2 * (0.5 * uViewport.x);
    vec3 jy = (row1 * clip.w - row3 * clip.y) / w2 * (0.5 * uViewport.y);
    vec3 sx = sigma * jx;
    vec3 sy = sigma * jy;
    float a = dot(jx, sx) + LOW_PASS;
    float b = dot(jx, sy);
    float c = dot(jy, sy) + LOW_PASS;
    // Eigen decomposition of the 2x2 screen covariance [[a, b], [b, c]].
    float mid = 0.5 * (a + c);
    float radius = length(vec2(0.5 * (a - c), b));
    float major = mid + radius;
    float minor = mid - radius;
    if (minor <= 0.0) {
      gl_Position = CULLED;
      return;
    }
    // The major eigenvector from whichever form is better conditioned.
    vec2 v1 = vec2(b, major - a);
    vec2 v2 = vec2(major - c, b);
    vec2 v = dot(v1, v1) > dot(v2, v2) ? v1 : v2;
    vec2 e1 = dot(v, v) > 0.0 ? normalize(v) : vec2(1.0, 0.0);
    vec2 e2 = vec2(-e1.y, e1.x);
    vec2 axis1 = e1 * min(sqrt(major), MAX_SIGMA_PX);
    vec2 axis2 = e2 * min(sqrt(minor), MAX_SIGMA_PX);
    vOffset = aPos.xy * SPLAT_EXTENT;
    vec2 pixels = vOffset.x * axis1 + vOffset.y * axis2;
    vec2 ndc = clip.xy / clip.w + pixels * 2.0 / uViewport;
    gl_Position = vec4(ndc * clip.w, clip.z, clip.w);
    // A display-space scene (SceneOptions.blendSpace) blends the
    // record's sRGB color as encoded - what the capture was trained
    // against; a linear scene decodes it like every color input.
    vColor = vec4(uBlendSpace > 0.5 ? color.rgb : srgbToLinear(color.rgb), color.a);
  }
`
}

/** The stock splat fragment: the gaussian falloff times opacity, cut at
 * the extent and below one 8-bit step, premultiplied out. */
export const SPLAT_FRAGMENT = glsl`
  in vec4 vColor;
  in vec2 vOffset;
  ${SPLAT_CONSTANTS}
  // Below this coverage a fragment would not change an 8-bit pixel.
  const float MIN_ALPHA = 1.0 / 255.0;

  void main() {
    float r2 = dot(vOffset, vOffset);
    if (r2 > SPLAT_EXTENT * SPLAT_EXTENT) discard;
    float alpha = vColor.a * exp(-0.5 * r2);
    if (alpha < MIN_ALPHA) discard;
    fragColor = vec4(vColor.rgb * alpha, alpha);
  }
`

// One class and one shared instance per SH degree, created on first use
// (the source compiles the device's texture width in, so the class waits
// for the runtime). The textures bind per MESH, so every cloud of a
// degree shares the instance.
let classes = new Map<number, ShaderMaterialClass>()
let stockMaterials = new Map<number, Material>()
let stockQuads: Geometry | null = null

/**
 * The stock splat material class for clouds baked at `shDegree` (default
 * 0): splatVertexSource(shDegree) and SPLAT_FRAGMENT over the id-stream
 * layout (SPLAT_ID_ATTRIBUTES), transparent, both faces. Cached per
 * degree. A custom look forks the sources into its own shaderMaterialClass
 * over the same layout; a mesh over it still wants createSplatMesh's
 * geometry, order, textures and bounds, so pass its instance as the
 * option there rather than building the mesh by hand.
 */
export function splatMaterialClass(shDegree = 0): ShaderMaterialClass {
  checkDegree(shDegree, "splatMaterialClass")
  let cls = classes.get(shDegree)
  if (cls === undefined) {
    cls = shaderMaterialClass({
      vertex: splatVertexSource(shDegree),
      fragment: SPLAT_FRAGMENT,
      instanceBuffers: [{ attributes: SPLAT_ID_ATTRIBUTES }],
      transparent: true,
      cull: "none",
      label: `splat-sh${shDegree}`,
    })
    classes.set(shDegree, cls)
  }
  return cls
}

/** A splat mesh is a record mesh in the indexed form; everything there
 * applies (setVisible, setTransform, setRecordCount for the draw count -
 * rounded up to whole groups of SPLAT_GROUP - disposeInstances, which
 * frees the id buffer and the data textures). */
export type SplatMesh = RecordMesh

export type SplatMeshOptions = {
  /** How many splats draw (default all). Records are importance-sorted
   * at bake, so the first n are the scene at n; dial later with
   * setRecordCount (the engine re-sorts and republishes the prefix of
   * its own copy). */
  count?: number
  /** A custom splat material: a fork of splatVertexSource(data.shDegree)
   * and SPLAT_FRAGMENT over SPLAT_ID_ATTRIBUTES; default the stock
   * class's shared instance for the cloud's degree. The data textures
   * bind per mesh under the stock names (uSplatRecords, uSplatSh), so a
   * fork keeps them. */
  material?: Material
  /** Debug label for the id buffer and the data textures (default
   * "splat"). */
  label?: string
}

// A data texture from a texel block (SPLAT_TEXEL_BYTES per texel, `count`
// x `texels` of them): the block padded to whole rows of the device
// width, uploaded as rgba32ui. The pad copy is the one per-cloud copy
// loading makes (native, tens of ms at 1M), and it is transient.
function splatTexture(block: Uint8Array, texels: number, count: number, label: string): TextureId {
  let { width } = textureWidth()
  let rows = Math.max(1, Math.ceil((count * texels) / width))
  if (rows > limits.maxTextureSize) {
    throw new Error("createSplatMesh: " + count + " splats need a " + width + "x" + rows + " " + label + " texture; this device allows " + limits.maxTextureSize + " rows")
  }
  let padded = new Uint32Array(rows * width * (SPLAT_TEXEL_BYTES / 4))
  new Uint8Array(padded.buffer).set(block.subarray(0, count * texels * SPLAT_TEXEL_BYTES))
  return createTexture(padded, width, rows, { format: "rgba32ui", autoFree: false, label })
}

/**
 * A mesh drawing a baked splat cloud (loadSplat, or parseSplat at bake
 * scale): a record mesh in the indexed form over the stock splat material
 * for the cloud's SH degree - the records handed to the engine as the
 * depth-sort key source AND uploaded once as the rgba32ui record texture
 * the vertex stage fetches by id, the SH block (when baked) as a second
 * texture, SPLAT_GROUP splats per drawn instance of the merged-quad
 * geometry, the cloud's header bounds as the picking and cull box, and
 * the core-side back-to-front order on the splat centers materialized as
 * the id stream (a camera turn re-sorts and republishes 4 bytes per splat
 * core-side with no publish from JS; an order-preserving move uploads
 * nothing). The cloud is write-once: treat `data` as consumed and let it
 * go (the engine holds its own copy; the fetched bytes free with it), and
 * swap clouds by a new mesh. disposeInstances frees the id buffer, the
 * engine copy and both textures. One caveat carried from the capture
 * side: captures are TRAINED against sRGB blending, so show a capture in
 * a display-space scene (`blendSpace: "display"`) for the trained
 * appearance - a linear scene blends it in linear light, which lays a
 * milky veil over stacked translucent splats
 * (okf/done/gaussian-splats.md).
 */
export function createSplatMesh(data: SplatData, opts?: SplatMeshOptions): SplatMesh {
  checkDegree(data.shDegree, "createSplatMesh")
  if (data.shDegree > 0 && (data.sh === null || data.sh.byteLength < data.count * SPLAT_SH_TEXELS[data.shDegree]! * SPLAT_TEXEL_BYTES)) {
    throw new Error("createSplatMesh: the cloud claims SH degree " + data.shDegree + " but carries no SH block of that size")
  }
  if (stockQuads === null) {
    let one = plane({ width: 2, height: 2, label: "splat-quad" })
    stockQuads = mergeGeometries(Array.from({ length: SPLAT_GROUP }, () => one), "splat-quads")
  }
  let material = opts?.material
  if (material === undefined) {
    material = stockMaterials.get(data.shDegree)
    if (material === undefined) {
      material = splatMaterialClass(data.shDegree).instance()
      stockMaterials.set(data.shDegree, material)
    }
  }
  let label = opts?.label ?? "splat"
  let mesh = createRecordMesh(stockQuads, material, data.records, opts?.count, {
    bounds: data.bounds,
    instanceOrder: { position: "iCenter", records: SPLAT_ATTRIBUTES, descending: true },
    label,
  })
  let textures: TextureBindings = { uSplatRecords: splatTexture(data.records, SPLAT_RECORD_TEXELS, data.count, label + "-records") }
  mesh._instances.textures.push(textures.uSplatRecords as TextureId)
  if (data.shDegree > 0) {
    textures.uSplatSh = splatTexture(data.sh!, SPLAT_SH_TEXELS[data.shDegree]!, data.count, label + "-sh")
    mesh._instances.textures.push(textures.uSplatSh as TextureId)
  }
  mesh._textures = textures
  return mesh
}

/** Read a baked .sol3s splat cloud (`sol tool 3d/splat`): no parsing, the
 * records and the SH block view the file's bytes and upload as-is at
 * createSplatMesh. */
export async function loadSplat(path: string): Promise<SplatData> {
  return decodeSplat(await file(path).bytes())
}
