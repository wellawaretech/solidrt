// The splat RUNTIME side: the stock splat material and createSplatMesh
// over a baked SplatData (parse and bake live in ./splat-data.ts, the
// runtime-free `@solidrt/3d/splat` entry; `sol tool 3d/splat` writes the
// .sol3s this loads). A splat cloud here is an ordinary record mesh in the
// INDEXED form - the records live in an rgba32ui data texture the vertex
// stage fetches by id and decodes against the chunk texture (the chunk's
// position and log-scale boxes), SPLAT_GROUP splats share one drawn
// instance (a merged-quad geometry; the instancing frontend was the
// measured wall), and the instance buffer is the id stream the core sorts
// back to front from the packed position words, the scene feeding the
// view direction: zero per-frame JS, a re-sort uploads 4 bytes per splat,
// a parked camera uploads nothing. Depth-tested against opaque geometry,
// placed by the transparent entry sort, picked by its bounds, transformed
// by its node. The spherical-harmonic bands a bake carries (`--sh`) ride
// a third data texture, evaluated per corner from the view direction.

import { file } from "flux:fs"
import { createMutableTexture, limits, uploadTexture } from "@solidrt/core/gpu"
import type { TextureBindings, TextureId, VertexAttribute } from "@solidrt/core/gpu"
import { mergeGeometries, plane } from "./geometry.ts"
import type { Geometry } from "./geometry.ts"
import { shaderMaterialClass } from "./material.ts"
import type { Material, ShaderMaterialClass } from "./material.ts"
import { createRecordMesh } from "./mesh.ts"
import type { RecordMesh } from "./mesh.ts"
import {
  decodeSplat,
  splatChunkBounds,
  SPLAT_ATTRIBUTES,
  SPLAT_CHUNK_BYTES,
  SPLAT_CHUNK_SIZE,
  SPLAT_CHUNK_TEXELS,
  SPLAT_COLOR_BYTES,
  SPLAT_POSITION_BITS,
  SPLAT_RECORD_TEXELS,
  SPLAT_ROTATION_BITS,
  SPLAT_SCALE_BITS,
  SPLAT_SH_BYTE_SCALE,
  SPLAT_SH_BYTE_ZERO,
  SPLAT_SH_LABEL_BYTES,
  SPLAT_SH_LABEL_TEXELS,
  SPLAT_SH_PALETTE_SIZE,
  SPLAT_SH_SUBSPACES,
  SPLAT_SH_TEXELS,
  SPLAT_TEXEL_BYTES,
} from "./splat-data.ts"
import type { SplatData, SplatShEncoding } from "./splat-data.ts"
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
// cloud's records at 251 rows and its SH3 block at 752.
const SPLAT_TEXTURE_WIDTH_MAX = 4096
// The id past the sorted population (the core's INDEX_NONE): culled
// before any fetch.
const SPLAT_NONE = 0xffffffff
// The packing of a splat record's position words, what the engine's key
// decodes (the core's PackedFormat).
const SPLAT_PACKED_FORMAT = "unorm16x3"

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
  // The reference rasterizer's cap on a splat's alpha: no splat fully
  // occludes what is behind it, and trained opacities assume the cap.
  const float SPLAT_ALPHA_MAX = 0.99;
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

// Under u8, byte 3k + c of the splat's block is word j / 4 of texel
// j / 16: the coefficient's rgb as three bytes.
let u8ByteAt = (k: number, c: number): string => {
  let j = 3 * k + c
  return `shByte(t${j >> 4}[${(j >> 2) & 3}] >> ${8 * (j & 3)}u)`
}
// Under pq, coefficient k's green is byte k - first of its luma
// subspace's palette texel, and its red and blue are green plus the
// differences at bytes k - first and count + k - first of its chroma
// subspace's texel.
let pqByteAt = (plane: "luma" | "chroma", k: number, second: boolean): string => {
  let j = SPLAT_SH_SUBSPACES.findIndex(sub => sub.plane === plane && k >= sub.first && k < sub.first + sub.count)
  let sub = SPLAT_SH_SUBSPACES[j]!
  let byte = k - sub.first + (second ? sub.count : 0)
  return `shByte(t${j}[${byte >> 2}] >> ${8 * (byte & 3)}u)`
}

// The SH evaluation for `degree` under `encoding`: the view direction in
// the model frame (uCamPos through the transposed model rotation - exact
// under rotation and uniform scale), the texels fetched (u8: the splat's
// SPLAT_SH_TEXELS coefficient texels; pq: the splat's label texels, then
// one palette texel per subspace at label x subspaces + subspace), the
// coefficients unpacked from them at constant positions (u8ByteAt,
// pqByteAt with the chroma differences added back to green), the basis
// polynomials per band added to the base color.
let shBlock = (degree: number, encoding: SplatShEncoding): string => {
  let subspaces = SPLAT_SH_LABEL_BYTES[degree]!
  let texels = encoding === "pq" ? subspaces : SPLAT_SH_TEXELS[degree]!
  let fetch =
    encoding === "pq"
      ? (j: number) => `    uvec4 t${j} = texelFetch(uSplatShPalette, splatTexel(l${j >> 2}[${j & 3}] * ${subspaces}u + ${j}u), 0);`
      : (j: number) => `    uvec4 t${j} = texelFetch(uSplatSh, splatTexel(s + ${j}u), 0);`
  let c =
    encoding === "pq"
      ? (k: number) => `shRgb(${pqByteAt("luma", k, false)}, ${pqByteAt("chroma", k, false)}, ${pqByteAt("chroma", k, true)})`
      : (k: number) => `vec3(${u8ByteAt(k, 0)}, ${u8ByteAt(k, 1)}, ${u8ByteAt(k, 2)})`
  let labelTexels = SPLAT_SH_LABEL_TEXELS[degree]!
  let lines = [
    `    vec3 worldCenter = (uModel * vec4(center, 1.0)).xyz;`,
    `    vec3 d = normalize(transpose(mat3(uModel)) * (worldCenter - uCamPos));`,
    ...(encoding === "pq"
      ? Array.from({ length: labelTexels }, (_, t) => `    uvec4 l${t} = texelFetch(uSplatShLabels, splatTexel(id * ${labelTexels}u + ${t}u), 0);`)
      : [`    uint s = id * ${texels}u;`]),
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

/** What a splat material is compiled for: the cloud's SH degree (default
 * 0) and encoding (default "pq"; moot at degree 0), and whether the
 * Mip-Splatting compensation applies (default false) - the bake's
 * `shDegree`, `shEncoding` and `antialiased`. */
export type SplatMaterialOptions = {
  shDegree?: number
  shEncoding?: SplatShEncoding
  antialiased?: boolean
}

// The options resolved, validated and keyed, naming `site` in a throw.
type SplatMaterialKind = { shDegree: number; shEncoding: SplatShEncoding; antialiased: boolean; key: string }

let materialKind = (opts: SplatMaterialOptions | undefined, site: string): SplatMaterialKind => {
  let shDegree = opts?.shDegree ?? 0
  let shEncoding = opts?.shEncoding ?? "pq"
  let antialiased = opts?.antialiased === true
  if (!(Number.isInteger(shDegree) && shDegree >= 0 && shDegree <= 3)) throw new Error(site + ": shDegree must be an integer 0..3, got " + shDegree)
  if (shEncoding !== "u8" && shEncoding !== "pq") throw new Error(site + ": shEncoding must be \"u8\" or \"pq\", got " + String(shEncoding))
  if (shDegree === 0) shEncoding = "u8"
  return { shDegree, shEncoding, antialiased, key: `${shDegree}:${shEncoding}:${antialiased ? "aa" : "plain"}` }
}

/**
 * The stock splat vertex stage for a cloud baked as `opts` says: the id
 * from the instance's id record (quad `gl_VertexID / 4` of the merged
 * geometry), the record's one rgba32ui texel from `uSplatRecords` decoded
 * against its chunk's three rgba32f texels from `uSplatChunks` (the
 * 16-bit position and the packed log scale over the chunk's boxes, the
 * smallest-three rotation - decodeRecord in splat-data.ts is the
 * reference), its rgba8 color from `uSplatColors`, the 3D covariance R S St Rt rebuilt
 * from them, the SH bands (evaluated per corner from uCamPos) from
 * `uSplatSh` under the u8 encoding or from the splat's labels in
 * `uSplatShLabels` through the codebook texels of `uSplatShPalette` under
 * pq, green plus the chroma differences per coefficient (decodeSh is the
 * reference), the covariance projected to a screen
 * ellipse through the Jacobian of `uViewProj * uModel`, the quad's corners
 * placed along its axes. `antialiased` applies the Mip-Splatting
 * compensation (the footprint's determinant before and after the dilation)
 * a capture trained in that mode expects. The texture width is this
 * device's, compiled in. For a forked class that keeps the projection and
 * swaps the shading: fork this source over SPLAT_ID_ATTRIBUTES, and pass
 * the class's instance as the mesh material (the mesh binds the textures).
 */
export function splatVertexSource(opts?: SplatMaterialOptions): string {
  let { shDegree, shEncoding, antialiased } = materialKind(opts, "splatVertexSource")
  let { width, log2 } = textureWidth()
  let attributes = SPLAT_ID_ATTRIBUTES.map(a => `  in uvec4 ${a.name};\n`).join("")
  let [bitsX, bitsY, bitsZ] = SPLAT_SCALE_BITS as [number, number, number]
  return glsl`
  in vec3 aPos;
${attributes}  uniform mat4 uModel;
  uniform mat4 uViewProj;
  uniform vec2 uViewport;
  uniform float uBlendSpace;
  uniform highp usampler2D uSplatRecords;
  uniform highp sampler2D uSplatChunks;
  uniform highp sampler2D uSplatColors;
${shDegree > 0 ? `${shEncoding === "pq" ? "  uniform highp usampler2D uSplatShLabels;\n  uniform highp usampler2D uSplatShPalette;\n" : "  uniform highp usampler2D uSplatSh;\n"}  uniform vec3 uCamPos;\n${SH_GLSL}` : ""}  out vec4 vColor;
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
  // Texels per record and per chunk, records per chunk, and the data
  // textures' row addressing.
  const uint SPLAT_RECORD_TEXELS = ${SPLAT_RECORD_TEXELS}u;
  const uint SPLAT_CHUNK_TEXELS = ${SPLAT_CHUNK_TEXELS}u;
  const uint SPLAT_CHUNK_SIZE = ${SPLAT_CHUNK_SIZE}u;
  const uint SPLAT_TEXEL_MASK = ${width - 1}u;
  const uint SPLAT_TEXEL_SHIFT = ${log2}u;
  // The position's 16-bit unorms (x | y << 16, then z) and the scale
  // word's 11/10/11 (x in the top bits, then y, z); the unorm ranges.
  const float POSITION_MAX = ${(1 << SPLAT_POSITION_BITS) - 1}.0;
  const uint POSITION_MASK = ${(1 << SPLAT_POSITION_BITS) - 1}u;
  const vec3 SCALE_MAX = vec3(${(1 << bitsX) - 1}.0, ${(1 << bitsY) - 1}.0, ${(1 << bitsZ) - 1}.0);
  // The stored quaternion components: 10 bits each over [-1/sqrt2,
  // 1/sqrt2] in an even number of steps, the largest's index on top.
  const float ROTATION_MAX = ${(1 << SPLAT_ROTATION_BITS) - 2}.0;
  const uint ROTATION_MASK = ${(1 << SPLAT_ROTATION_BITS) - 1}u;
  const float SQRT1_2 = 0.70710678118654752;
  // An SH coefficient byte decodes as (byte - zero) / scale.
  const float SH_BYTE_ZERO = ${SPLAT_SH_BYTE_ZERO}.0;
  const float SH_BYTE_SCALE = ${SPLAT_SH_BYTE_SCALE}.0;

  ivec2 splatTexel(uint t) {
    return ivec2(int(t & SPLAT_TEXEL_MASK), int(t >> SPLAT_TEXEL_SHIFT));
  }

  vec3 unpackScale(uint w) {
    return vec3(float(w >> ${bitsY + bitsZ}u), float((w >> ${bitsZ}u) & ${(1 << bitsY) - 1}u), float(w & ${(1 << bitsZ) - 1}u)) / SCALE_MAX;
  }

  vec3 unpackPosition(uint xy, uint z) {
    return vec3(float(xy & POSITION_MASK), float(xy >> ${SPLAT_POSITION_BITS}u), float(z & POSITION_MASK)) / POSITION_MAX;
  }

  float shByte(uint shifted) {
    return (float(shifted & 255u) - SH_BYTE_ZERO) / SH_BYTE_SCALE;
  }

  // A pq coefficient's rgb from its planes: green, and the red and blue
  // differences from green.
  vec3 shRgb(float g, float rd, float bd) {
    return vec3(g + rd, g, g + bd);
  }

  void main() {
    uint quad = uint(gl_VertexID) / 4u;
${idSelect()}    if (id == SPLAT_NONE) {
      gl_Position = CULLED;
      return;
    }
    uvec4 rec = texelFetch(uSplatRecords, splatTexel(id * SPLAT_RECORD_TEXELS), 0);
    uint chunk = (id / SPLAT_CHUNK_SIZE) * SPLAT_CHUNK_TEXELS;
    vec4 c0 = texelFetch(uSplatChunks, splatTexel(chunk), 0);
    vec4 c1 = texelFetch(uSplatChunks, splatTexel(chunk + 1u), 0);
    vec4 c2 = texelFetch(uSplatChunks, splatTexel(chunk + 2u), 0);
    vec3 center = mix(c0.xyz, vec3(c0.w, c1.xy), unpackPosition(rec.x, rec.y));
    vec3 scale = exp(mix(vec3(c1.zw, c2.x), c2.yzw, unpackScale(rec.w)));
    // The rgba8 color texel, sampled as unorms.
    vec4 color = texelFetch(uSplatColors, splatTexel(id), 0);
    // Smallest-three: the three stored components in index order, the
    // largest (made positive at bake) from the unit norm.
    uint largest = rec.z >> 30u;
    vec3 abc = (vec3(float((rec.z >> ${2 * SPLAT_ROTATION_BITS}u) & ROTATION_MASK), float((rec.z >> ${SPLAT_ROTATION_BITS}u) & ROTATION_MASK), float(rec.z & ROTATION_MASK)) / ROTATION_MAX * 2.0 - 1.0) * SQRT1_2;
    float rest = sqrt(max(0.0, 1.0 - dot(abc, abc)));
    vec4 q = largest == 0u ? vec4(rest, abc) : largest == 1u ? vec4(abc.x, rest, abc.yz) : largest == 2u ? vec4(abc.xy, rest, abc.z) : vec4(abc, rest);
    float qw = q.x, qx = q.y, qy = q.z, qz = q.w;
    // R by columns, M = R diag(scale), sigma = M Mt.
    mat3 rot = mat3(
      1.0 - 2.0 * (qy * qy + qz * qz), 2.0 * (qx * qy + qw * qz), 2.0 * (qx * qz - qw * qy),
      2.0 * (qx * qy - qw * qz), 1.0 - 2.0 * (qx * qx + qz * qz), 2.0 * (qy * qz + qw * qx),
      2.0 * (qx * qz + qw * qy), 2.0 * (qy * qz - qw * qx), 1.0 - 2.0 * (qx * qx + qy * qy));
    mat3 m = mat3(rot[0] * scale.x, rot[1] * scale.y, rot[2] * scale.z);
    mat3 sigma = m * transpose(m);
${shDegree > 0 ? shBlock(shDegree, shEncoding) : ""}    mat4 mvp = uViewProj * uModel;
    vec4 clip = mvp * vec4(center, 1.0);
    if (clip.w <= 0.0 || clip.z < -clip.w || any(greaterThan(abs(clip.xy), vec2(clip.w * FRUSTUM_MARGIN)))) {
      gl_Position = CULLED;
      return;
    }
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
    float a0 = dot(jx, sx);
    float b = dot(jx, sy);
    float c0d = dot(jy, sy);
    float a = a0 + LOW_PASS;
    float c = c0d + LOW_PASS;
${antialiased ? `    // Mip-Splatting's compensation: the capture was trained with the
    // dilation's density loss folded into opacity, so the renderer
    // scales it by the footprint's determinant ratio.
    color.a *= sqrt(max(0.0, a0 * c0d - b * b) / max(a * c - b * b, 1e-6));
` : ""}    // Eigen decomposition of the 2x2 screen covariance [[a, b], [b, c]].
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
 * the extent and below one 8-bit step, capped at the reference's
 * SPLAT_ALPHA_MAX, premultiplied out. */
export const SPLAT_FRAGMENT = glsl`
  in vec4 vColor;
  in vec2 vOffset;
  ${SPLAT_CONSTANTS}
  // Below this coverage a fragment would not change an 8-bit pixel.
  const float MIN_ALPHA = 1.0 / 255.0;

  void main() {
    float r2 = dot(vOffset, vOffset);
    if (r2 > SPLAT_EXTENT * SPLAT_EXTENT) discard;
    float alpha = min(SPLAT_ALPHA_MAX, vColor.a * exp(-0.5 * r2));
    if (alpha < MIN_ALPHA) discard;
    fragColor = vec4(vColor.rgb * alpha, alpha);
  }
`

// One class and one shared instance per (SH degree, SH encoding,
// antialiased), created on first use (the source compiles the device's
// texture width in, so the class waits for the runtime). The textures bind
// per MESH, so every cloud of a kind shares the instance.
let classes = new Map<string, ShaderMaterialClass>()
let stockMaterials = new Map<string, Material>()
let stockQuads: Geometry | null = null

/**
 * The stock splat material class for clouds baked as `opts` says (the
 * bake's shDegree, shEncoding and antialiased; defaults 0, "pq", false):
 * splatVertexSource(opts) and SPLAT_FRAGMENT over the id-stream layout
 * (SPLAT_ID_ATTRIBUTES), transparent, both faces. Cached per kind. A
 * custom look forks the sources into its own shaderMaterialClass over the
 * same layout; a mesh over it still wants createSplatMesh's geometry,
 * order, textures and bounds, so pass its instance as the option there
 * rather than building the mesh by hand.
 */
export function splatMaterialClass(opts?: SplatMaterialOptions): ShaderMaterialClass {
  let kind = materialKind(opts, "splatMaterialClass")
  let cls = classes.get(kind.key)
  if (cls === undefined) {
    cls = shaderMaterialClass({
      vertex: splatVertexSource(kind),
      fragment: SPLAT_FRAGMENT,
      instanceBuffers: [{ attributes: SPLAT_ID_ATTRIBUTES }],
      transparent: true,
      cull: "none",
      label: `splat-sh${kind.shDegree}${kind.shDegree > 0 ? "-" + kind.shEncoding : ""}${kind.antialiased ? "-aa" : ""}`,
    })
    classes.set(kind.key, cls)
  }
  return cls
}

/** A splat mesh is a record mesh in the indexed form; everything there
 * applies (setVisible, setTransform, setRecordCount for the draw count -
 * rounded up to whole groups of SPLAT_GROUP - disposeInstances, which
 * frees the id buffer and the data textures). */
export type SplatMesh = RecordMesh

export type SplatMeshOptions = {
  /** How many splats draw (default all): the first n of the bake's draw
   * order, by importance, so the first n are the scene at n; dial later
   * with setRecordCount (the engine re-sorts and republishes the prefix
   * of its own copy). */
  count?: number
  /** A custom splat material: a fork of splatVertexSource(data) and
   * SPLAT_FRAGMENT over SPLAT_ID_ATTRIBUTES; default the stock class's
   * shared instance for the cloud's degree, encoding and antialiasing.
   * The data textures bind per mesh under the stock names (uSplatRecords,
   * uSplatChunks, uSplatColors, and uSplatSh or uSplatShLabels +
   * uSplatShPalette by the encoding), so a fork keeps them. */
  material?: Material
  /** Debug label for the id buffer and the data textures (default
   * "splat"). */
  label?: string
}

// The most bytes one band of a data texture upload carries: a block
// uploads as row bands straight from its source view, so this bounds the
// load's transient memory (one band in flight) where a padded whole-block
// copy would hold the block twice.
const SPLAT_UPLOAD_BAND_BYTES = 4 << 20

type SplatTextureFormat = "rgba32ui" | "rgba32f" | "rgba8" | "r8ui" | "rg8ui" | "rgba8ui"

// Bytes per texel of each data texture format.
const SPLAT_FORMAT_BYTES: Record<SplatTextureFormat, number> = { rgba32ui: SPLAT_TEXEL_BYTES, rgba32f: SPLAT_TEXEL_BYTES, rgba8: SPLAT_COLOR_BYTES, r8ui: 1, rg8ui: 2, rgba8ui: 4 }

// The label texture's integer format per SH degree: SPLAT_SH_LABEL_TEXELS
// texels of SPLAT_SH_LABEL_TEXEL_BYTES per splat, read as a uint per
// component.
const SPLAT_LABEL_FORMATS: SplatTextureFormat[] = ["rgba8ui", "rg8ui", "rgba8ui", "rgba8ui"]

// A data texture from a texel block (`count` x `texels` texels of the
// format's size), rgba32ui, rgba32f or a byte format: allocated at its size, then
// filled in place from the block - whole rows of the device width in
// bands, the tail of the last row as a partial rect. No padded copy: a
// bake's bytes or the .sol3s payload upload as they are, and the texels
// past the block (the rest of the last row) are never fetched - every id
// the shader sees is below the count or the sentinel.
function splatTexture(block: Uint8Array, texels: number, count: number, format: SplatTextureFormat, label: string): TextureId {
  let { width } = textureWidth()
  let total = count * texels
  let rows = Math.max(1, Math.ceil(total / width))
  if (rows > limits.maxTextureSize) {
    throw new Error("createSplatMesh: " + count + " splats need a " + width + "x" + rows + " " + label + " texture; this device allows " + limits.maxTextureSize + " rows")
  }
  let id = createMutableTexture(null, width, rows, { format, autoFree: false, label })
  let texelBytes = SPLAT_FORMAT_BYTES[format]
  let rowBytes = width * texelBytes
  let bandRows = Math.max(1, Math.floor(SPLAT_UPLOAD_BAND_BYTES / rowBytes))
  let fullRows = Math.floor(total / width)
  for (let y = 0; y < fullRows; y += bandRows) {
    let height = Math.min(bandRows, fullRows - y)
    uploadTexture(id, texelWords(block, y * rowBytes, height * rowBytes, format), { x: 0, y, width, height })
  }
  let tail = total - fullRows * width
  if (tail > 0) {
    uploadTexture(id, texelWords(block, fullRows * rowBytes, tail * texelBytes, format), { x: 0, y: fullRows, width: tail, height: 1 })
  }
  return id
}

// `bytes` of `block` from `at` as the typed array an upload of `format`
// takes (u32 words for rgba32ui, floats for rgba32f, bytes for the byte
// formats): a view when the range sits on a word boundary (a bake's buffer
// and the .sol3s payload do), a copy of just that range otherwise.
function texelWords(block: Uint8Array, at: number, bytes: number, format: SplatTextureFormat): Uint32Array | Float32Array | Uint8Array {
  if (format !== "rgba32ui" && format !== "rgba32f") return block.subarray(at, at + bytes)
  let offset = block.byteOffset + at
  let buffer = offset % 4 === 0 ? block.buffer : block.slice(at, at + bytes).buffer
  let start = offset % 4 === 0 ? offset : 0
  return format === "rgba32ui" ? new Uint32Array(buffer, start, bytes / 4) : new Float32Array(buffer, start, bytes / 4)
}

/**
 * A mesh drawing a baked splat cloud (loadSplat, or parseSplat at bake
 * scale): a record mesh in the indexed form over the stock splat material
 * for the cloud's SH degree and antialiasing - the records' packed
 * position words kept by the engine as the depth-sort key beside their
 * ids in the bake's draw order (12 bytes per splat, decoded against the
 * chunk table the engine also keeps) AND the record block uploaded once,
 * in place and in row bands, as the rgba32ui record texture the vertex
 * stage fetches by id, the chunk table as an rgba32f texture, the colors
 * as an rgba8 texture, the SH bands (when baked) as a fourth - the u8
 * coefficient texels, or the pq labels as an 8-bit integer texture beside
 * the 32 KB palette texture - SPLAT_GROUP
 * splats per drawn instance of
 * the merged-quad geometry, the cloud's header bounds as the picking and
 * cull box, and the core-side back-to-front order materialized as the id
 * stream (a camera turn re-sorts and republishes 4 bytes per splat
 * core-side with no publish from JS; an order-preserving move uploads
 * nothing). The cloud is write-once: treat `data` as consumed and let it
 * go (the engine holds the keys; the fetched bytes free with it), and
 * swap clouds by a new mesh. disposeInstances frees the id buffer, the
 * engine copy and the textures. One caveat carried from the capture
 * side: captures are TRAINED against sRGB blending, so show a capture in
 * a display-space scene (`blendSpace: "display"`) for the trained
 * appearance - a linear scene blends it in linear light, which lays a
 * milky veil over stacked translucent splats
 * (okf/done/gaussian-splats.md).
 */
export function createSplatMesh(data: SplatData, opts?: SplatMeshOptions): SplatMesh {
  let kind = materialKind(data, "createSplatMesh")
  if (kind.shDegree > 0) {
    let shBytes = kind.shEncoding === "pq" ? SPLAT_SH_LABEL_BYTES[kind.shDegree]! : SPLAT_SH_TEXELS[kind.shDegree]! * SPLAT_TEXEL_BYTES
    if (data.sh === null || data.sh.byteLength < data.count * shBytes) {
      throw new Error("createSplatMesh: the cloud claims SH degree " + kind.shDegree + " as " + kind.shEncoding + " but carries no SH block of that size")
    }
    let paletteBytes = SPLAT_SH_PALETTE_SIZE * SPLAT_SH_LABEL_BYTES[kind.shDegree]! * SPLAT_TEXEL_BYTES
    if (kind.shEncoding === "pq" && (data.shPalette === null || data.shPalette.byteLength !== paletteBytes)) {
      throw new Error("createSplatMesh: a pq-encoded cloud at SH degree " + kind.shDegree + " needs a " + paletteBytes + "-byte palette block")
    }
  }
  let chunkCount = Math.ceil(data.count / SPLAT_CHUNK_SIZE)
  if (data.chunks.byteLength !== chunkCount * SPLAT_CHUNK_BYTES) {
    throw new Error("createSplatMesh: the chunk table holds " + data.chunks.byteLength + " bytes; " + data.count + " splats need " + chunkCount * SPLAT_CHUNK_BYTES)
  }
  if (data.order.length !== data.count) throw new Error("createSplatMesh: the draw order lists " + data.order.length + " ids for " + data.count + " splats")
  if (data.colors.byteLength !== data.count * SPLAT_COLOR_BYTES) throw new Error("createSplatMesh: the color block holds " + data.colors.byteLength + " bytes for " + data.count + " splats")
  if (stockQuads === null) {
    let one = plane({ width: 2, height: 2, label: "splat-quad" })
    stockQuads = mergeGeometries(Array.from({ length: SPLAT_GROUP }, () => one), "splat-quads")
  }
  let material = opts?.material
  if (material === undefined) {
    material = stockMaterials.get(kind.key)
    if (material === undefined) {
      material = splatMaterialClass(kind).instance()
      stockMaterials.set(kind.key, material)
    }
  }
  let label = opts?.label ?? "splat"
  let mesh = createRecordMesh(stockQuads, material, data.records, opts?.count, {
    bounds: data.bounds,
    instanceOrder: {
      position: "iPosition",
      records: SPLAT_ATTRIBUTES,
      descending: true,
      packed: { format: SPLAT_PACKED_FORMAT, chunk: SPLAT_CHUNK_SIZE, bounds: splatChunkBounds(data) },
      order: data.order,
    },
    label,
  })
  let textures: TextureBindings = {
    uSplatRecords: splatTexture(data.records, SPLAT_RECORD_TEXELS, data.count, "rgba32ui", label + "-records"),
    uSplatChunks: splatTexture(data.chunks, SPLAT_CHUNK_TEXELS, chunkCount, "rgba32f", label + "-chunks"),
    uSplatColors: splatTexture(data.colors, 1, data.count, "rgba8", label + "-colors"),
  }
  mesh._instances.textures.push(textures.uSplatRecords as TextureId, textures.uSplatChunks as TextureId, textures.uSplatColors as TextureId)
  if (kind.shDegree > 0 && kind.shEncoding === "pq") {
    let subspaces = SPLAT_SH_LABEL_BYTES[kind.shDegree]!
    textures.uSplatShLabels = splatTexture(data.sh!, SPLAT_SH_LABEL_TEXELS[kind.shDegree]!, data.count, SPLAT_LABEL_FORMATS[kind.shDegree]!, label + "-sh-labels")
    textures.uSplatShPalette = splatTexture(data.shPalette!, subspaces, SPLAT_SH_PALETTE_SIZE, "rgba32ui", label + "-sh-palette")
    mesh._instances.textures.push(textures.uSplatShLabels as TextureId, textures.uSplatShPalette as TextureId)
  } else if (kind.shDegree > 0) {
    textures.uSplatSh = splatTexture(data.sh!, SPLAT_SH_TEXELS[kind.shDegree]!, data.count, "rgba32ui", label + "-sh")
    mesh._instances.textures.push(textures.uSplatSh as TextureId)
  }
  mesh._textures = textures
  return mesh
}

/** Read a baked .sol3s splat cloud (`sol tool 3d/splat`): no parsing, the
 * records, the colors, the chunk table, the draw order and the SH block
 * view the file's bytes and upload or hand off as-is at createSplatMesh. */
export async function loadSplat(path: string): Promise<SplatData> {
  return decodeSplat(await file(path).bytes())
}
