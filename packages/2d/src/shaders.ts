// The sprite pipelines: a unit quad instanced over a layer's atlases,
// mapped world -> clip through uCamera (offset + zoom), uCameraRot ([cos,
// sin, pivotX, pivotY]: the camera rotation about the pivot, identity
// [1, 0, 0, 0]) and uViewport - the projectCamera mapping (camera.ts)
// spelled in GLSL; the two must agree. World and clip space are both
// y-down (core gpu.ts pixel contract), so the mapping carries NO flip
// anywhere - do not add one. Two record layouts share the fragment stage:
// - VERTEX + INSTANCE_ATTRIBUTES: the 20-float interleaved record
//   [cx, cy, w, h, u0, v0, u1, v1, rot, tintR, tintG, tintB, tintA,
//   minScreenPx, maxScreenPx, atlas, outlineR, outlineG, outlineB,
//   outlineWidth], used by the records layer (records.ts) and the tile
//   layer (tiles.ts).
// - VERTEX_SPLIT + INSTANCE_LAYOUTS_SPLIT: the node-backed live layer
//   (layer.ts), pose and style in separate instance-buffer slots - slot 0
//   is the core-written Pose2D record [x, y, angle, sx, sy], slot 1 the
//   JS-written style record [u0, v0, u1, v1, tintR, tintG, tintB, tintA,
//   renderOrder, minScreenPx, maxScreenPx, atlas, outlineR, outlineG,
//   outlineB, outlineWidth]. The shader never reads iRenderOrder - it
//   exists for the core's instanceOrder (orderBy: "renderOrder");
//   declared attributes without an active program attribute are skipped,
//   their bytes pad the stride.
// `atlas` is the sampler index of the record's texture in the layer's
// atlas list: the fragment stage is GENERATED per layer with one sampler
// per declared atlas (fragmentFor), the flat per-instance index picking
// the sampler - the multi-texture batch of PixiJS and Phaser, where Unity
// and Godot break the batch on every texture change. Any number of sheets
// interleave in one draw, key order included. An atlas declared as a
// distance field (`Atlas.sdf`) is decoded in its branch instead of
// sampled as colour: the median of rgb is the edge, anti-aliased over one
// screen pixel from the quad's own texel footprint (the msdfgen reference
// shader), the true field in alpha grows the per-instance outline.
// `outline` is rgb plus a width in WORLD pixels; the vertex stage turns
// the width into screen pixels through the camera zoom and the screen-size
// clamp, so an outline scales with its glyph. A colour atlas ignores it.
// Both vertex stages apply the screen-size clamp (SpriteOptions.
// minScreenPx/maxScreenPx) through core's screenSizeScale: the quad
// scales uniformly so its smaller axis stays within the bounds at the
// camera's zoom (the pixels per world unit), and picking applies the JS
// twin to the rect it tests.
// The rotations here (clockwise, y-down) and their JS partners must
// agree: iRot with pointInSprite in pick.ts, uCameraRot with
// projectCamera in camera.ts. The differential checks guard the JS side
// against oracles but NOT against these shaders - if you touch one
// rotation, touch all.
import { compileShader, createBuffer, createRenderPipeline, destroyBuffer, destroyProgram, destroyRenderPipeline, destroyShader, glsl, linkProgram, SCREEN_SIZE_GLSL } from "@solidrt/core/gpu"
import type { BlendMode, BufferId, RenderPipelineId, TextureBindings, TextureId, VertexAttribute } from "@solidrt/core/gpu"

/** What the fragment stage reads of a declared atlas: whether it is a
 * distance field, and its range (see `Atlas.sdf`). */
export type AtlasShading = { sdf?: { range: number } }

// The world -> clip mapping and the varyings both vertex stages share;
// `corner` is the quad corner after the screen-size clamp, `center` the
// sprite's world position.
const VERTEX_BODY = glsl`
    float c = cos(rot), s = sin(rot);
    vec2 world = center + vec2(corner.x * c - corner.y * s, corner.x * s + corner.y * c);
    vec2 view = (world - uCamera.xy) * uCamera.zw;
    vec2 screen = uCameraRot.zw + vec2(view.x * uCameraRot.x - view.y * uCameraRot.y, view.x * uCameraRot.y + view.y * uCameraRot.x);
    // World and clip are both y-down, so the mapping carries no flip.
    gl_Position = vec4(screen / uViewport * 2.0 - 1.0, 0.0, 1.0);
    vUv = mix(iUv.xy, iUv.zw, aPos + 0.5);
    vFrame = iUv;
    vTint = iTint;
    vAtlas = int(iAtlas);
    vOutlineColor = iOutline.rgb;
    // World pixels to screen pixels: the camera zoom, and the clamp the
    // quad itself scaled by.
    vOutlinePx = iOutline.a * uCamera.z * clampScale;
`

const VARYINGS = glsl`
  out vec2 vUv;
  flat out vec4 vFrame;
  out vec4 vTint;
  flat out int vAtlas;
  flat out vec3 vOutlineColor;
  flat out float vOutlinePx;
  uniform vec2 uViewport;
  uniform vec4 uCamera;
  uniform vec4 uCameraRot;
`

export let VERTEX = glsl`
  in vec2 aPos;
  in vec2 iCenter;
  in vec2 iSize;
  in vec4 iUv;
  in float iRot;
  in vec4 iTint;
  in vec2 iScreenPx;
  in float iAtlas;
  in vec4 iOutline;
  ${VARYINGS}
  ${SCREEN_SIZE_GLSL}
  void main() {
    float clampScale = screenSizeScale(iSize, uCamera.z, iScreenPx);
    vec2 corner = aPos * iSize * clampScale;
    vec2 center = iCenter;
    float rot = iRot;
    ${VERTEX_BODY}
  }
`

/** The sampler uniform of atlas `i` in a layer's pipeline. */
export function atlasUniform(i: number): string {
  return `uAtlas${i}`
}

/** The texture bindings of a layer's atlases, by sampler uniform: what
 * every target drawing with the layer's pipeline binds. */
export function atlasBindings(atlases: readonly TextureId[]): TextureBindings {
  let out: TextureBindings = {}
  atlases.forEach((texture, i) => (out[atlasUniform(i)] = texture))
  return out
}

/**
 * The fragment stage of a layer over its atlases: one sampler per atlas,
 * the record's atlas index (flat, so one per instance) picking it, and a
 * distance-field decode in the branch of every atlas declared `sdf`.
 *
 * uTint is the LAYER tint, multiplied over the per-instance vTint
 * (identity [1, 1, 1, 1] - GLSL uniforms default to ZERO, so every target
 * creation must pin it explicitly or everything renders transparent
 * black).
 *
 * The sample is clamped to its frame, pulled in by half a texel of ITS
 * atlas: a quad at a fractional position lands edge samples so close to
 * the frame's edge that a linear tap (and float interpolation under
 * nearest) reaches the cell next door, painting a one-texel line of the
 * neighbour along the sprite's edge. The clamp keeps the quad's texel
 * mapping 1:1 and only moves those edge samples onto the edge texel, so
 * touching cells do not bleed and no gutter is needed at mip level 0. It
 * cannot help a mip chain, whose texels straddle the cell edge before
 * sampling - that is extrude.ts. min/max because a flipped frame stores
 * its UVs swapped, and the inner min/max because a sub-texel frame would
 * otherwise hand clamp() a lower bound above its upper one.
 *
 * The derivatives are taken from the UNCLAMPED uv before any branch and
 * the tap is textureGrad: the mip footprint is the quad's own (a clamped
 * uv has a zero derivative along the clamped axis, which would pick a
 * sharper level along every edge), and the lookup is well defined inside
 * the per-instance branch (an implicit-derivative texture() in
 * non-uniform control flow is not, which the PixiJS-style batch lives
 * with). A single-atlas layer skips the branch and keeps the same tap.
 *
 * A colour atlas yields the texel times the instance tint (premultiplied
 * in, premultiplied out). A distance-field atlas yields the instance
 * tint as the fill, anti-aliased over one screen pixel: the field's range
 * mapped through the quad's texel footprint gives the screen pixels per
 * field unit (floored at 1, so a glyph far below its texel size fades
 * instead of vanishing), the median of rgb is the edge, and when the
 * instance carries an outline the true field in alpha, pushed out by the
 * outline's screen pixels, paints the outline colour under the fill.
 */
export function fragmentFor(atlases: readonly AtlasShading[]): string {
  if (!Array.isArray(atlases) || atlases.length === 0) throw new Error(`fragmentFor: atlases must be a non-empty list, got ${JSON.stringify(atlases)}`)
  let samplers = ""
  atlases.forEach((_, i) => (samplers += `uniform sampler2D ${atlasUniform(i)};\n  `))
  let shade = (i: number) => {
    let sdf = atlases[i]!.sdf
    if (sdf === undefined) return `tap(${atlasUniform(i)}, dx, dy) * vTint`
    if (!(typeof sdf.range === "number" && sdf.range > 0 && Number.isFinite(sdf.range))) {
      throw new Error(`fragmentFor: atlases[${i}].sdf.range must be a positive number, got ${sdf.range}`)
    }
    return `field(${atlasUniform(i)}, ${sdf.range.toFixed(2)}, dx, dy)`
  }
  let cases = ""
  atlases.forEach((_, i) => (cases += `case ${i}: color = ${shade(i)}; break;\n      `))
  let pick =
    atlases.length === 1
      ? `vec4 color = ${shade(0)};`
      : `vec4 color;
    switch (vAtlas) {
      ${cases}default: color = ${shade(0)}; break;
    }`
  return glsl`
  in vec2 vUv;
  flat in vec4 vFrame;
  in vec4 vTint;
  flat in int vAtlas;
  flat in vec3 vOutlineColor;
  flat in float vOutlinePx;
  ${samplers}uniform vec4 uTint;

  vec4 tap(sampler2D atlas, vec2 dx, vec2 dy) {
    vec2 halfTexel = 0.5 / vec2(textureSize(atlas, 0));
    vec2 lo = min(vFrame.xy, vFrame.zw) + halfTexel;
    vec2 hi = max(vFrame.xy, vFrame.zw) - halfTexel;
    return textureGrad(atlas, clamp(vUv, min(lo, hi), max(lo, hi)), dx, dy);
  }

  float median3(vec3 v) {
    return max(min(v.r, v.g), min(max(v.r, v.g), v.b));
  }

  // Screen pixels per field unit (0..1 spans the range): the range in
  // texels over the quad's texel footprint per screen pixel, floored at
  // one so a minified glyph fades rather than aliasing away.
  float screenPxRange(sampler2D atlas, float range, vec2 dx, vec2 dy) {
    vec2 unitRange = vec2(range) / vec2(textureSize(atlas, 0));
    vec2 screenTexSize = vec2(1.0) / (abs(dx) + abs(dy));
    return max(0.5 * dot(unitRange, screenTexSize), 1.0);
  }

  vec4 field(sampler2D atlas, float range, vec2 dx, vec2 dy) {
    vec4 t = tap(atlas, dx, dy);
    float px = screenPxRange(atlas, range, dx, dy);
    float fill = clamp((median3(t.rgb) - 0.5) * px + 0.5, 0.0, 1.0);
    vec3 rgb = vTint.rgb;
    float coverage = fill;
    if (vOutlinePx > 0.0) {
      float outline = clamp((t.a - 0.5) * px + vOutlinePx + 0.5, 0.0, 1.0);
      coverage = max(fill, outline);
      rgb = mix(vOutlineColor, vTint.rgb, fill);
    }
    float a = coverage * vTint.a;
    return vec4(rgb * a, a);
  }

  void main() {
    vec2 dx = dFdx(vUv);
    vec2 dy = dFdy(vUv);
    ${pick}
    fragColor = color * uTint;
  }
`
}

/** The instance attribute list matching the 20-float record layout. */
export const INSTANCE_ATTRIBUTES: VertexAttribute[] = [
  { name: "iCenter", format: "float32x2" },
  { name: "iSize", format: "float32x2" },
  { name: "iUv", format: "float32x4" },
  { name: "iRot", format: "float32" },
  { name: "iTint", format: "float32x4" },
  { name: "iScreenPx", format: "float32x2" },
  { name: "iAtlas", format: "float32" },
  { name: "iOutline", format: "float32x4" },
]

export let VERTEX_SPLIT = glsl`
  in vec2 aPos;
  in vec2 iPos;
  in float iRot;
  in vec2 iScale;
  in vec4 iUv;
  in vec4 iTint;
  in vec2 iScreenPx;
  in float iAtlas;
  in vec4 iOutline;
  ${VARYINGS}
  ${SCREEN_SIZE_GLSL}
  void main() {
    float clampScale = screenSizeScale(iScale, uCamera.z, iScreenPx);
    vec2 corner = aPos * iScale * clampScale;
    vec2 center = iPos;
    float rot = iRot;
    ${VERTEX_BODY}
  }
`

/** The split layout: one instance buffer holding the Pose2D record, a
 * second holding the style record (iRenderOrder is layout-only - see the
 * header note). */
export const INSTANCE_LAYOUTS_SPLIT: VertexAttribute[][] = [
  [
    { name: "iPos", format: "float32x2" },
    { name: "iRot", format: "float32" },
    { name: "iScale", format: "float32x2" },
  ],
  [
    { name: "iUv", format: "float32x4" },
    { name: "iTint", format: "float32x4" },
    { name: "iRenderOrder", format: "float32" },
    { name: "iScreenPx", format: "float32x2" },
    { name: "iAtlas", format: "float32" },
    { name: "iOutline", format: "float32x4" },
  ],
]

/** What a layer's targets draw with: the unit quad every instance reuses
 * and the pipeline over one of the two layouts above. */
export type SpritePipeline = { quad: BufferId; pipeline: RenderPipelineId; dispose(): void }

/**
 * Compile one sprite pipeline: `vertex` with its matching attribute list
 * (VERTEX + INSTANCE_ATTRIBUTES, or the split pair) over the fragment
 * stage generated for `atlases` (count and distance-field decodes),
 * triangle strips blended with `blend` (the layer's `blend` option,
 * "alpha" by default). Spelled out (not the fused createPipelineTexture)
 * so a layer's targets - its views, a tile layer's chunks - share the one
 * compile. The program lives as long as the pipeline; `dispose` frees
 * both and the quad.
 */
export function createSpritePipeline(label: string, vertex: string, instanceLayouts: VertexAttribute[][], blend: BlendMode, atlases: readonly AtlasShading[]): SpritePipeline {
  // One unit quad (triangle strip), reused by every instance.
  let quad = createBuffer(new Float32Array([-0.5, -0.5, 0.5, -0.5, -0.5, 0.5, 0.5, 0.5]), {
    label: `${label}-quad`,
    autoFree: false,
  })
  let vs = compileShader("vertex", vertex, { header: true })
  let fs = compileShader("fragment", fragmentFor(atlases), { header: true })
  let program = linkProgram(vs, fs, { label })
  destroyShader(vs)
  destroyShader(fs)
  let pipeline = createRenderPipeline(program, {
    label,
    topology: "triangle-strip",
    buffers: [{ attributes: [{ name: "aPos", format: "float32x2" }] }, ...instanceLayouts.map(attributes => ({ stepMode: "instance" as const, attributes }))],
    blend,
  })
  return {
    quad,
    pipeline,
    dispose() {
      destroyRenderPipeline(pipeline)
      destroyProgram(program)
      destroyBuffer(quad)
    },
  }
}
