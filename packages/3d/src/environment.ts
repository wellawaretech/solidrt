// Environment helpers: the baked HDR environment loader (the .srte chain
// `srt tool 3d/environment` writes), the equirectangular-to-cube
// conversion a fetched LDR panorama needs at runtime, the GPU prefilter
// that turns a rendered cube (a reflection probe's, a baked sky's) into
// the roughness chain, and the placeholder cube a scene binds as uEnv
// while no environment is set. The face passes here render straight into
// cube draw targets: no readback, no re-upload.

import { file } from "flux:fs"
import {
  addDraw,
  compileShader,
  createCubeDrawTarget,
  createCubeTexture,
  createRenderPipeline,
  destroyProgram,
  destroyRenderPipeline,
  destroyShader,
  destroyTexture,
  glsl,
  limits,
  linkProgram,
  removeDraw,
  renderTarget,
  setDrawParams,
} from "@solidrt/core/gpu"
import type { CreateOptions, DrawId, ProgramId, RenderPipelineId, SamplerOptions, TextureFormatOptions, TextureId } from "@solidrt/core/gpu"
import { decodeEnvironment, levelRoughness, mipLevels } from "./environment-bake.ts"

/**
 * Read a baked environment (`srt tool 3d/environment sky.hdr -o
 * assets/sky.srte`) and upload it: the HDR cube with its GGX-prefiltered
 * mip chain, as an explicit "rgba16f" chain - no generated mipmaps, so it
 * works on every device. The id is what `environment={{ cube }}` and
 * `background={{ cube }}` take (Three's `scene.environment` from an
 * RGBELoader + PMREMGenerator, Unity's convolved reflection probe, Godot's
 * radiance map, all done at build time here). Created after an await, so
 * it is NOT auto-freed: an environment normally lives as long as the app;
 * destroyTexture it otherwise. The same async shape as loadModel (see
 * examples/model-load.tsx for the <Loading> pattern).
 */
export async function loadEnvironment(path: string, opts?: CreateOptions): Promise<TextureId> {
  let { size, levels } = decodeEnvironment(await file(path).bytes())
  return createCubeTexture(levels, size, { ...opts, format: "rgba16f", mipmap: true, label: opts?.label ?? "environment" })
}

// The GL cube-map table as GLSL (cubeDirection in environment-bake.ts):
// the world direction whose lookup lands on `face` at (s, t) in 0..1, t =
// 0 the first row, unnormalized. Every pass that writes a face texel
// (equirectToCube's, the prefilter's) derives its direction from it, so
// the face holds what a lookup of that direction returns.
const CUBE_FACE_DIRECTION = glsl`
  vec3 cubeFaceDirection(int face, vec2 st) {
    float a = 2.0 * st.x - 1.0;
    float b = 2.0 * st.y - 1.0;
    if (face == 0) return vec3(1.0, -b, -a);
    if (face == 1) return vec3(-1.0, -b, a);
    if (face == 2) return vec3(a, 1.0, b);
    if (face == 3) return vec3(a, -1.0, -b);
    if (face == 4) return vec3(a, -b, 1.0);
    return vec3(-a, -b, -1.0);
  }
`

// The vertex stage of every face pass: the attributeless covering
// triangle emitting vUV with t = 0 at the face's first row, so the
// fragment's texel is (s, t) of the GL cube table - the same vUV the
// background and shader-target passes carry.
const FACE_VERTEX = glsl`
  out vec2 vUV;
  void main() {
    vec2 p = vec2(float((gl_VertexID << 1) & 2), float(gl_VertexID & 2));
    vUV = p;
    gl_Position = vec4(p * 2.0 - 1.0, 0.0, 1.0);
  }
`

// A face pass: FACE_VERTEX over `fragment`, one pipeline owned by the
// caller (destroyed through `dispose` once its faces are rendered).
function facePipeline(fragment: string, label: string): { pipeline: RenderPipelineId; program: ProgramId; dispose(): void } {
  let vs = compileShader("vertex", FACE_VERTEX, { header: true })
  let fs = compileShader("fragment", fragment, { header: true })
  let program = linkProgram(vs, fs, { label })
  destroyShader(vs)
  destroyShader(fs)
  let pipeline = createRenderPipeline(program, { label })
  return {
    pipeline,
    program,
    dispose() {
      destroyRenderPipeline(pipeline)
      destroyProgram(program)
    },
  }
}

// Render the six faces of a cube draw target whose one entry reads uFace:
// the first five into level 0 explicitly (an explicit level regenerates
// nothing) and the last plainly, so a mipmapped cube regenerates its
// chain once.
function renderFaces(cube: TextureId, entry: DrawId): void {
  for (let face = 0; face < 6; face++) {
    setDrawParams(cube, entry, { uFace: face })
    if (face < 5) renderTarget(cube, face, 0)
    else renderTarget(cube, face)
  }
}

// One face of the cube: the texel's sampling direction from the GL
// cube-map table. The panorama's center column faces -Z (the
// camera's default forward; Godot's PanoramaSkyMaterial and Unity's
// Skybox/Panoramic agree; Three centers +X, a quarter turn away), its top
// row +Y. At the seam column the uv derivative jumps by a full turn;
// zeroing that component keeps a mipmapped source from drawing a line.
// An sRGB source samples decoded and the face is written linear; a cube
// created at the same format encodes it on write and decodes on sample,
// so the bytes round-trip.
const EQUIRECT_FACE = glsl`
  in vec2 vUV;
  uniform sampler2D uMap;
  uniform float uFace;
  ${CUBE_FACE_DIRECTION}
  const float PI = 3.14159265358979;
  // A uv derivative larger than this is the seam wrap, not a real step.
  const float SEAM_JUMP = 0.5;
  void main() {
    vec3 d = normalize(cubeFaceDirection(int(uFace + 0.5), vUV));
    vec2 uv = vec2(atan(d.x, -d.z) / (2.0 * PI) + 0.5, acos(clamp(d.y, -1.0, 1.0)) / PI);
    vec2 dx = dFdx(uv);
    vec2 dy = dFdy(uv);
    if (abs(dx.x) > SEAM_JUMP) dx.x = 0.0;
    if (abs(dy.x) > SEAM_JUMP) dy.x = 0.0;
    fragColor = textureGrad(uMap, uv, dx, dy);
  }
`

/**
 * Convert an equirectangular panorama (an uploaded 2D texture: createImage,
 * createTexture) into a cube map of `size` x `size` faces, returned as a
 * cube TextureId ready for a skybox or the environment - Three's
 * `WebGLCubeRenderTarget.fromEquirectangularTexture`, Unity's cube import
 * of a lat-long image. The six faces render on the GPU straight into a
 * cube draw target (no readback, no upload), synchronously. `opts` are
 * createCubeTexture's (`mipmap: true` for an environment shininess can
 * blur; `label`; `autoFree: false` to own it), and `format` names the
 * PANORAMA's format so the cube decodes like it: "rgba8-srgb" for a
 * photographed sky uploaded as such, "rgba8" (default) for data,
 * "rgba16f" for an HDR panorama (the cube keeps the range; renderable on
 * every device with half-float rendering, else it throws - the prefiltered
 * form of that panorama is `srt tool 3d/environment` plus
 * loadEnvironment). The panorama's center column faces -Z and its top
 * row is +Y. Leave its wrap at the default clamp: `repeat` would also
 * wrap vertically and bleed the poles across the top and bottom rows,
 * while the clamped seam column costs at most a texel-wide blend at +Z.
 * Three centers its panoramas on +X: a rotation tuned there differs by a
 * quarter turn here.
 */
export function equirectToCube(map: TextureId, size: number, opts?: CreateOptions & SamplerOptions & TextureFormatOptions): TextureId {
  if (!Number.isInteger(size) || size < 1) throw new Error("equirectToCube: size must be a positive integer, got " + size)
  let format = opts?.format ?? "rgba8"
  if (format !== "rgba8" && format !== "rgba8-srgb" && format !== "rgba16f") {
    throw new Error('equirectToCube: format must be "rgba8", "rgba8-srgb" or "rgba16f" (the panorama\'s), got ' + format)
  }
  let label = opts?.label ?? "equirect"
  let cube = createCubeDrawTarget(size, null, { ...opts, format, label })
  let pass = facePipeline(EQUIRECT_FACE, label + "-face")
  let entry = addDraw(cube, pass.pipeline, { uFace: 0 }, { vertexCount: 3, textures: { uMap: map } })
  renderFaces(cube, entry)
  // The faces are pixels now; the pass and its binding of the panorama go.
  removeDraw(cube, entry)
  pass.dispose()
  return cube
}

// One face of one level of the prefiltered chain: prefilterCube's GGX
// convolution (environment-bake.ts) as a fragment. The texel's direction
// is the normal, view and reflection direction at once (N = V = R, every
// engine's bake), the GGX lobe at uRoughness is importance sampled with a
// Hammersley set, each sample reading the sharp source cube at the mip
// level whose texel covers the sample's solid angle (the source carries a
// generated chain), weighted by the reflected direction's cosine. A
// roughness of 0 is the base level: a copy.
const PREFILTER_FACE = glsl`
  in vec2 vUV;
  uniform samplerCube uSource;
  uniform float uFace;
  uniform float uRoughness;
  ${CUBE_FACE_DIRECTION}
  const float PI = 3.14159265358979;
  // GGX samples per texel: the CPU bake takes 512; reading each sample at
  // the lod of its solid angle lets a fraction of that converge on the GPU.
  const int PREFILTER_SAMPLES = 64;
  // Bias on the source lod a sample reads (Karis: +1 smooths the estimate
  // without visible extra blur).
  const float SOURCE_LOD_BIAS = 1.0;
  // A sample's normal-lobe cosine below this contributes nothing (the lobe
  // mirrored below the horizon).
  const float MIN_NOL = 1e-6;
  // |n.z| above which the tangent frame's helper axis switches to +X.
  const float POLE = 0.999;
  // Van der Corput radical inverse: the second Hammersley coordinate.
  float radicalInverse(uint bits) {
    bits = (bits << 16u) | (bits >> 16u);
    bits = ((bits & 0x55555555u) << 1u) | ((bits & 0xAAAAAAAAu) >> 1u);
    bits = ((bits & 0x33333333u) << 2u) | ((bits & 0xCCCCCCCCu) >> 2u);
    bits = ((bits & 0x0F0F0F0Fu) << 4u) | ((bits & 0xF0F0F0F0u) >> 4u);
    bits = ((bits & 0x00FF00FFu) << 8u) | ((bits & 0xFF00FF00u) >> 8u);
    return float(bits) / 4294967296.0;
  }
  void main() {
    vec3 n = normalize(cubeFaceDirection(int(uFace + 0.5), vUV));
    if (uRoughness <= 0.0) {
      fragColor = vec4(textureLod(uSource, n, 0.0).rgb, 1.0);
      return;
    }
    float alpha = uRoughness * uRoughness;
    float a2 = alpha * alpha;
    float edge = float(textureSize(uSource, 0).x);
    // The solid angle of one base-level source texel.
    float texelAngle = 4.0 * PI / (6.0 * edge * edge);
    vec3 up = abs(n.z) > POLE ? vec3(1.0, 0.0, 0.0) : vec3(0.0, 0.0, 1.0);
    vec3 t = normalize(cross(up, n));
    vec3 b = cross(n, t);
    vec3 acc = vec3(0.0);
    float weight = 0.0;
    for (int i = 0; i < PREFILTER_SAMPLES; i++) {
      float u1 = (float(i) + 0.5) / float(PREFILTER_SAMPLES);
      float u2 = radicalInverse(uint(i));
      float phi = 2.0 * PI * u1;
      float cosT = sqrt((1.0 - u2) / (1.0 + (a2 - 1.0) * u2));
      float sinT = sqrt(max(0.0, 1.0 - cosT * cosT));
      // N = V, so the reflected sample's cosine is 2 cos^2 - 1.
      float nol = 2.0 * cosT * cosT - 1.0;
      if (nol <= MIN_NOL) continue;
      vec3 h = t * (sinT * cos(phi)) + b * (sinT * sin(phi)) + n * cosT;
      vec3 l = 2.0 * cosT * h - n;
      float d = cosT * cosT * (a2 - 1.0) + 1.0;
      float pdf = a2 / (PI * d * d) / 4.0;
      float sampleAngle = 1.0 / (float(PREFILTER_SAMPLES) * pdf);
      float lod = max(0.0, 0.5 * log2(sampleAngle / texelAngle) + SOURCE_LOD_BIAS);
      acc += textureLod(uSource, l, lod).rgb * nol;
      weight += nol;
    }
    fragColor = vec4(acc / weight, 1.0);
  }
`

/** A roughness chain rendered on the GPU from a sharp cube (internal: the
 * reflection probe's and bakeBackground's second half). */
export type Prefilter = {
  /** The chain: a mipmapped cube draw target of the source's size, what
   * the environment samples. */
  cube: TextureId
  /** Fill every level of every face from the source's current pixels:
   * level 0 a copy, each level below convolved at its levelRoughness. */
  run(): void
  /** Drop the pass (its pipeline, program and binding of the source) and
   * hand over the chain, which keeps its pixels: a one-shot bake's end.
   * The source is free to go afterwards. */
  finish(): TextureId
  /** finish() plus the chain itself. */
  dispose(): void
}

/** The format of a rendered radiance cube (a probe's faces, its chain, a
 * baked sky): half float where the device renders it, so the range of a
 * sun or an emissive survives into reflections as in every engine's HDR
 * probe, else 8-bit and clamped - the picture degrades, the app runs. Not
 * a knob: the renderer decides for all probes (Godot), HDR by default
 * (Unity). */
export type ProbeFormat = "rgba8" | "rgba16f"
export function probeFormat(): ProbeFormat {
  return limits.halfFloatRenderable ? "rgba16f" : "rgba8"
}

/**
 * The runtime counterpart of the bake tool's prefilterCube: a mipmapped
 * cube draw target of `size` and `format` whose chain `run()` fills from
 * `source`, a `mipmap: true` cube of the same size and format (a cube
 * draw target rendered sharp), one pass per face per level - 48 small
 * passes at 128. Three's PMREMGenerator, Unity's and Godot's probe
 * convolution, on the same roughness-to-level rule as the .srte chain
 * (levelRoughness), so `standard` reads both alike.
 */
export function createPrefilter(size: number, source: TextureId, format: ProbeFormat, label: string): Prefilter {
  let pass: ReturnType<typeof facePipeline> | null = facePipeline(PREFILTER_FACE, label + "-prefilter")
  let cube = createCubeDrawTarget(size, null, { mipmap: true, format, autoFree: false, label })
  let entry = addDraw(cube, pass.pipeline, { uFace: 0, uRoughness: 0 }, { vertexCount: 3, textures: { uSource: source } })
  let levels = mipLevels(size)
  let finish = () => {
    if (pass !== null) {
      removeDraw(cube, entry)
      pass.dispose()
      pass = null
    }
    return cube
  }
  return {
    cube,
    run() {
      if (pass === null) return
      for (let level = 0; level < levels; level++) {
        // The base is a copy whatever the size (a chain of 4 or less has
        // no ramp: levelRoughness is 1 everywhere), as the CPU bake's is.
        let roughness = level === 0 ? 0 : levelRoughness(size, level)
        for (let face = 0; face < 6; face++) {
          setDrawParams(cube, entry, { uFace: face, uRoughness: roughness })
          renderTarget(cube, face, level)
        }
      }
    },
    finish,
    dispose() {
      destroyTexture(finish())
    },
  }
}

/** The 1x1 black cube a scene binds as uEnv while it has no environment
 * (uEnvOn 0 makes the term vanish; the binding only keeps the sampler
 * complete). Owned by the scene, which destroys it with itself. */
export function createEnvironmentPlaceholder(label: string): TextureId {
  let black = new Uint8Array([0, 0, 0, 255])
  return createCubeTexture([black, black, black, black, black, black], 1, { autoFree: false, label })
}
