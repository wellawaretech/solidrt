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

import type { CullMode } from "@solidrt/core/gpu"

// core/gpu's glsl tag, aliased locally (it is String.raw) so this module
// stays runtime-pure: no flux:gpu import, so the checks/ rigs and any
// bake-time tool can run it headless on bare flux.
let glsl = String.raw

/** The cap of the scene's light list (directional, spot and point nodes
 * alike; the hemisphere ambient is not in it) and of the `lit` fragment;
 * a custom fragment composes LIGHT_SLOTS and LIGHT_LOOKUP and loops to
 * `uLightCount`. A shader-source constant, so it is fixed for the app
 * (see okf/backlog/app-runtime-config.md). */
export const MAX_LIGHTS = 8

/** The most cascades one casting light splits its shadow into
 * (`shadow.cascades`, 1..MAX_CASCADES). */
export const MAX_CASCADES = 4

/** The shadow-map slot budget of the scene's shadow set: every casting
 * light claims consecutive slots, one per map (a directional light
 * `shadow.cascades`, a point light its six faces, a spot one), dealt in
 * light order, and a caster past the
 * budget throws at attach. Bounds `uShadowRect`/`uShadowMatrix` - its
 * own constant, NOT MAX_LIGHTS * MAX_CASCADES, so raising the light cap
 * does not size the fragment uniform budget for the worst case where
 * every light is a fully cascaded sun. */
export const MAX_SHADOW_MAPS = 8

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
/** The skinned-layout declarations a vertex stage splices in: the
 * aJoints/aWeights channels, the `uBones` palette and `mat4 boneAt(int)`.
 * The palette is a float texture, not a uniform array, so rig size is
 * bounded only by texture height (>= 2048 everywhere), never by the
 * vertex uniform budget: an rgba32f texture 4 texels wide, one row per
 * joint, each row the four columns of that joint's mat4 (the spatial
 * flush writes it from the bound joint nodes; createModel binds it per
 * skinned mesh). Pair with
 * SKIN_MATRIX in main(). */
export const SKIN_DECLS = glsl`
  in vec4 aJoints;
  in vec4 aWeights;
  uniform sampler2D uBones;
  mat4 boneAt(int j) {
    return mat4(texelFetch(uBones, ivec2(0, j), 0), texelFetch(uBones, ivec2(1, j), 0),
      texelFetch(uBones, ivec2(2, j), 0), texelFetch(uBones, ivec2(3, j), 0));
  }
`
/** The linear-blend skin matrix, spliced into main() after SKIN_DECLS:
 * apply as `skin * vec4(aPos, 1.0)` before uModel. Bone-space normals go
 * through `mat3(skin)` - exact for rigid bones, the standard
 * approximation under bone scale. */
export const SKIN_MATRIX = glsl`
    mat4 skin = aWeights.x * boneAt(int(aJoints.x)) + aWeights.y * boneAt(int(aJoints.y)) +
      aWeights.z * boneAt(int(aJoints.z)) + aWeights.w * boneAt(int(aJoints.w));
`

// The one lit vertex template: the standard prefix always, aColor and
// aUV2 only when the fragment reads them (an `in` the source mentions
// makes the material require that channel, so the blocks must be absent,
// not inactive), the skin blocks only for a skinned material (they make
// it require the "skinned" layout the same way). LIT_VERTEX /
// LIT_VERTEX_COLORED are its two named forms; litVertex(o) picks per
// option set.
function litVertexSource(colored: boolean, uv2: boolean, skinned: boolean): string {
  return glsl`
  in vec3 aPos;
  in vec3 aNormal;
  in vec2 aUV;
  ${colored ? "in vec4 aColor;" : ""}
  ${uv2 ? "in vec2 aUV2;" : ""}
  ${skinned ? SKIN_DECLS : ""}
  uniform mat4 uModel;
  uniform mat4 uViewProj;
  uniform mat4 uNormal;
  out vec3 vWorldPos;
  out vec3 vNormal;
  out vec2 vUv;
  ${colored ? "out vec4 vColor;" : ""}
  ${uv2 ? "out vec2 vUv2;" : ""}

  void main() {
    ${skinned ? SKIN_MATRIX : ""}
    vec4 world = uModel * ${skinned ? "(skin * vec4(aPos, 1.0))" : "vec4(aPos, 1.0)"};
    gl_Position = uViewProj * world;
    vWorldPos = world.xyz;
    vNormal = mat3(uNormal) * ${skinned ? "(mat3(skin) * aNormal)" : "aNormal"};
    vUv = aUV;
    ${colored ? "vColor = aColor;" : ""}
    ${uv2 ? "vUv2 = aUV2;" : ""}
  }
`
}

export const LIT_VERTEX = litVertexSource(false, false, false)

/**
 * LIT_VERTEX for "colored"-layout geometry: the same interface plus the
 * per-vertex aColor vec4 forwarded raw as `in vec4 vColor` - what it means
 * (a tint, baked AO in one channel, anything) is the fragment's business.
 * Using this constant makes the material read aColor (shaderMaterial
 * collects the vertex stage's `in` declarations), so its meshes need
 * geometry carrying that channel - withColors() - or add() throws.
 */
export const LIT_VERTEX_COLORED = litVertexSource(true, false, false)

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
 * The metalness/roughness light model `standard` shades with (Three's
 * MeshStandardMaterial, Godot's StandardMaterial3D, Unity's Standard).
 * `vec3 ggxSpecular(n, v, l, f0, roughness)` is one light's specular
 * lobe - GGX distribution x height-correlated Smith visibility x Schlick
 * fresnel, times pi - for the caller to weight by `lambert(n, l)` and
 * the light color, the same convention the diffuse uses (a light of
 * intensity 1 lights a white diffuse surface to 1 face-on; Three's
 * lights are the same lobe with intensities a factor pi larger).
 * `roughness` is perceptual: alpha is its square, floored at
 * MIN_ROUGHNESS so a mirror keeps a lobe of finite width. `vec2
 * envBrdf(nv, roughness)` is the split-sum environment BRDF - the scale
 * and bias on f0 (`f0 * ab.x + ab.y`) that turns prefiltered radiance
 * into the specular image term - as Lazarov's analytic fit (Three's
 * DFGApprox, Godot's brdf_approx; no lookup texture). `DIELECTRIC_F0` is
 * a non-metal's face-on reflectance (0.04 in all three engines); a
 * metal's f0 is its base color. The pieces (`ggxDistribution(nh, a2)`,
 * `smithVisibility(nl, nv, a2)`, `schlickFresnel(f0, vh)`) are there for
 * a custom lobe. Defines PBR_PI, not PI, so a prelude's own PI stands.
 */
export const PBR = glsl`
  const float PBR_PI = 3.14159265;
  // Face-on reflectance of a dielectric (index of refraction about 1.5).
  const float DIELECTRIC_F0 = 0.04;
  // Floor on the perceptual roughness: alpha never reaches zero, so a
  // perfect mirror keeps a highlight of finite width (Three's clamp).
  const float MIN_ROUGHNESS = 0.0525;
  // Floor on the visibility denominator at grazing angles.
  const float VISIBILITY_MIN = 1e-5;
  // Schlick's approximation exponent.
  const float SCHLICK_POWER = 5.0;
  float ggxDistribution(float nh, float a2) {
    float d = nh * nh * (a2 - 1.0) + 1.0;
    return a2 / (PBR_PI * d * d);
  }
  float smithVisibility(float nl, float nv, float a2) {
    float gv = nl * sqrt(nv * nv * (1.0 - a2) + a2);
    float gl = nv * sqrt(nl * nl * (1.0 - a2) + a2);
    return 0.5 / max(gv + gl, VISIBILITY_MIN);
  }
  vec3 schlickFresnel(vec3 f0, float vh) {
    return f0 + (1.0 - f0) * pow(1.0 - vh, SCHLICK_POWER);
  }
  vec3 ggxSpecular(vec3 n, vec3 v, vec3 l, vec3 f0, float roughness) {
    vec3 h = normalize(l + v);
    float nl = max(dot(n, l), 0.0);
    float nv = max(dot(n, v), 0.0);
    float nh = max(dot(n, h), 0.0);
    float vh = max(dot(v, h), 0.0);
    float r = max(roughness, MIN_ROUGHNESS);
    float a2 = r * r * r * r;
    return schlickFresnel(f0, vh) * (ggxDistribution(nh, a2) * smithVisibility(nl, nv, a2) * PBR_PI);
  }
  // Lazarov's fit of the environment BRDF integral: the coefficients of
  // the two polynomials in roughness, and the grazing-angle exponent.
  vec2 envBrdf(float nv, float roughness) {
    const vec4 c0 = vec4(-1.0, -0.0275, -0.572, 0.022);
    const vec4 c1 = vec4(1.0, 0.0425, 1.04, -0.04);
    vec4 r = roughness * c0 + c1;
    float a004 = min(r.x * r.x, exp2(-9.28 * nv)) * r.x + r.y;
    return vec2(-1.04, 1.04) * a004 + r.zw;
  }
`

/**
 * The scene's light list as a receiving program declares it, written by
 * the scene (`uLightDir` and `uLightPos` core-driven, so a moving light
 * costs no JS): per light i to `uLightCount`, its `uLightType[i]`
 * (LIGHT_DIRECTIONAL | LIGHT_SPOT | LIGHT_POINT), `uLightDir[i]` (world
 * unit vector TOWARD a directional light; a spot's axis toward the
 * light; unused for a point), `uLightPos[i]` (a spot's or point's world
 * position; unused for a directional), `uLightColor[i]` (rgb, intensity
 * folded in) and `uLightParams[i]` (cosInner, cosOuter, distance, decay
 * - the cone cosines a spot fades between, the falloff cutoff and
 * exponent a spot or point attenuates by; unused for a directional).
 * Compose it before LIGHT_LOOKUP; `lit` is the shape.
 */
export const LIGHT_SLOTS = glsl`
  uniform int uLightCount;
  uniform int uLightType[${MAX_LIGHTS}];
  uniform vec3 uLightDir[${MAX_LIGHTS}];
  uniform vec3 uLightPos[${MAX_LIGHTS}];
  uniform vec3 uLightColor[${MAX_LIGHTS}];
  uniform vec4 uLightParams[${MAX_LIGHTS}];
  const int LIGHT_DIRECTIONAL = 0;
  const int LIGHT_SPOT = 1;
  const int LIGHT_POINT = 2;
`

/**
 * The step from a light index to its incoming vector and strength, over
 * LIGHT_SLOTS (compose it first). `float lightVector(int i, vec3
 * worldPos, out vec3 l)` writes the unit vector from `worldPos` TOWARD
 * light i and returns its attenuation: 1 for a directional light; for a
 * spot or point, the windowed inverse falloff `1 / d^decay` faded to
 * zero at `distance` (0 = no cutoff; Three's punctual-light falloff), a
 * spot's additionally faded across its cone from cosInner to cosOuter.
 * Zero means the light cannot reach the fragment - skip its shadow
 * lookup and its terms (`lit` does exactly that).
 */
export const LIGHT_LOOKUP = glsl`
  // Floors the falloff divisors so a fragment at the light's own
  // position stays finite (Three's punctual-light rule).
  const float FALLOFF_MIN = 0.01;

  float lightVector(int i, vec3 worldPos, out vec3 l) {
    if (uLightType[i] == LIGHT_DIRECTIONAL) {
      l = uLightDir[i];
      return 1.0;
    }
    vec3 dv = uLightPos[i] - worldPos;
    float d = length(dv);
    l = dv / max(d, FALLOFF_MIN);
    vec4 p = uLightParams[i];
    float atten = 1.0 / max(pow(d, p.w), FALLOFF_MIN);
    if (p.z > 0.0) {
      float win = clamp(1.0 - pow(d / p.z, 4.0), 0.0, 1.0);
      atten *= win * win;
    }
    if (uLightType[i] == LIGHT_SPOT) {
      atten *= smoothstep(p.y, p.x, dot(l, uLightDir[i]));
    }
    return atten;
  }
`

/**
 * `vec3 perturbNormal(vec3 n, vec3 worldPos, vec2 uv)` - the surface
 * normal bent by a tangent-space normal map (`uniform sampler2D
 * uNormalMap`, OpenGL-style +Y as glTF mandates, `uniform float
 * uNormalScale` weighting the bend). The tangent frame is built per
 * fragment from screen-space derivatives of worldPos and uv (Three's
 * untangented path, Schuler's cotangent frame), so it works on ANY
 * UV-mapped geometry with no tangent attribute; the trade is mild seams
 * on mirrored UVs, the case a real aTangent layout would fix. Needs no
 * varying of its own - pass the same worldPos and uv the caller lights
 * and samples with. `n` must be normalized and already facing the viewer
 * (flip back faces before calling).
 */
export const NORMAL_MAP = glsl`
  uniform sampler2D uNormalMap;
  uniform float uNormalScale;
  // NORMAL_MAP_EPS floors the tangent frame's magnitude so a face with
  // no UV variation (degenerate derivatives) yields the geometric normal
  // instead of a NaN.
  const float NORMAL_MAP_EPS = 1e-20;
  vec3 perturbNormal(vec3 n, vec3 worldPos, vec2 uv) {
    vec3 mapN = texture(uNormalMap, uv).xyz * 2.0 - 1.0;
    mapN.xy *= uNormalScale;
    vec3 dp1 = dFdx(worldPos);
    vec3 dp2 = dFdy(worldPos);
    vec2 duv1 = dFdx(uv);
    vec2 duv2 = dFdy(uv);
    vec3 dp2perp = cross(dp2, n);
    vec3 dp1perp = cross(n, dp1);
    vec3 t = dp2perp * duv1.x + dp1perp * duv2.x;
    vec3 b = dp2perp * duv1.y + dp1perp * duv2.y;
    float invMax = inversesqrt(max(max(dot(t, t), dot(b, b)), NORMAL_MAP_EPS));
    return normalize(mat3(t * invMax, b * invMax, n) * mapN);
  }
`

/**
 * The scene's fog (`scene.setFog`): the uniform set it writes - `uFogColor`,
 * the linear band `uFogNear` / `uFogInv` (1 / (far - near)), the exp2
 * `uFogDensity`, and the height attenuation `uFogHeight` /
 * `uFogHeightFalloff`; the form not in use is 0, a fogless scene writes
 * every rate 0, so the factor is 0 with no branch and no enable flag -
 * plus `vec3 fog(vec3 rgb, float alpha, vec3 worldPos, vec3 camPos)`: the
 * factor by the RADIAL distance from the camera (the larger of the two
 * forms), thinned by `exp(-(y - height) * falloff)` above the fog height,
 * mixing toward the fog color at the fragment's written alpha
 * (premultiplied output stays premultiplied). Compose it last, after the
 * alphaTest discard, with the alpha you are about to write:
 *
 *   fragColor = vec4(fog(rgb, a, vWorldPos, uCamPos), a);
 *
 * The standard materials compose exactly this; `fog: false` on one drops
 * it (a sky sphere, a far backdrop). The background is not fogged.
 *
 * An ADDITIVE blend (`blend: "add"`) must not fade toward the fog color
 * - a distant glow would brighten into a sky-colored halo - so it uses
 * `vec3 fogAdditive(vec3 rgb, vec3 worldPos, vec3 camPos)`, the same
 * factor fading toward black.
 *
 * Only what composes one of these is fogged: a shaderMaterial that does
 * not - and so every instanced mesh, whose material is always custom -
 * stays crisp in a fogged scene. The engine cannot inject it for you.
 */
export const FOG = glsl`
  uniform vec3 uFogColor;
  uniform float uFogNear;
  uniform float uFogInv;
  uniform float uFogDensity;
  uniform float uFogHeight;
  uniform float uFogHeightFalloff;
  vec3 fog(vec3 rgb, float alpha, vec3 worldPos, vec3 camPos) {
    float d = distance(worldPos, camPos);
    float linear = clamp((d - uFogNear) * uFogInv, 0.0, 1.0);
    float dd = d * uFogDensity;
    float exp2 = 1.0 - exp(-dd * dd);
    float h = exp(-max(worldPos.y - uFogHeight, 0.0) * uFogHeightFalloff);
    return mix(rgb, uFogColor * alpha, max(linear, exp2) * h);
  }
  vec3 fogAdditive(vec3 rgb, vec3 worldPos, vec3 camPos) {
    return fog(rgb, 0.0, worldPos, camPos);
  }
`

/**
 * The sRGB transfer functions (IEC 61966-2-1): `vec3 srgbToLinear(vec3)`
 * and `vec3 linearToSrgb(vec3)`, clamped at zero. An "rgba8-srgb" texture
 * decodes in hardware and needs neither; these serve pixels a format
 * cannot tag (a render target sampled as a map) and the output stage.
 * OUTPUT includes this set - never compose both.
 */
export const SRGB = glsl`
  vec3 srgbToLinear(vec3 c) {
    c = max(c, vec3(0.0));
    return mix(c / 12.92, pow((c + 0.055) / 1.055, vec3(2.4)), step(0.04045, c));
  }
  vec3 linearToSrgb(vec3 c) {
    c = max(c, vec3(0.0));
    return mix(c * 12.92, 1.055 * pow(c, vec3(1.0 / 2.4)) - 0.055, step(0.0031308, c));
  }
`

/**
 * The output stage every library fragment ends with: `vec4
 * outputColor(vec3 rgb, float alpha)` takes the PREMULTIPLIED linear
 * result and its alpha, applies the scene's exposure and tone mapping
 * (`uniform float uExposure`; `uniform float uToneMapping`, 0 none, 1
 * ACES - the shared params scene.setExposure / setToneMapping write),
 * encodes to sRGB and returns the premultiplied pixel the target holds.
 * It un-premultiplies inside so the encode is exact at partial alpha.
 * Compose it last in a custom fragment: `fragColor = outputColor(rgb,
 * alpha);` - a fragment that writes fragColor directly writes final
 * encoded pixels and skips exposure and tone mapping. Includes SRGB.
 */
export const OUTPUT = glsl`
  uniform float uExposure;
  uniform float uToneMapping;
  // 1 (the scene's default) clamps and encodes to sRGB for display; 0
  // writes linear light unclamped, what a reflection probe's faces hold
  // so the environment lookups read radiance (the scene sets it per
  // target; an 8-bit target clamps it on write regardless).
  uniform float uOutputEncode;
  ${SRGB}
  // Stephen Hill's fit of the ACES RRT and ODT (Three's ACESFilmic, Godot's
  // ACES): the matrices between sRGB and the fit's working space, and the
  // exposure scale that puts middle gray where the reference does.
  const float ACES_EXPOSURE_SCALE = 1.0 / 0.6;
  const mat3 ACES_INPUT = mat3(
    0.59719, 0.07600, 0.02840,
    0.35458, 0.90834, 0.13383,
    0.04823, 0.01566, 0.83777);
  const mat3 ACES_OUTPUT = mat3(
    1.60475, -0.10208, -0.00327,
    -0.53108, 1.10813, -0.07276,
    -0.07367, -0.00605, 1.07602);
  vec3 acesFit(vec3 v) {
    vec3 a = v * (v + 0.0245786) - 0.000090537;
    vec3 b = v * (0.983729 * v + 0.4329510) + 0.238081;
    return a / b;
  }
  vec3 acesToneMap(vec3 c) {
    c = ACES_INPUT * (c * ACES_EXPOSURE_SCALE);
    c = acesFit(c);
    return clamp(ACES_OUTPUT * c, 0.0, 1.0);
  }
  vec4 outputColor(vec3 rgb, float alpha) {
    vec3 c = alpha > 0.0 ? rgb / alpha : rgb;
    c *= uExposure;
    if (uToneMapping > 0.5) c = acesToneMap(c);
    // Linear output keeps its range (a half-float probe holds a sun's
    // radiance); the display encode is 8-bit and clamps first.
    if (uOutputEncode > 0.5) c = linearToSrgb(clamp(c, 0.0, 1.0));
    else c = max(c, vec3(0.0));
    return vec4(c * alpha, alpha);
  }
`

/**
 * The scene's environment as a reflecting program declares it: `uEnv`
 * (a samplerCube the scene binds on every receiving target - a 1x1 black
 * cube while no environment is set), `uEnvIntensity`, `uEnvRotation`
 * (the inverse of the environment's y turn, the skybox's convention) and
 * `uEnvOn` (0 with no environment, so the term contributes nothing
 * instead of reflecting black). `envRadiance(r, roughness)` samples the
 * environment along world direction `r` at a mip level from a perceptual
 * roughness - 0 the sharp base level, 1 the level whose faces are
 * ENV_ROUGH_FACE (4) texels wide, linear between (Godot's
 * `roughness * MAX_ROUGHNESS_LOD`; Unity's six LOD steps; Three maps
 * onto a 16-texel floor): the level a baked chain (`srt tool
 * 3d/environment`, loadEnvironment) convolves at that roughness, and a
 * generated chain merely box-filters - scaled by the intensity: the
 * prefiltered radiance of the split sum (`standard` multiplies it by PBR's
 * envBrdf); a cube uploaded without mipmaps stays sharp.
 * `envIrradiance(n)` is the diffuse face of it - the fully rough sample
 * along the normal, what Three (`getIBLIrradiance`) and Godot read for
 * the image-lit diffuse term; `standard` adds it to the hemisphere
 * ambient. `envReflection(n, v, shininess)`
 * is the Blinn-Phong face of it: the mirror direction (v toward the
 * camera) at roughness `sqrt(2 / (shininess + 2))`, so a wide sheen
 * reflects a blurred environment and a mirror dot a sharp one. `envWeight(n, v,
 * reflectivity)` is the Schlick fresnel weight: `reflectivity` face-on,
 * 1 at grazing, 0 with no environment. `lit` applies them as
 * `rgb = mix(rgb, envReflection(n, v, uShininess), envWeight(n, v,
 * uReflectivity))` - Three's MixOperation with a fresnel weight. Every
 * lookup is a plain `texture(uEnv, dir)` in world space: a cube map here
 * holds what a lookup returns (GL's own convention, as Godot and Unity
 * sample theirs), so there is no handedness flip and a Three shader
 * ported with `flipEnvMap` drops it.
 */
export const ENVIRONMENT = glsl`
  uniform samplerCube uEnv;
  uniform float uEnvIntensity;
  uniform mat4 uEnvRotation;
  uniform float uEnvOn;
  // Schlick's approximation exponent.
  const float ENV_SCHLICK_POWER = 5.0;
  // log2 of the face edge (4 texels) a roughness of 1 samples: the chain
  // is convolved down to it (environment-bake.ts ENV_ROUGH_FACE), and the
  // levels below keep the sampler complete.
  const float ENV_ROUGH_FACE_LOG2 = 2.0;
  vec3 envRadiance(vec3 r, float roughness) {
    float roughLevel = max(log2(float(textureSize(uEnv, 0).x)) - ENV_ROUGH_FACE_LOG2, 0.0);
    return textureLod(uEnv, mat3(uEnvRotation) * r, roughness * roughLevel).rgb * uEnvIntensity;
  }
  vec3 envIrradiance(vec3 n) {
    return envRadiance(n, 1.0);
  }
  vec3 envReflection(vec3 n, vec3 v, float shininess) {
    return envRadiance(reflect(-v, n), sqrt(2.0 / (shininess + 2.0)));
  }
  float envWeight(vec3 n, vec3 v, float reflectivity) {
    float f = pow(1.0 - max(dot(n, v), 0.0), ENV_SCHLICK_POWER);
    return (reflectivity + (1.0 - reflectivity) * f) * uEnvOn;
  }
`

/**
 * The scene's shadow set as a receiving program declares it: ONE
 * `uShadowAtlas` (a `sampler2DShadow` - every casting light's depth map is
 * a tile of it, so N maps render as one pass; a cleared one-texel depth
 * map when nothing casts), a MAP slot
 * set of `MAX_SHADOW_MAPS` - `uShadowRect[M]` (map slot j's tile as x, y,
 * width, height in atlas 0..1 UV) and `uShadowMatrix[M]` (its light-space
 * viewProj) - and, per light index (the light list's, hemisphere
 * excluded), `uShadowFirst[N]` /
 * `uShadowCount[N]` (light i's maps are slots `first .. first + count - 1`;
 * count 0 = it does not cast; a box light has one map, a cascaded light
 * `shadow.cascades` of them tightest first, a point light six face maps
 * in +X, -X, +Y, -Y, +Z, -Z order), `uShadowBias[N]`,
 * `uShadowNormalBias[N]` and `uShadowRadius[N]` (the filter radius in
 * map texels, 1 = one tap). The scene binds and writes all of it on every
 * target a receiving material can draw into; a custom material composes
 * LIGHT_SLOTS, this, SHADOW, then SHADOW_LOOKUP (in that order - the
 * lookup reads the light type and position for a point light's face
 * select) and multiplies
 * light i's term by `lightShadow(i, worldPos, n)` - `lit` is the shape. A
 * material that does not receive composes none of it, so it declares no
 * sampler for nothing.
 */
export const SHADOW_SLOTS = glsl`
  uniform sampler2DShadow uShadowAtlas;
  uniform vec4 uShadowRect[${MAX_SHADOW_MAPS}];
  uniform mat4 uShadowMatrix[${MAX_SHADOW_MAPS}];
  uniform int uShadowFirst[${MAX_LIGHTS}];
  uniform int uShadowCount[${MAX_LIGHTS}];
  uniform float uShadowBias[${MAX_LIGHTS}];
  uniform float uShadowNormalBias[${MAX_LIGHTS}];
  uniform float uShadowRadius[${MAX_LIGHTS}];
`

/**
 * The shadow lookup in three steps plus their composition, one map at a
 * time. `vec3 shadowPoint(vec4 coord)` takes a world point carried into a
 * casting light's clip space by that map's `uShadowMatrix[j]` to its map
 * point: xy in 0..1 across the map, z the depth to compare. `bool
 * shadowInside(vec3 p)` is whether the map has it at all (xy in 0..1, z
 * not past the far plane) - the cascade select. `float shadowSample(
 * sampler2DShadow map, vec4 rect, vec3 p, float bias, float radius)` is
 * the factor (1 lit, 0 shadowed) of a point the map has. At `radius` 1
 * it is ONE comparison tap - the sampler compares `p.z - bias` against
 * the map in hardware (LEQUAL) and LINEAR-weights the four neighbours'
 * results, the 2x2 PCF a shader loop cannot match (it weights the
 * compare, not the depth). Above 1 it averages a 3x3 grid of those taps
 * `radius` texels apart (Three's PCF kernel: the edge softens over about
 * 2 * radius texels; past ~3 the taps separate and the edge bands). A tap
 * lands in the map's tile `rect` (x, y, width, height in the atlas's 0..1
 * UV; `vec4(0, 0, 1, 1)` is a whole map), clamped to the tile inset by
 * half a texel so the footprint never reads a neighbouring map's tile.
 * `float shadow(sampler2DShadow map, vec4 rect, vec4 coord, float bias,
 * float radius)` composes the three: 1 (lit) outside the map, else the
 * sample - `shadow(uShadowAtlas, uShadowRect[0], uShadowMatrix[0] *
 * vec4(vWorldPos, 1.0), uShadowBias[0], uShadowRadius[0])`.
 * SHADOW_LOOKUP uses the steps, so it projects each map once. The engine
 * binds the comparison sampler wherever a program declares the uniform
 * as sampler2DShadow; a program declaring plain `sampler2D uShadowAtlas`
 * (hand-rolled old GLSL) still reads raw depth values at nearest.
 */
export const SHADOW = glsl`
  vec3 shadowPoint(vec4 coord) {
    return coord.xyz / coord.w * 0.5 + 0.5;
  }

  bool shadowInside(vec3 p) {
    return all(greaterThanEqual(p.xy, vec2(0.0))) && all(lessThanEqual(p, vec3(1.0)));
  }

  float shadowSample(sampler2DShadow map, vec4 rect, vec3 p, float bias, float radius) {
    vec2 texel = 1.0 / vec2(textureSize(map, 0));
    vec2 lo = rect.xy + 0.5 * texel;
    vec2 hi = rect.xy + rect.zw - 0.5 * texel;
    vec2 base = rect.xy + p.xy * rect.zw;
    float z = p.z - bias;
    if (radius <= 1.0) return texture(map, vec3(clamp(base, lo, hi), z));
    vec2 step = texel * radius;
    float sum = 0.0;
    for (int y = -1; y <= 1; y++) {
      for (int x = -1; x <= 1; x++) {
        sum += texture(map, vec3(clamp(base + vec2(float(x), float(y)) * step, lo, hi), z));
      }
    }
    return sum / 9.0;
  }

  float shadow(sampler2DShadow map, vec4 rect, vec4 coord, float bias, float radius) {
    vec3 p = shadowPoint(coord);
    return shadowInside(p) ? shadowSample(map, rect, p, bias, radius) : 1.0;
  }
`

/**
 * The step from a light index to its shadow factor, over LIGHT_SLOTS,
 * SHADOW_SLOTS and SHADOW (compose all three first - the light type and
 * position pick a point light's face map). `float lightShadow(int i,
 * vec3 worldPos, vec3 n)` is the one to call per light: 1 for a light
 * that does not cast, else the factor for `worldPos` pushed along its
 * normal `n` by `uShadowNormalBias[i]` (the acne knob to reach for
 * first). A point light's six face maps are selected directly: the
 * dominant axis of the light-to-point vector names the face (the slot
 * order +X, -X, +Y, -Y, +Z, -Z), one projection, one tap. Every other
 * light is looked up in the FIRST of its maps that has the point (a box
 * light has one; a cascaded light's maps come tightest first, so the
 * sharpest cascade that has the point wins and a point past the last is
 * lit) with that map's `uShadowMatrix[j]`, its tile, `uShadowBias[i]`
 * and `uShadowRadius[i]`. Inside the outer SHADOW_BLEND of a map (in map 0..1
 * units, so 0.1 is its outer 10% on each side) the factor fades into
 * the next cascade's, so the hand-over is a band and not a seam; the
 * last map, a box light's only one, and any rim the next cascade does
 * not reach (the near side, at the camera's feet) have no band.
 * Position and normal are arguments, so no varying name is pinned and a
 * custom vertex stage composes freely.
 */
export const SHADOW_LOOKUP = glsl`
  const float SHADOW_BLEND = 0.1;

  float lightShadow(int i, vec3 worldPos, vec3 n) {
    int count = uShadowCount[i];
    if (count == 0) return 1.0;
    vec4 w = vec4(worldPos + n * uShadowNormalBias[i], 1.0);
    float bias = uShadowBias[i];
    float radius = uShadowRadius[i];
    int first = uShadowFirst[i];
    if (uLightType[i] == LIGHT_POINT) {
      // Six 90-degree face frusta partition every direction: the
      // dominant axis IS the face, so no scan and no behind-the-camera
      // projection (a perspective map's w flips sign there and xy can
      // land inside 0..1 spuriously).
      vec3 dv = worldPos - uLightPos[i];
      vec3 ad = abs(dv);
      int face;
      if (ad.x >= ad.y && ad.x >= ad.z) face = dv.x > 0.0 ? 0 : 1;
      else if (ad.y >= ad.z) face = dv.y > 0.0 ? 2 : 3;
      else face = dv.z > 0.0 ? 4 : 5;
      int j = first + face;
      return shadow(uShadowAtlas, uShadowRect[j], uShadowMatrix[j] * w, bias, radius);
    }
    int last = first + count - 1;
    for (int j = first; j <= last; j++) {
      vec3 p = shadowPoint(uShadowMatrix[j] * w);
      if (!shadowInside(p)) continue;
      float s = shadowSample(uShadowAtlas, uShadowRect[j], p, bias, radius);
      if (j == last) return s;
      float edge = min(min(p.x, 1.0 - p.x), min(p.y, 1.0 - p.y));
      if (edge >= SHADOW_BLEND) return s;
      vec3 q = shadowPoint(uShadowMatrix[j + 1] * w);
      if (!shadowInside(q)) return s;
      return mix(shadowSample(uShadowAtlas, uShadowRect[j + 1], q, bias, radius), s, edge / SHADOW_BLEND);
    }
    return 1.0;
  }
`

/**
 * The option set the lit program builders take: the flags that pick the
 * program - the same names and defaults as LitOptions, minus the values
 * (`map: true` says the fragment samples uMap, not which texture) - plus
 * two slots an app splices its own GLSL into.
 *
 * The slots are deliberately narrow: `prelude` adds declarations,
 * `discardIf` is a bool EXPRESSION, spliced at a fixed point - not a
 * statement block against named locals, so no local of the generated
 * program is part of the contract and restructuring it stays an internal
 * change. A slot can read the varyings (vWorldPos, vNormal, vUv, and
 * vColor with vertexColors), the uniforms the flags declare, and whatever
 * `prelude` declares; a uniform `prelude` declares is an ordinary
 * per-entry param, passed to instance() like uColor. Colors in this
 * program are PREMULTIPLIED (the engine's pixel contract), which is why
 * no slot touches them.
 *
 * Reach past the slots - a tint, a normal perturbation, a different light
 * model - by composing a fragment from LIT_VERTEX and the pure functions
 * above. That is the same import list, not a lower tier.
 */
export type LitSourceOptions = {
  /** The fragment samples a `uniform sampler2D uMap`, tinted by uColor. */
  map?: boolean
  /** Multiply the base by the colored layout's per-vertex vColor; pairs
   * with LIT_VERTEX_COLORED (litVertex picks it). */
  vertexColors?: boolean
  /** Sample uMap by world position blended across the three axis planes,
   * at `uniform float uTriplanar` repeats per world unit, instead of by
   * UV. Needs `map`. */
  triplanar?: boolean
  /** Write the base alpha through and blend; opaque (the default) writes
   * alpha 1, so a leaked texel alpha cannot punch through it. */
  transparent?: boolean
  /** Compose the scene's shadow set and multiply each light's term by its
   * factor (default true). False declares no shadow sampler at all. */
  receiveShadow?: boolean
  /** Which faces the material's own pipeline drops (default "back"). The
   * fragment's business because a program that shows back faces lights
   * them with the normal flipped, else a double-sided leaf's back is
   * black. */
  cull?: CullMode
  /** Declare `uniform float uAlphaTest` and discard a fragment whose base
   * alpha falls below it (the cutoff is a per-entry uniform, so one
   * program serves every value). */
  alphaTest?: boolean
  /** Compose the scene's fog over the result (default true). */
  fog?: boolean
  /** Bend the lit normal by a tangent-space normal map (NORMAL_MAP:
   * `uniform sampler2D uNormalMap` scaled by `uniform float
   * uNormalScale`), sampled at the same uv as uMap. The frame comes from
   * screen-space derivatives - no tangent attribute. The cutout and
   * discardIf still see the geometric normal; lighting sees the bent
   * one. Not with `triplanar` (which samples by world position, not uv). */
  normalMap?: boolean
  /** Add `uniform vec3 uEmissive` after the lighting terms - unlit by
   * design, shadow-proof, fogged like everything else. */
  emissive?: boolean
  /** `uniform sampler2D uEmissiveMap` multiplying uEmissive; implies
   * `emissive`. */
  emissiveMap?: boolean
  /** `uniform sampler2D uSpecularMap`: its RED channel scales uSpecular
   * per fragment (Three's specularMap - chrome and rubber on one mesh). */
  specularMap?: boolean
  /** Compose the scene's ENVIRONMENT and mix its mirror reflection in by
   * `uniform float uReflectivity` (the Schlick face-on weight), scaled
   * per fragment by uSpecularMap's red when `specularMap`. */
  env?: boolean
  /** Add a baked-light term: `uniform sampler2D uLightMap` times
   * `uniform float uLightMapIntensity`, sampled by the aUV2 channel and
   * ADDED to the light sum like the hemisphere term (a fully baked scene
   * runs with no lights at all). litVertex(o) then reads aUV2, so the
   * geometry must carry that channel. */
  lightMap?: boolean
  /** Declare `uniform vec4 uMapTransform` ([repeatU, repeatV, offsetU,
   * offsetV]) and sample every uv-driven map at `vUv * repeat + offset` -
   * ONE transform for the material's uv maps (Godot's uv1_offset/scale,
   * Unity's Tiling/Offset), deliberately not Three's per-texture
   * transform: a TextureId is a shared value whose sampling is
   * creation-time state. The shadow source transforms its cutout the
   * same way; lightMap's aUV2 is not transformed. Not with `triplanar`. */
  mapTransform?: boolean
  /** Skin positions and normals by the "skinned" layout's aJoints/
   * aWeights against the `uBones` palette texture (rgba32f, 4 texels
   * wide, one row per joint: model-local jointWorld x inverseBind -
   * the spatial flush writes it). The vertex stage then requires that layout
   * and something must bind `uBones`; the fragment is unchanged. */
  skinned?: boolean
  /** GLSL spliced at file scope, before main: the uniforms, constants and
   * helper functions `discardIf` calls. */
  prelude?: string
  /** A `bool` expression: true discards the fragment. Runs where the
   * alphaTest discard runs, after the base color is resolved. Splices
   * into the SHADOW source too, so what it discards casts no shadow. */
  discardIf?: string
}

/**
 * The option set `standardFragment` builds from: every lit option but
 * the Blinn-Phong ones (`specularMap`; `env`, since the environment is
 * always composed), plus the two packed data maps.
 */
export type StandardSourceOptions = Omit<LitSourceOptions, "specularMap" | "env"> & {
  /** `uniform sampler2D uMetalnessMap`: its BLUE channel scales
   * uMetalness per fragment (Three's metalnessMap, glTF's packed
   * channel). */
  metalnessMap?: boolean
  /** `uniform sampler2D uRoughnessMap`: its GREEN channel scales
   * uRoughness (Three's roughnessMap, glTF's packed channel - bind the
   * same texture to both). */
  roughnessMap?: boolean
}

// The resolved option set: every flag concrete, every slot a string, so
// the builders below never re-apply a default and `lit`'s class key and
// its program cannot drift apart.
type LitSource = {
  map: boolean
  vertexColors: boolean
  triplanar: boolean
  transparent: boolean
  receiveShadow: boolean
  cull: CullMode
  alphaTest: boolean
  fog: boolean
  normalMap: boolean
  emissive: boolean
  emissiveMap: boolean
  specularMap: boolean
  env: boolean
  lightMap: boolean
  mapTransform: boolean
  skinned: boolean
  prelude: string
  discardIf: string
  // The light model: Blinn-Phong (`lit`) or GGX metalness/roughness
  // (`standard`, which also samples the environment unconditionally).
  brdf: "blinn" | "ggx"
  metalnessMap: boolean
  roughnessMap: boolean
}

function resolveLit(o: LitSourceOptions): LitSource {
  return {
    map: o.map === true,
    vertexColors: o.vertexColors === true,
    triplanar: o.triplanar === true && o.map === true,
    transparent: o.transparent === true,
    receiveShadow: o.receiveShadow !== false,
    cull: o.cull ?? "back",
    alphaTest: o.alphaTest === true,
    fog: o.fog !== false,
    normalMap: o.normalMap === true,
    emissive: o.emissive === true || o.emissiveMap === true,
    emissiveMap: o.emissiveMap === true,
    specularMap: o.specularMap === true,
    env: o.env === true,
    lightMap: o.lightMap === true,
    mapTransform: o.mapTransform === true,
    skinned: o.skinned === true,
    prelude: o.prelude ?? "",
    discardIf: o.discardIf ?? "",
    brdf: "blinn",
    metalnessMap: false,
    roughnessMap: false,
  }
}

function resolveStandard(o: StandardSourceOptions): LitSource {
  return {
    ...resolveLit(o),
    specularMap: false,
    env: true,
    brdf: "ggx",
    metalnessMap: o.metalnessMap === true,
    roughnessMap: o.roughnessMap === true,
  }
}

/** Whether the source samples anything by uv - then litBase resolves the
 * `uv` local (transformed when mapTransform) both passes sample by. */
function litUv(c: LitSource): boolean {
  return (c.map && !c.triplanar) || c.normalMap || c.emissiveMap || c.specularMap || c.metalnessMap || c.roughnessMap
}

// The varyings both lit sources read. The shadow source declares the same
// set even where it uses none of them, so a discardIf written against the
// main pass compiles unchanged there.
function litVaryings(c: LitSource): string {
  return glsl`
    in vec3 vWorldPos;
    in vec3 vNormal;
    in vec2 vUv;
    ${c.vertexColors ? "in vec4 vColor;" : ""}
    ${c.lightMap ? "in vec2 vUv2;" : ""}
  `
}

// Base color to discard decision, shared by the main and shadow sources:
// uColor, the map (by UV or triplanar), the vertex color, then the cutout
// and the app's own discard. `n` is the surface normal the sampling and
// discardIf see: flipped toward the viewer in the main pass of a program
// that shows back faces, the plain geometric normal in the shadow pass
// (which draws the other side of the surface; triplanar weights are
// abs(n), so the sample is the same either way).
function litBase(c: LitSource, flip: boolean): string {
  return glsl`
    vec3 n = normalize(vNormal);
    ${flip ? "if (!gl_FrontFacing) n = -n;" : ""}
    ${litUv(c) ? `vec2 uv = ${c.mapTransform ? "vUv * uMapTransform.xy + uMapTransform.zw" : "vUv"};` : ""}
    vec4 base = uColor;
    ${
      c.map
        ? c.triplanar
          ? `vec3 w = pow(abs(n), vec3(4.0));
    w /= w.x + w.y + w.z;
    vec3 p = vWorldPos * uTriplanar;
    base *= texture(uMap, p.yz) * w.x + texture(uMap, p.xz) * w.y + texture(uMap, p.xy) * w.z;`
          : "base *= texture(uMap, uv);"
        : ""
    }
    ${c.vertexColors ? "base *= vColor;" : ""}
    ${c.alphaTest ? "if (base.a < uAlphaTest) discard;" : ""}
    ${c.discardIf ? `if (${c.discardIf}) discard;` : ""}
  `
}

/**
 * The vertex stage `lit` pairs with a given option set: LIT_VERTEX, or
 * LIT_VERTEX_COLORED when the fragment reads vColor. The shadow source
 * takes this same stage, which is what lets a discardIf read the same
 * varyings in both passes.
 */
export function litVertex(o: LitSourceOptions = {}): string {
  return litVertexSource(o.vertexColors === true, o.lightMap === true, o.skinned === true)
}

/**
 * The `lit` fragment source for an option set - the exact program `lit`
 * itself builds, composed from the constants above. Pair it with
 * litVertex(o) in a shaderMaterialClass to get a lit material with your
 * own GLSL in it, and pass litShadowFragment(o) alongside when it can
 * discard.
 *
 * Per-entry uniforms to supply on instance(): `uColor` (premultiplied
 * vec4), `uSpecular`, `uShininess`, plus `uMap`/`uTriplanar`/`uAlphaTest`/
 * `uNormalMap`+`uNormalScale`/`uEmissive`/`uEmissiveMap`/`uSpecularMap`/
 * `uLightMap`+`uLightMapIntensity`/`uMapTransform`
 * for the options that declare them, plus whatever `prelude` declares.
 * Everything else - the lights, the hemisphere, the camera position, the
 * shadow set, the fog - is written by the scene.
 */
export function litFragment(o: LitSourceOptions = {}): string {
  return lightingFragment(resolveLit(o))
}

/**
 * The `standard` fragment source for an option set - the exact program
 * `standard` builds: the lit program (base, cutout, normal map, the
 * light and shadow loop, emissive, fog, output) shaded with the GGX
 * metalness/roughness model (PBR) instead of Blinn-Phong, and the
 * scene's ENVIRONMENT sampled unconditionally as the split sum
 * (envRadiance times envBrdf). Pair it with litVertex(o), and with
 * litShadowFragment(o) when it can discard, exactly as litFragment.
 *
 * Per-entry uniforms: `uColor` (premultiplied vec4), `uMetalness`,
 * `uRoughness`, `uMetalnessMap` (blue channel) and `uRoughnessMap`
 * (green) when declared, plus the lit slots the options declare
 * (`uMap`/`uTriplanar`/`uAlphaTest`/`uNormalMap`+`uNormalScale`/
 * `uEmissive`/`uEmissiveMap`/`uLightMap`+`uLightMapIntensity`/
 * `uMapTransform`) and whatever `prelude` declares.
 */
export function standardFragment(o: StandardSourceOptions = {}): string {
  return lightingFragment(resolveStandard(o))
}

// The one lit program, both light models. What the brdf changes: the
// material uniforms, each light's specular term, and the image lighting
// (Blinn-Phong mixes a fresnel-weighted mirror in when asked; GGX adds
// the split-sum term always). The light convention is Godot's and
// Unity's for both: a light of intensity 1 lights a white diffuse
// surface to 1 face-on, and the GGX lobe carries the pi its
// normalisation assumes (PBR's doc).
function lightingFragment(c: LitSource): string {
  let alpha = c.transparent ? "base.a" : "1.0"
  let ggx = c.brdf === "ggx"
  return glsl`
    ${litVaryings(c)}
    uniform vec4 uColor;
    ${c.map ? "uniform sampler2D uMap;" : ""}
    ${ggx ? "uniform float uMetalness;\n    uniform float uRoughness;" : "uniform float uSpecular;\n    uniform float uShininess;"}
    ${c.metalnessMap ? "uniform sampler2D uMetalnessMap;" : ""}
    ${c.roughnessMap ? "uniform sampler2D uRoughnessMap;" : ""}
    ${c.triplanar ? "uniform float uTriplanar;" : ""}
    ${c.alphaTest ? "uniform float uAlphaTest;" : ""}
    ${c.emissive ? "uniform vec3 uEmissive;" : ""}
    ${c.emissiveMap ? "uniform sampler2D uEmissiveMap;" : ""}
    ${c.specularMap ? "uniform sampler2D uSpecularMap;" : ""}
    ${c.env && !ggx ? "uniform float uReflectivity;" : ""}
    ${c.lightMap ? "uniform sampler2D uLightMap;\n    uniform float uLightMapIntensity;" : ""}
    ${c.mapTransform ? "uniform vec4 uMapTransform;" : ""}
    ${c.normalMap ? NORMAL_MAP : ""}
    uniform vec3 uCamPos;
    uniform vec3 uHemiSky;
    uniform vec3 uHemiGround;
    ${LIGHT_SLOTS}
    ${LIGHT_LOOKUP}
    ${
      c.receiveShadow
        ? `${SHADOW_SLOTS}
    ${SHADOW}
    ${SHADOW_LOOKUP}`
        : ""
    }
    ${HEMISPHERE}
    ${LAMBERT}
    ${ggx ? PBR : BLINN_SPECULAR}
    ${c.env ? ENVIRONMENT : ""}
    ${c.fog ? FOG : ""}
    ${OUTPUT}
    ${c.prelude}

    void main() {
      ${litBase(c, c.cull !== "back")}
      ${c.normalMap ? "n = perturbNormal(n, vWorldPos, uv);" : ""}
      vec3 v = normalize(uCamPos - vWorldPos);
      ${
        ggx
          ? `float metalness = uMetalness${c.metalnessMap ? " * texture(uMetalnessMap, uv).b" : ""};
      float roughness = uRoughness${c.roughnessMap ? " * texture(uRoughnessMap, uv).g" : ""};
      vec3 diffuseColor = base.rgb * (1.0 - metalness);
      vec3 f0 = mix(vec3(DIELECTRIC_F0) * base.a, base.rgb, metalness);`
          : ""
      }
      vec3 light = hemisphere(n, uHemiSky, uHemiGround);
      ${ggx ? "light += envIrradiance(n);" : ""}
      ${c.lightMap ? "light += texture(uLightMap, vUv2).rgb * uLightMapIntensity;" : ""}
      vec3 spec = vec3(0.0);
      for (int i = 0; i < ${MAX_LIGHTS}; i++) {
        if (i >= uLightCount) break;
        vec3 l;
        float a = lightVector(i, vWorldPos, l);
        if (a <= 0.0) continue;
        ${c.receiveShadow ? "float s = lightShadow(i, vWorldPos, n);" : "float s = 1.0;"}
        vec3 lc = uLightColor[i] * (a * s);
        ${
          ggx
            ? `float nl = lambert(n, l);
        light += lc * nl;
        spec += lc * (nl * ggxSpecular(n, v, l, f0, roughness));`
            : `light += lc * lambert(n, l);
        spec += lc * blinnSpecular(n, v, l, uShininess);`
        }
      }
      ${
        ggx
          ? `vec3 rgb = diffuseColor * light + spec;
      vec2 ab = envBrdf(max(dot(n, v), 0.0), roughness);
      rgb += envRadiance(reflect(-v, n), roughness) * (f0 * ab.x + ab.y * base.a);`
          : `vec3 rgb = base.rgb * light + spec * ${c.specularMap ? "(uSpecular * texture(uSpecularMap, uv).r)" : "uSpecular"} * base.a;
      ${c.env ? `rgb = mix(rgb, envReflection(n, v, uShininess) * base.a, envWeight(n, v, uReflectivity${c.specularMap ? " * texture(uSpecularMap, uv).r" : ""}));` : ""}`
      }
      ${c.emissive ? `rgb += uEmissive${c.emissiveMap ? " * texture(uEmissiveMap, uv).rgb" : ""} * base.a;` : ""}
      ${c.fog ? `rgb = fog(rgb, ${alpha}, vWorldPos, uCamPos);` : ""}
      fragColor = outputColor(rgb, ${alpha});
    }
  `
}

/**
 * The depth-pass source that makes a discarding lit material cast what it
 * actually draws: the same base, cutout and discardIf as litFragment(o),
 * and nothing after them (no lighting, no fog - a shadow has neither).
 * Undefined when the option set cannot discard, which means the scene's
 * default depth material is already right and the material should carry
 * no `shadow` of its own.
 *
 * Use it as a second shaderMaterialClass on litVertex(o) with the
 * OPPOSITE cull (Three's shadowSide rule: `cull: "back"` casts from
 * "front"), instanced with the uniforms this source declares - uColor,
 * plus uMap/uTriplanar/uAlphaTest/prelude's as opted into - and passed as
 * the main instance's `shadow`.
 */
export function litShadowFragment(o: LitSourceOptions = {}): string | undefined {
  let c = resolveLit(o)
  if (!c.alphaTest && !c.discardIf) return undefined
  return glsl`
    ${litVaryings(c)}
    uniform vec4 uColor;
    ${c.map ? "uniform sampler2D uMap;" : ""}
    ${c.triplanar ? "uniform float uTriplanar;" : ""}
    ${c.alphaTest ? "uniform float uAlphaTest;" : ""}
    ${c.mapTransform ? "uniform vec4 uMapTransform;" : ""}
    ${c.prelude}

    void main() {
      ${litBase(c, false)}
      fragColor = vec4(1.0);
    }
  `
}

/**
 * The unlit vertex stage: model then view-projection, with UV and world
 * position as varyings (vWorldPos is the fog distance input; a fragment
 * that reads neither leaves the outs unmatched, which links fine).
 * aNormal from the shared layout is deliberately not declared - inactive
 * attributes are skipped and only the stride accounts for them.
 */
export const UNLIT_VERTEX = glsl`
  in vec3 aPos;
  in vec2 aUV;
  out vec2 vUv;
  out vec3 vWorldPos;
  uniform mat4 uModel;
  uniform mat4 uViewProj;

  void main() {
    vec4 world = uModel * vec4(aPos, 1.0);
    vWorldPos = world.xyz;
    gl_Position = uViewProj * world;
    vUv = aUV;
  }
`

/**
 * The option set the unlit builders take: LitSourceOptions minus
 * everything lighting decides (no vertexColors, triplanar, receiveShadow)
 * and minus cull (the unlit fragment has no facing-dependent code - cull
 * is pure pipeline state, passed to shaderMaterialClass directly). The
 * slots follow the lit contract; the varyings here are vUv and vWorldPos
 * only.
 */
export type UnlitSourceOptions = {
  /** The fragment samples a `uniform sampler2D uMap`, tinted by uColor. */
  map?: boolean
  /** Write the base alpha through and blend; opaque (the default) writes
   * alpha 1. */
  transparent?: boolean
  /** Declare `uniform float uAlphaTest` and discard below it. */
  alphaTest?: boolean
  /** Compose the scene's fog over the result (default true). */
  fog?: boolean
  /** Declare `uniform vec4 uMapTransform` ([repeatU, repeatV, offsetU,
   * offsetV]) and sample the map at `vUv * repeat + offset` (see the lit
   * option of the same name). Needs `map`. */
  mapTransform?: boolean
  /** Skin positions by the "skinned" layout against uBones (see the lit
   * option); unlitVertex(o) reads it, the fragment is unchanged. */
  skinned?: boolean
  /** GLSL spliced at file scope, before main. */
  prelude?: string
  /** A `bool` expression: true discards the fragment; splices into the
   * shadow source too. */
  discardIf?: string
}

type UnlitSource = {
  map: boolean
  transparent: boolean
  alphaTest: boolean
  fog: boolean
  mapTransform: boolean
  prelude: string
  discardIf: string
}

function resolveUnlit(o: UnlitSourceOptions): UnlitSource {
  return {
    map: o.map === true,
    transparent: o.transparent === true,
    alphaTest: o.alphaTest === true,
    fog: o.fog !== false,
    mapTransform: o.mapTransform === true && o.map === true,
    prelude: o.prelude ?? "",
    discardIf: o.discardIf ?? "",
  }
}

/**
 * The vertex stage `unlit` pairs with an option set: UNLIT_VERTEX, or its
 * skinned form (the skin matrix applied before uModel) when `skinned`.
 */
export function unlitVertex(o: UnlitSourceOptions = {}): string {
  if (o.skinned !== true) return UNLIT_VERTEX
  return glsl`
  in vec3 aPos;
  in vec2 aUV;
  ${SKIN_DECLS}
  out vec2 vUv;
  out vec3 vWorldPos;
  uniform mat4 uModel;
  uniform mat4 uViewProj;

  void main() {
    ${SKIN_MATRIX}
    vec4 world = uModel * (skin * vec4(aPos, 1.0));
    vWorldPos = world.xyz;
    gl_Position = uViewProj * world;
    vUv = aUV;
  }
`
}

/** The unlit base sample: uColor times the (possibly transformed) map. */
function unlitBase(c: UnlitSource): string {
  if (!c.map) return "vec4 base = uColor;"
  let uv = c.mapTransform ? "vUv * uMapTransform.xy + uMapTransform.zw" : "vUv"
  return `vec4 base = texture(uMap, ${uv}) * uColor;`
}

/**
 * The `unlit` fragment source for an option set (sprite shares it): the
 * color, times the map when there is one, the discards, then the scene's
 * fog unless opted out. An opaque program writes alpha 1: the scene
 * target is composited premultiplied, so a leaked texel alpha would punch
 * a hole through an opaque draw. Pair with UNLIT_VERTEX (or a custom
 * vertex stage writing vUv/vWorldPos, as sprite does).
 *
 * Per-entry uniforms on instance(): `uColor` (linear light, premultiplied
 * vec4), plus `uMap`/`uAlphaTest` for the options that declare them, plus
 * whatever `prelude` declares. Ends with OUTPUT's outputColor like every
 * library fragment.
 */
export function unlitFragment(o: UnlitSourceOptions = {}): string {
  let c = resolveUnlit(o)
  let alpha = c.transparent ? "base.a" : "1.0"
  return glsl`
    ${c.map ? "in vec2 vUv;" : ""}
    ${c.fog ? "in vec3 vWorldPos;" : ""}
    ${c.map ? "uniform sampler2D uMap;" : ""}
    uniform vec4 uColor;
    ${c.alphaTest ? "uniform float uAlphaTest;" : ""}
    ${c.mapTransform ? "uniform vec4 uMapTransform;" : ""}
    ${c.fog ? "uniform vec3 uCamPos;" : ""}
    ${c.fog ? FOG : ""}
    ${OUTPUT}
    ${c.prelude}
    void main() {
      ${unlitBase(c)}
      ${c.alphaTest ? "if (base.a < uAlphaTest) discard;" : ""}
      ${c.discardIf ? `if (${c.discardIf}) discard;` : ""}
      ${c.fog ? `base.rgb = fog(base.rgb, ${alpha}, vWorldPos, uCamPos);` : ""}
      fragColor = outputColor(base.rgb, ${alpha});
    }
  `
}

/**
 * The depth-pass source for a discarding unlit material, litShadowFragment's
 * unlit twin: the same base and discards, nothing else. Undefined when
 * the option set cannot discard. Pair with UNLIT_VERTEX and the opposite
 * cull, instanced with the uniforms it declares.
 */
export function unlitShadowFragment(o: UnlitSourceOptions = {}): string | undefined {
  let c = resolveUnlit(o)
  if (!c.alphaTest && !c.discardIf) return undefined
  return glsl`
    ${c.map ? "in vec2 vUv;" : ""}
    uniform vec4 uColor;
    ${c.map ? "uniform sampler2D uMap;" : ""}
    ${c.alphaTest ? "uniform float uAlphaTest;" : ""}
    ${c.mapTransform ? "uniform vec4 uMapTransform;" : ""}
    ${c.prelude}
    void main() {
      ${unlitBase(c)}
      ${c.alphaTest ? "if (base.a < uAlphaTest) discard;" : ""}
      ${c.discardIf ? `if (${c.discardIf}) discard;` : ""}
      fragColor = vec4(1.0);
    }
  `
}
