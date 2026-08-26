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
 * `float shadow(sampler2D map, vec4 coord, float bias)` - the directional
 * shadow factor (1 lit, 0 shadowed) for a world point carried into the
 * light's clip space by the scene's `uShadowMatrix`:
 * `shadow(uShadowMap, uShadowMatrix * vec4(vWorldPos, 1.0), uShadowBias)`.
 * Perspective divide, 0..1 remap, out-of-frustum returns 1 (lit), then a
 * 3x3 PCF over texel neighbours comparing the map's `.r` (a stage-1 depth
 * texture samples nearest, so the softness is this loop, not the sampler).
 * `bias` is subtracted from the point's depth against acne; the receiving
 * `lit` variant also offsets the point along its normal by
 * `uShadowNormalBias` before the transform.
 */
export const SHADOW = glsl`
  float shadow(sampler2D map, vec4 coord, float bias) {
    vec3 p = coord.xyz / coord.w * 0.5 + 0.5;
    if (p.x < 0.0 || p.x > 1.0 || p.y < 0.0 || p.y > 1.0 || p.z > 1.0) return 1.0;
    vec2 texel = 1.0 / vec2(textureSize(map, 0));
    float lit = 0.0;
    for (int y = -1; y <= 1; y++) {
      for (int x = -1; x <= 1; x++) {
        float d = texture(map, p.xy + vec2(float(x), float(y)) * texel).r;
        lit += p.z - bias <= d ? 1.0 : 0.0;
      }
    }
    return lit / 9.0;
  }
`
