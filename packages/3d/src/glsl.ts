// The exported lighting GLSL: string constants an app composes into its
// own shaderMaterial sources with plain template literals - the same
// pieces the package's future lit materials will be built from, so a
// custom material never becomes second-class (no preprocessor, no include
// resolver; the policy is argued in okf/research/3d-differentiators.md).
//
// The light-model functions are PURE: normals, view vectors, light
// directions, colors and exponents all arrive as arguments, so these
// constants pin nothing but function names. The LIT_VERTEX pair are the
// deliberate exception - they pin the standard varying interface
// (vWorldPos, vNormal, vUv, plus vColor on the colored variant) and
// consume the standard uniform set (uModel, uViewProj, uNormal);
// fragments written against those names compose with them directly. All directions are expected normalized;
// every function returns its raw term, weighting and color belong to the
// caller.

import { glsl } from "@solidrt/core/gpu"

/** The directional-light cap of the scene's light list (DirectionalLight nodes)
 * and of the `lit` fragment; a custom fragment declares
 * `uniform vec3 uLightDir[MAX_LIGHTS]` / `uLightColor[MAX_LIGHTS]` and
 * loops to `uLightCount`. A shader-source constant, so it is fixed for
 * the app (see okf/backlog/app-runtime-config.md). */
export const MAX_LIGHTS = 4

/** The most cascades one casting light splits its shadow into
 * (`shadow.cascades`, 1..MAX_CASCADES). */
export const MAX_CASCADES = 4

/** The shadow-map slot count of the scene's shadow set: every casting
 * light owns `shadow.cascades` consecutive slots (one per map), so this
 * bounds `uShadowRect`/`uShadowMatrix`. */
export const MAX_SHADOW_MAPS = MAX_LIGHTS * MAX_CASCADES

/**
 * The standard lit vertex stage: clip position via uViewProj * uModel,
 * with world position, world normal (via `mat3(uNormal)`, correct under
 * non-uniform scale) and UV as varyings:
 *
 *   in vec3 vWorldPos; in vec3 vNormal; in vec2 vUv;
 *
 * Pair it with your own fragment; the view vector there is
 * `normalize(uCamPos - vWorldPos)`.
 */
export const LIT_VERTEX = glsl`
  in vec3 aPos;
  in vec3 aNormal;
  in vec2 aUV;
  uniform mat4 uModel;
  uniform mat4 uViewProj;
  uniform mat4 uNormal;
  out vec3 vWorldPos;
  out vec3 vNormal;
  out vec2 vUv;

  void main() {
    vec4 world = uModel * vec4(aPos, 1.0);
    gl_Position = uViewProj * world;
    vWorldPos = world.xyz;
    vNormal = mat3(uNormal) * aNormal;
    vUv = aUV;
  }
`

/**
 * LIT_VERTEX for "colored"-layout geometry: the same interface plus the
 * per-vertex aColor vec4 forwarded raw as `in vec4 vColor` - what it means
 * (a tint, baked AO in one channel, anything) is the fragment's business.
 * Using this constant makes the material read aColor (shaderMaterial
 * collects the vertex stage's `in` declarations), so its meshes need
 * geometry carrying that channel - withColors() - or add() throws.
 */
export const LIT_VERTEX_COLORED = glsl`
  in vec3 aPos;
  in vec3 aNormal;
  in vec2 aUV;
  in vec4 aColor;
  uniform mat4 uModel;
  uniform mat4 uViewProj;
  uniform mat4 uNormal;
  out vec3 vWorldPos;
  out vec3 vNormal;
  out vec2 vUv;
  out vec4 vColor;

  void main() {
    vec4 world = uModel * vec4(aPos, 1.0);
    gl_Position = uViewProj * world;
    vWorldPos = world.xyz;
    vNormal = mat3(uNormal) * aNormal;
    vUv = aUV;
    vColor = aColor;
  }
`

/** `vec3 hemisphere(vec3 n, vec3 sky, vec3 ground)` - ambient from a
 * sky/ground gradient by the normal's vertical tilt: sky straight up,
 * ground bounce straight down. */
export const HEMISPHERE = glsl`
  vec3 hemisphere(vec3 n, vec3 sky, vec3 ground) {
    return mix(ground, sky, n.y * 0.5 + 0.5);
  }
`

/** `float lambert(vec3 n, vec3 l)` - the diffuse term for a directional
 * light pointing TOWARD the light (multiply by your light color). */
export const LAMBERT = glsl`
  float lambert(vec3 n, vec3 l) {
    return max(dot(n, l), 0.0);
  }
`

/** `float blinnSpecular(vec3 n, vec3 v, vec3 l, float shininess)` - the
 * Blinn-Phong highlight from the half vector between view and light;
 * shininess runs from wide matte sheen (~8) to tight mirror dot (~150). */
export const BLINN_SPECULAR = glsl`
  float blinnSpecular(vec3 n, vec3 v, vec3 l, float shininess) {
    return pow(max(dot(n, normalize(l + v)), 0.0), shininess);
  }
`

/** `float fresnel(vec3 n, vec3 v, float power)` - the grazing-angle rim
 * weight (1 at silhouettes, 0 face-on); typical power 3 to 5. */
export const FRESNEL = glsl`
  float fresnel(vec3 n, vec3 v, float power) {
    return pow(1.0 - max(dot(n, v), 0.0), power);
  }
`

/**
 * The scene's shadow set as a receiving program declares it: ONE
 * `uShadowAtlas` (every casting light's depth map is a tile of it, so N
 * maps render as one pass; a white texel when nothing casts), a MAP slot
 * set of `MAX_SHADOW_MAPS` - `uShadowRect[M]` (map slot j's tile as x, y,
 * width, height in atlas 0..1 UV) and `uShadowMatrix[M]` (its light-space
 * viewProj) - and, per directional light index, `uShadowFirst[N]` /
 * `uShadowCount[N]` (light i's maps are slots `first .. first + count - 1`;
 * count 0 = it does not cast; a box light has one map, a cascaded light
 * `shadow.cascades` of them, tightest first), `uShadowBias[N]` and
 * `uShadowNormalBias[N]`. The scene binds and writes all of it on every
 * target a receiving material can draw into; a custom material composes
 * this, then SHADOW, then SHADOW_LOOKUP (in that order) and multiplies
 * light i's term by `lightShadow(i, worldPos, n)` - `lit` is the shape. A
 * material that does not receive composes none of it, so it declares no
 * sampler for nothing.
 */
export const SHADOW_SLOTS = glsl`
  uniform sampler2D uShadowAtlas;
  uniform vec4 uShadowRect[${MAX_SHADOW_MAPS}];
  uniform mat4 uShadowMatrix[${MAX_SHADOW_MAPS}];
  uniform int uShadowFirst[${MAX_LIGHTS}];
  uniform int uShadowCount[${MAX_LIGHTS}];
  uniform float uShadowBias[${MAX_LIGHTS}];
  uniform float uShadowNormalBias[${MAX_LIGHTS}];
`

/**
 * The shadow lookup in three steps plus their composition, one map at a
 * time. `vec3 shadowPoint(vec4 coord)` takes a world point carried into a
 * casting light's clip space by that map's `uShadowMatrix[j]` to its map
 * point: xy in 0..1 across the map, z the depth to compare. `bool
 * shadowInside(vec3 p)` is whether the map has it at all (xy in 0..1, z
 * not past the far plane) - the cascade select. `float shadowSample(
 * sampler2D map, vec4 rect, vec3 p, float bias)` is the factor (1 lit, 0
 * shadowed) of a point the map has: a 3x3 PCF over texel neighbours in
 * the map's tile `rect` (x, y, width, height in `map`'s 0..1 UV;
 * `vec4(0, 0, 1, 1)` is a whole map) comparing the map's `.r` (a stage-1
 * depth texture samples nearest, so the softness is this loop, not the
 * sampler); every tap is clamped to the tile inset by half a texel, so
 * no tap reads a neighbouring map's tile; `bias` is subtracted from the
 * point's depth against acne. `float shadow(sampler2D map, vec4 rect,
 * vec4 coord, float bias)` composes the three: 1 (lit) outside the map,
 * else the sample -
 * `shadow(uShadowAtlas, uShadowRect[0], uShadowMatrix[0] * vec4(vWorldPos, 1.0), uShadowBias[0])`.
 * SHADOW_LOOKUP uses the steps, so it projects each map once.
 */
export const SHADOW = glsl`
  vec3 shadowPoint(vec4 coord) {
    return coord.xyz / coord.w * 0.5 + 0.5;
  }

  bool shadowInside(vec3 p) {
    return all(greaterThanEqual(p.xy, vec2(0.0))) && all(lessThanEqual(p, vec3(1.0)));
  }

  float shadowSample(sampler2D map, vec4 rect, vec3 p, float bias) {
    vec2 texel = 1.0 / vec2(textureSize(map, 0));
    vec2 lo = rect.xy + 0.5 * texel;
    vec2 hi = rect.xy + rect.zw - 0.5 * texel;
    vec2 base = rect.xy + p.xy * rect.zw;
    float lit = 0.0;
    for (int y = -1; y <= 1; y++) {
      for (int x = -1; x <= 1; x++) {
        float d = texture(map, clamp(base + vec2(float(x), float(y)) * texel, lo, hi)).r;
        lit += p.z - bias <= d ? 1.0 : 0.0;
      }
    }
    return lit / 9.0;
  }

  float shadow(sampler2D map, vec4 rect, vec4 coord, float bias) {
    vec3 p = shadowPoint(coord);
    return shadowInside(p) ? shadowSample(map, rect, p, bias) : 1.0;
  }
`

/**
 * The step from a light index to its shadow factor, over SHADOW_SLOTS and
 * SHADOW (compose both first). `float lightShadow(int i, vec3 worldPos,
 * vec3 n)` is the one to call per light: 1 for a light that does not
 * cast, else the factor for `worldPos` pushed along its normal `n` by
 * `uShadowNormalBias[i]` (the acne knob to reach for first), looked up in
 * the FIRST of light i's maps that has the point (a box light has one; a
 * cascaded light's maps come tightest first, so the sharpest cascade
 * that has the point wins and a point past the last is lit) with that
 * map's `uShadowMatrix[j]`, its tile and `uShadowBias[i]`. Inside the
 * outer SHADOW_BLEND of a map (in map 0..1 units, so 0.1 is its outer
 * 10% on each side) the factor fades into the next cascade's, so the
 * hand-over is a band and not a seam; the last map, a box light's only
 * one, and any rim the next cascade does not reach (the near side, at
 * the camera's feet) have no band. Position and normal are arguments, so
 * no varying name is pinned and a custom vertex stage composes freely.
 */
export const SHADOW_LOOKUP = glsl`
  const float SHADOW_BLEND = 0.1;

  float lightShadow(int i, vec3 worldPos, vec3 n) {
    int count = uShadowCount[i];
    if (count == 0) return 1.0;
    vec4 w = vec4(worldPos + n * uShadowNormalBias[i], 1.0);
    float bias = uShadowBias[i];
    int first = uShadowFirst[i];
    int last = first + count - 1;
    for (int j = first; j <= last; j++) {
      vec3 p = shadowPoint(uShadowMatrix[j] * w);
      if (!shadowInside(p)) continue;
      float s = shadowSample(uShadowAtlas, uShadowRect[j], p, bias);
      if (j == last) return s;
      float edge = min(min(p.x, 1.0 - p.x), min(p.y, 1.0 - p.y));
      if (edge >= SHADOW_BLEND) return s;
      vec3 q = shadowPoint(uShadowMatrix[j + 1] * w);
      if (!shadowInside(q)) return s;
      return mix(shadowSample(uShadowAtlas, uShadowRect[j + 1], q, bias), s, edge / SHADOW_BLEND);
    }
    return 1.0;
  }
`
