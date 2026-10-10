// The sprite GLSL and the instance layouts: what a layer's material
// compiles over, and what an app composes its own stages from
// (`@solidrt/2d/glsl`, the twin of `@solidrt/3d/glsl`).
//
// The VERTEX side is one stage for every layer kind: a unit quad
// instanced over the layer's records, mapped world -> clip through
// uCamera (offset + zoom), uCameraRot ([cos, sin, pivotX, pivotY]: the
// camera rotation about the pivot, identity [1, 0, 0, 0]) and uViewport -
// the projectCamera mapping (camera.ts) spelled in GLSL; the two must
// agree. World and clip space are both y-down (core gpu.ts pixel
// contract), so the mapping carries NO flip anywhere - do not add one.
// The stage reads its attributes BY NAME from whichever instance buffers
// the layer binds, which is why the two record layouts below share
// names: the records layer and the tile layer bind the 20-float
// RECORD_ATTRIBUTES, the node layer binds the core-written
// POSE_ATTRIBUTES (slot 0, the Pose2D record [x, y, angle, sx, sy]) and
// the JS-written SPRITE_ATTRIBUTES (slot 1); a material's own instance
// buffers follow. iRenderOrder is read by no stage - it exists for the
// core's instanceOrder (orderBy: "renderOrder"); a declared attribute
// without an active program attribute is skipped, its bytes pad the
// stride.
//
// The FRAGMENT side is GENERATED per layer (spriteSource): one sampler
// per declared atlas, the record's flat atlas index picking it - the
// multi-texture batch of PixiJS and Phaser, where Unity and Godot break
// the batch on every texture change - with a distance-field decode in
// the branch of every atlas declared `sdf` (Atlas.sdf). The set fills a
// `Sprite` struct, the contract every look is written against: the
// stock fragment (unlitFragment) is a surface slot over it, a custom
// fragment (shaderMaterialClass) a main of its own over the same set.
// The layer prepends the set to whatever fragment the material carries,
// so a fragment never names a sampler or branches on an atlas itself.
//
// `outline` is rgb plus a width in WORLD pixels; the vertex stage turns
// the width into screen pixels through the camera zoom and the
// screen-size clamp, so an outline scales with its glyph. A colour atlas
// ignores it. The vertex stage applies the screen-size clamp
// (SpriteOptions.minScreenPx/maxScreenPx) through core's screenSizeScale:
// the quad scales uniformly so its smaller axis stays within the bounds
// at the camera's zoom (the pixels per world unit), and picking applies
// the JS twin to the rect it tests. The rotations here (clockwise,
// y-down) and their JS partners must agree: iRot with pointInSprite in
// pick.ts, uCameraRot with projectCamera in camera.ts. The differential
// checks guard the JS side against oracles but NOT against these
// shaders - if you touch one rotation, touch all.
import { glsl, SCREEN_SIZE_GLSL, VERTEX_FORMATS } from "@solidrt/core/gpu"
import type { TextureBindings, TextureId, VertexAttribute, VertexFormat } from "@solidrt/core/gpu"

/** What the fragment set reads of a declared atlas: whether it is a
 * distance field, and its range (see `Atlas.sdf`). An Atlas record is
 * one. */
export type AtlasShading = { sdf?: { range: number } }

/** The core-written pose record of the node layer (slot 0): the Pose2D
 * projection [x, y, angle, sx, sy]. */
export const POSE_ATTRIBUTES: VertexAttribute[] = [
  { name: "iPos", format: "float32x2" },
  { name: "iRot", format: "float32" },
  { name: "iScale", format: "float32x2" },
]

/** The JS-written sprite record of the node layer (slot 1, 16 floats):
 * [u0, v0, u1, v1, tintR, tintG, tintB, tintA, renderOrder, minScreenPx,
 * maxScreenPx, atlas, outlineR, outlineG, outlineB, outlineWidth]. */
export const SPRITE_ATTRIBUTES: VertexAttribute[] = [
  { name: "iUv", format: "float32x4" },
  { name: "iTint", format: "float32x4" },
  { name: "iRenderOrder", format: "float32" },
  { name: "iScreenPx", format: "float32x2" },
  { name: "iAtlas", format: "float32" },
  { name: "iOutline", format: "float32x4" },
]

/** The records layer's and the tile layer's one record (20 floats):
 * [cx, cy, w, h, u0, v0, u1, v1, rot, tintR, tintG, tintB, tintA,
 * minScreenPx, maxScreenPx, atlas, outlineR, outlineG, outlineB,
 * outlineWidth]. */
export const RECORD_ATTRIBUTES: VertexAttribute[] = [
  { name: "iPos", format: "float32x2" },
  { name: "iScale", format: "float32x2" },
  { name: "iUv", format: "float32x4" },
  { name: "iRot", format: "float32" },
  { name: "iTint", format: "float32x4" },
  { name: "iScreenPx", format: "float32x2" },
  { name: "iAtlas", format: "float32" },
  { name: "iOutline", format: "float32x4" },
]

/** The attribute names the layers' own records and the unit quad take:
 * what a material's instance buffers may not reuse. */
export const LAYER_ATTRIBUTE_NAMES: readonly string[] = ["aPos", ...[...POSE_ATTRIBUTES, ...SPRITE_ATTRIBUTES, ...RECORD_ATTRIBUTES].map(a => a.name)]

/** The uniforms every 2d vertex stage must declare and use: the camera
 * mapping is theirs (see SPRITE_VERTEX_BODY), and a stage without them
 * cannot place a sprite. shaderMaterialClass checks for them. */
export const VERTEX_UNIFORMS: readonly string[] = ["uCamera", "uCameraRot", "uViewport"]

/**
 * The varyings the fragment set reads and the uniforms the mapping
 * needs: what a custom vertex stage declares beside its own `in`s.
 * vUv is the sample uv, vFrame the frame's UV rect (flat), vTint the
 * sprite's tint, vAtlas the sampler index (flat), vOutlineColor and
 * vOutlinePx the outline a distance-field atlas draws (flat; write
 * zeros when the stage has no outline).
 */
export const SPRITE_VARYINGS = glsl`
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

/**
 * The world -> clip mapping and the varying writes, the tail of every
 * vertex stage: expects `corner` (the quad corner in world pixels,
 * after the screen-size clamp), `center` (the sprite's world position),
 * `rot` (its rotation) and `clampScale` (the clamp's scale, 1 without
 * one) as locals, and the record's iUv, iTint, iAtlas and iOutline as
 * `in`s. Compose it after your own placement.
 */
export const SPRITE_VERTEX_BODY = glsl`
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

/** The GLSL type a vertex format feeds (its kind and component count):
 * `float`/`vec2`/`vec3`/`vec4` for the float and normalized formats,
 * `uint`/`uvec*` for uint*, `int`/`ivec*` for sint*. */
export function attributeType(format: VertexFormat): string {
  let codec = VERTEX_FORMATS[format]
  let prefix = codec.kind === "float" ? "" : codec.kind === "uint" ? "u" : "i"
  if (codec.components === 1) return codec.kind === "float" ? "float" : codec.kind === "uint" ? "uint" : "int"
  return `${prefix}vec${codec.components}`
}

/** The varying the stock vertex stage forwards an instance attribute
 * as: `iFoo` becomes `vFoo` (flat, the record's exact value). An
 * attribute not named `i` plus a capital has no forwarded name and
 * throws - the layer's own records follow the same convention. */
export function forwardedName(name: string): string {
  if (!/^i[A-Z]/.test(name)) throw new Error(`an instance attribute the stock stage forwards is named iSomething (forwarded as vSomething), got '${name}'`)
  return "v" + name.slice(1)
}

/**
 * The stock vertex stage, for every layer kind: the record's pose
 * (iPos, iRot, iScale - the core's pose record or the records layer's
 * fields, by name) placing the unit quad under the screen-size clamp,
 * then SPRITE_VERTEX_BODY. `forwards` are a material's own instance
 * attributes (its instanceBuffers'), each declared as an `in` and
 * forwarded unchanged as a flat varying (forwardedName: `iPalette` ->
 * `vPalette`), which is how the stock material's `surface` reads a
 * per-sprite style record. The tier-1 custom look pairs its own stage
 * with unlitFragment(); a wobble, a wind sway or a billboard-style
 * effect is this stage with its placement changed.
 */
export function unlitVertex(forwards: readonly VertexAttribute[] = []): string {
  let ins = ""
  let outs = ""
  let writes = ""
  for (let attr of forwards) {
    let type = attributeType(attr.format)
    let varying = forwardedName(attr.name)
    ins += `in ${type} ${attr.name};\n  `
    outs += `flat out ${type} ${varying};\n  `
    writes += `${varying} = ${attr.name};\n    `
  }
  return glsl`
  in vec2 aPos;
  in vec2 iPos;
  in float iRot;
  in vec2 iScale;
  in vec4 iUv;
  in vec4 iTint;
  in vec2 iScreenPx;
  in float iAtlas;
  in vec4 iOutline;
  ${ins}${outs}${SPRITE_VARYINGS}
  ${SCREEN_SIZE_GLSL}
  void main() {
    float clampScale = screenSizeScale(iScale, uCamera.z, iScreenPx);
    vec2 corner = aPos * iScale * clampScale;
    vec2 center = iPos;
    float rot = iRot;
    ${writes}${SPRITE_VERTEX_BODY}
  }
`
}

/** The stock vertex stage with nothing forwarded: unlitVertex(). */
export const SPRITE_VERTEX = unlitVertex()

/** The sampler uniform of atlas `i` in a layer's program. */
export function atlasUniform(i: number): string {
  return `uAtlas${i}`
}

/** The texture bindings of a layer's atlases, by sampler uniform: what
 * every target drawing with the layer's pipeline binds (a material's own
 * textures merge over them). */
export function atlasBindings(atlases: readonly TextureId[]): TextureBindings {
  let out: TextureBindings = {}
  atlases.forEach((texture, i) => (out[atlasUniform(i)] = texture))
  return out
}

/**
 * The fragment SET of a layer over its atlases - what the layer prepends
 * to its material's fragment, so every look is written over it:
 *
 * - the varyings (SPRITE_VARYINGS' `in` side), one `uniform sampler2D
 *   uAtlasN` per atlas, and `uniform vec4 uTint`, the LAYER tint
 *   (multiplied over the per-instance vTint by the stock fragment;
 *   identity [1, 1, 1, 1] - GLSL uniforms default to ZERO, so every
 *   target creation pins it explicitly or everything renders
 *   transparent black);
 * - `struct Sprite { vec4 color; vec2 uv; vec4 frame; vec4 tint; int
 *   atlas; }`: the contract. `color` is the stock result for the
 *   fragment, premultiplied, before the layer tint: the atlas texel
 *   times the instance tint for a colour atlas, the decoded field (the
 *   tint as the fill, the outline under it) for a distance-field one;
 *   `uv` the sample uv, `frame` the frame's UV rect, `tint` the
 *   instance tint, `atlas` the sampler index;
 * - `Sprite spriteOf()`: fills the struct for this fragment. Takes the
 *   uv derivatives first, in main's uniform control flow, so the mip
 *   footprint is the quad's own and the lookup inside the per-instance
 *   branch is well defined (an implicit-derivative texture() in
 *   non-uniform control flow is not); a custom main calls it first;
 * - `vec4 spriteSample(vec2 uv)`: the stock result at another uv of the
 *   fragment's own atlas - the pick, the frame clamp, the field decode,
 *   the tint - for scrolling and distortion. The clamp into the frame
 *   still applies, so a wrap is `fract` in frame space:
 *   `spriteSample(mix(s.frame.xy, s.frame.zw, fract(t)))`.
 *
 * The sample is clamped to its frame, pulled in by half a texel of ITS
 * atlas: a quad at a fractional position lands edge samples so close to
 * the frame's edge that a linear tap (and float interpolation under
 * nearest) reaches the cell next door, painting a one-texel line of the
 * neighbour along the sprite's edge. The clamp keeps the quad's texel
 * mapping 1:1 and only moves those edge samples onto the edge texel, so
 * touching cells do not bleed and no gutter is needed at mip level 0.
 * It cannot help a mip chain, whose texels straddle the cell edge before
 * sampling - that is extrude.ts. min/max because a flipped frame stores
 * its UVs swapped, and the inner min/max because a sub-texel frame would
 * otherwise hand clamp() a lower bound above its upper one. The tap is
 * textureGrad over the derivatives spriteOf took, so a clamped uv (a
 * zero derivative along the clamped axis, which would pick a sharper
 * level along every edge) never decides the level.
 *
 * A distance-field atlas yields the instance tint as the fill,
 * anti-aliased over one screen pixel: the field's range mapped through
 * the quad's texel footprint gives the screen pixels per field unit
 * (floored at 1, so a glyph far below its texel size fades instead of
 * vanishing), the median of rgb is the edge, and when the instance
 * carries an outline the true field in alpha, pushed out by the
 * outline's screen pixels, paints the outline colour under the fill.
 */
export function spriteSource(atlases: readonly AtlasShading[]): string {
  if (!Array.isArray(atlases) || atlases.length === 0) throw new Error(`spriteSource: atlases must be a non-empty list, got ${JSON.stringify(atlases)}`)
  let samplers = ""
  atlases.forEach((_, i) => (samplers += `uniform sampler2D ${atlasUniform(i)};\n  `))
  let shade = (i: number) => {
    let sdf = atlases[i]!.sdf
    if (sdf === undefined) return `spriteTap(${atlasUniform(i)}, uv) * vTint`
    if (!(typeof sdf.range === "number" && sdf.range > 0 && Number.isFinite(sdf.range))) {
      throw new Error(`spriteSource: atlases[${i}].sdf.range must be a positive number, got ${sdf.range}`)
    }
    return `spriteField(${atlasUniform(i)}, ${sdf.range.toFixed(2)}, uv)`
  }
  let cases = ""
  atlases.forEach((_, i) => (cases += `case ${i}: return ${shade(i)};\n      `))
  let pick =
    atlases.length === 1
      ? `return ${shade(0)};`
      : `switch (vAtlas) {
      ${cases}default: return ${shade(0)};
    }`
  return glsl`
  in vec2 vUv;
  flat in vec4 vFrame;
  in vec4 vTint;
  flat in int vAtlas;
  flat in vec3 vOutlineColor;
  flat in float vOutlinePx;
  ${samplers}uniform vec4 uTint;

  struct Sprite {
    vec4 color;
    vec2 uv;
    vec4 frame;
    vec4 tint;
    int atlas;
  };

  // The quad's uv footprint per screen pixel, taken once by spriteOf in
  // uniform control flow; every tap of this fragment uses it.
  vec2 spriteDx;
  vec2 spriteDy;

  vec4 spriteTap(sampler2D atlas, vec2 uv) {
    vec2 halfTexel = 0.5 / vec2(textureSize(atlas, 0));
    vec2 lo = min(vFrame.xy, vFrame.zw) + halfTexel;
    vec2 hi = max(vFrame.xy, vFrame.zw) - halfTexel;
    return textureGrad(atlas, clamp(uv, min(lo, hi), max(lo, hi)), spriteDx, spriteDy);
  }

  float spriteMedian3(vec3 v) {
    return max(min(v.r, v.g), min(max(v.r, v.g), v.b));
  }

  // Screen pixels per field unit (0..1 spans the range): the range in
  // texels over the quad's texel footprint per screen pixel, floored at
  // one so a minified glyph fades rather than aliasing away.
  float spriteScreenPxRange(sampler2D atlas, float range) {
    vec2 unitRange = vec2(range) / vec2(textureSize(atlas, 0));
    vec2 screenTexSize = vec2(1.0) / (abs(spriteDx) + abs(spriteDy));
    return max(0.5 * dot(unitRange, screenTexSize), 1.0);
  }

  vec4 spriteField(sampler2D atlas, float range, vec2 uv) {
    vec4 t = spriteTap(atlas, uv);
    float px = spriteScreenPxRange(atlas, range);
    float fill = clamp((spriteMedian3(t.rgb) - 0.5) * px + 0.5, 0.0, 1.0);
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

  vec4 spriteSample(vec2 uv) {
    ${pick}
  }

  Sprite spriteOf() {
    spriteDx = dFdx(vUv);
    spriteDy = dFdy(vUv);
    return Sprite(spriteSample(vUv), vUv, vFrame, vTint, vAtlas);
  }
`
}

/** The options of the stock fragment: the tier-2 slot. */
export type UnlitSourceOptions = {
  /** GLSL at file scope, after the layer's set and before main:
   * uniforms (each a layer param - `params` on the layer or
   * `layer.setParams` - or a material texture) and helper functions. */
  prelude?: string
  /** GLSL declaring `void surface(inout Sprite s)`, called with the
   * struct filled (spriteOf) and before the layer tint: rewrite
   * `s.color` or `discard`. */
  surface?: string
  /** The instance attributes the stock vertex stage forwards
   * (unlitVertex's `forwards`): declared here as the matching flat
   * varyings, so the prelude and the surface read `vFoo` for a record's
   * `iFoo`. */
  forwards?: readonly VertexAttribute[]
}

/**
 * The stock fragment over the layer's set (spriteSource, which the layer
 * prepends): the sprite's stock colour, through the surface slot when
 * one is given, times the layer tint. `unlit()` compiles exactly this;
 * the tier-1 custom look pairs it with a vertex stage of its own.
 */
export function unlitFragment(o: UnlitSourceOptions = {}): string {
  let ins = ""
  for (let attr of o.forwards ?? []) ins += `flat in ${attributeType(attr.format)} ${forwardedName(attr.name)};\n  `
  return glsl`
  ${ins}${o.prelude ?? ""}
  ${o.surface ?? ""}
  void main() {
    Sprite s = spriteOf();
    ${o.surface ? "surface(s);" : ""}
    fragColor = s.color * uTint;
  }
`
}
