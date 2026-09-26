// The splat RUNTIME side: the stock splat material and createSplatMesh
// over a baked SplatData (parse and bake live in ./splat-data.ts, the
// runtime-free `@solidrt/3d/splat` entry; `srt tool 3d/splat` writes the
// .srts this loads). A splat cloud here is an ordinary record mesh - one
// camera-facing quad per record, depth-tested against opaque geometry,
// placed by the transparent entry sort, picked by its bounds, transformed
// by its node - drawn back to front by the core's retained instance order
// with the scene feeding the view direction: zero per-frame JS, and a
// parked camera uploads nothing.

import { file } from "flux:fs"
import { plane } from "./geometry.ts"
import type { Geometry } from "./geometry.ts"
import { shaderMaterialClass } from "./material.ts"
import type { Material } from "./material.ts"
import { createRecordMesh } from "./mesh.ts"
import type { RecordMesh } from "./mesh.ts"
import { decodeSplat, SPLAT_ATTRIBUTES } from "./splat-data.ts"
import type { SplatData } from "./splat-data.ts"
import { SRGB } from "./glsl.ts"

// core/gpu's glsl tag, aliased locally like ./glsl.ts does.
let glsl = String.raw

const SPLAT_CONSTANTS = glsl`
  // Quad half-extent in standard deviations: the gaussian is cut at
  // exp(-0.5 * 8) = e^-4, where the reference viewers cut it.
  const float SPLAT_EXTENT = 2.8284271;
`

/** The stock splat vertex stage: the record's precomputed 3D covariance
 * (iCovA/iCovB) projected to a screen ellipse through the Jacobian of
 * `uViewProj * uModel`, the quad's corners placed along its axes.
 * Exported for a forked class that keeps the projection and swaps the
 * shading. */
export const SPLAT_VERTEX = glsl`
  in vec3 aPos;
  in vec3 iCenter;
  in vec4 iCovA;
  in vec2 iCovB;
  in vec4 iColor;
  uniform mat4 uModel;
  uniform mat4 uViewProj;
  uniform vec2 uViewport;
  out vec4 vColor;
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

  void main() {
    mat4 mvp = uViewProj * uModel;
    vec4 clip = mvp * vec4(iCenter, 1.0);
    if (clip.w <= 0.0 || clip.z < -clip.w || any(greaterThan(abs(clip.xy), vec2(clip.w * FRUSTUM_MARGIN)))) {
      gl_Position = CULLED;
      return;
    }
    // The baked covariance, upper triangle back to the symmetric matrix.
    mat3 sigma = mat3(
      iCovA.x, iCovA.y, iCovA.z,
      iCovA.y, iCovA.w, iCovB.x,
      iCovA.z, iCovB.x, iCovB.y);
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
    vColor = vec4(srgbToLinear(iColor.rgb), iColor.a);
  }
`

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

/** The stock splat material class - SPLAT_VERTEX/SPLAT_FRAGMENT over the
 * .srts record layout (SPLAT_ATTRIBUTES), transparent, both faces. A
 * custom look forks the sources into its own shaderMaterialClass; a mesh
 * over it still wants createSplatMesh's order and bounds, so pass it as
 * the option there rather than building the mesh by hand. */
export let splatMaterialClass = shaderMaterialClass({
  vertex: SPLAT_VERTEX,
  fragment: SPLAT_FRAGMENT,
  instanceBuffers: [{ attributes: SPLAT_ATTRIBUTES }],
  transparent: true,
  cull: "none",
  label: "splat",
})

// The shared stock pieces, created on the first splat mesh: one material
// instance and one unit quad (corners at +-1 in xy; the vertex stage
// scales them by the projected footprint) however many clouds show.
let stockMaterial: Material | null = null
let stockQuad: Geometry | null = null

/** A splat mesh is a record mesh; everything there applies (setVisible,
 * setTransform, setRecordCount for the draw count, disposeInstances). */
export type SplatMesh = RecordMesh

export type SplatMeshOptions = {
  /** How many splats draw (default all). Records are importance-sorted
   * at bake, so the first n are the scene at n; dial later with
   * setRecordCount. */
  count?: number
  /** A custom splat material (a fork of SPLAT_VERTEX/SPLAT_FRAGMENT over
   * the same record layout); default the stock class's shared instance. */
  material?: Material
  /** Debug label for the record buffer (default "splat"). */
  label?: string
}

/**
 * A mesh drawing a baked splat cloud (loadSplat, or parseSplat at bake
 * scale): a record mesh over the stock splat material with the cloud's
 * records as-is, its header bounds as the picking and cull box, and the
 * core-side back-to-front order on the splat centers (`retain`: a camera
 * turn re-sorts and republishes core-side with no publish from JS, an
 * order-preserving move uploads nothing). One caveat carried from the
 * capture side: captures are trained against sRGB blending, and the
 * scene blends linear - judge a capture side by side before trusting a
 * "correction" (okf/plans/gaussian-splats.md).
 */
export function createSplatMesh(data: SplatData, opts?: SplatMeshOptions): SplatMesh {
  if (stockQuad === null) stockQuad = plane({ width: 2, height: 2, label: "splat-quad" })
  if (opts?.material === undefined && stockMaterial === null) stockMaterial = splatMaterialClass.instance()
  return createRecordMesh(stockQuad, opts?.material ?? stockMaterial!, data.records, opts?.count, {
    bounds: data.bounds,
    instanceOrder: { position: "iCenter", retain: true, descending: true },
    label: opts?.label ?? "splat",
  })
}

/** Read a baked .srts splat cloud (`srt tool 3d/splat`): no parsing, the
 * records view the file's bytes and upload as-is at createSplatMesh. */
export async function loadSplat(path: string): Promise<SplatData> {
  return decodeSplat(await file(path).bytes())
}
