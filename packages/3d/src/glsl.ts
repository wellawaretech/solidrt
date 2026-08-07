// The exported lighting GLSL: string constants an app composes into its
// own shaderMaterial sources with plain template literals - the same
// pieces the package's future lit materials will be built from, so a
// custom material never becomes second-class (no preprocessor, no include
// resolver; the policy is argued in okf/research/3d-differentiators.md).
//
// The light-model functions are PURE: normals, view vectors, light
// directions, colors and exponents all arrive as arguments, so these
// constants pin nothing but function names. LIT_VERTEX is the one
// deliberate exception - it pins the standard varying interface
// (vWorldPos, vNormal, vUv) and consumes the standard uniform set
// (uModel, uViewProj, uNormal); fragments written against those five
// names compose with it directly. All directions are expected normalized;
// every function returns its raw term, weighting and color belong to the
// caller.

import { glsl } from "@solidrt/core/gpu"

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
