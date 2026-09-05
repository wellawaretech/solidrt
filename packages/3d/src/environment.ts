// Environment helpers: the baked HDR environment loader (the .srte chain
// `srt tool 3d/environment` writes), the equirectangular-to-cube
// conversion a fetched LDR panorama needs at runtime, and the placeholder
// cube a scene binds as uEnv while no environment is set.

import { file } from "flux:fs"
import { createCubeTexture, createShaderTexture, destroyTexture, glsl, readTexture } from "@solidrt/core/gpu"
import type { CreateOptions, SamplerOptions, TextureFormatOptions, TextureId } from "@solidrt/core/gpu"
import { decodeEnvironment } from "./environment-bake.ts"
import { SRGB } from "./glsl.ts"

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

// One face of the cube: the texel's sampling direction from the GL
// cube-map table (t = 0 the first row), so the face holds what a lookup
// of that world direction returns. The panorama's center column faces -Z (the
// camera's default forward; Godot's PanoramaSkyMaterial and Unity's
// Skybox/Panoramic agree; Three centers +X, a quarter turn away), its top
// row +Y. At the seam column the uv derivative jumps by a full turn;
// zeroing that component keeps a mipmapped source from drawing a line.
// An sRGB source samples decoded; uEncode re-encodes the face so the
// cube, created at the same format, decodes the same way.
const EQUIRECT_FACE = glsl`
  uniform sampler2D uMap;
  uniform float uFace;
  uniform float uEncode;
  ${SRGB}
  const float PI = 3.14159265358979;
  // A uv derivative larger than this is the seam wrap, not a real step.
  const float SEAM_JUMP = 0.5;
  void main() {
    int face = int(uFace + 0.5);
    float a = 2.0 * vUV.x - 1.0;
    float b = 2.0 * vUV.y - 1.0;
    vec3 d;
    if (face == 0) d = vec3(1.0, -b, -a);
    else if (face == 1) d = vec3(-1.0, -b, a);
    else if (face == 2) d = vec3(a, 1.0, b);
    else if (face == 3) d = vec3(a, -1.0, -b);
    else if (face == 4) d = vec3(a, -b, 1.0);
    else d = vec3(-a, -b, -1.0);
    d = normalize(d);
    vec2 uv = vec2(atan(d.x, -d.z) / (2.0 * PI) + 0.5, acos(clamp(d.y, -1.0, 1.0)) / PI);
    vec2 dx = dFdx(uv);
    vec2 dy = dFdy(uv);
    if (abs(dx.x) > SEAM_JUMP) dx.x = 0.0;
    if (abs(dy.x) > SEAM_JUMP) dy.x = 0.0;
    vec4 c = textureGrad(uMap, uv, dx, dy);
    fragColor = uEncode > 0.5 ? vec4(linearToSrgb(c.rgb), c.a) : c;
  }
`

/**
 * Convert an equirectangular panorama (an uploaded 2D texture: createImage,
 * createTexture) into a cube map of `size` x `size` faces, returned as a
 * cube TextureId ready for a skybox or the environment - Three's
 * `WebGLCubeRenderTarget.fromEquirectangularTexture`, Unity's cube import
 * of a lat-long image. The six faces render on the GPU and are read back
 * and uploaded once, synchronously: a few milliseconds, the same cost as
 * the upload itself. `opts` are createCubeTexture's (`mipmap: true` for an
 * environment shininess can blur; `label`; `autoFree: false` to own it),
 * and `format` names the PANORAMA's format so the cube decodes like it:
 * "rgba8-srgb" for a photographed sky uploaded as such (the faces are
 * re-encoded), "rgba8" (default) for data. An HDR ("rgba16f") panorama
 * has no runtime path here - the face passes render rgba8 - so it throws;
 * bake it with `srt tool 3d/environment` and loadEnvironment it.
 * The panorama's center column faces -Z and its top row is +Y. Leave its
 * wrap at the default clamp: `repeat` would also wrap vertically and
 * bleed the poles across the top and bottom rows, while the clamped seam
 * column costs at most a texel-wide blend at +Z. Three centers its
 * panoramas on +X: a rotation tuned there differs by a quarter turn here.
 */
export function equirectToCube(map: TextureId, size: number, opts?: CreateOptions & SamplerOptions & TextureFormatOptions): TextureId {
  if (!Number.isInteger(size) || size < 1) throw new Error("equirectToCube: size must be a positive integer, got " + size)
  let format = opts?.format ?? "rgba8"
  if (format !== "rgba8" && format !== "rgba8-srgb") {
    throw new Error('equirectToCube: format must be "rgba8" or "rgba8-srgb" (the face passes render rgba8), got ' + format)
  }
  let encode = format === "rgba8-srgb" ? 1 : 0
  let faces: Uint8Array[] = []
  for (let face = 0; face < 6; face++) {
    let target = createShaderTexture(EQUIRECT_FACE, size, size, { uFace: face, uEncode: encode }, {
      textures: { uMap: map },
      autoFree: false,
      label: (opts?.label ?? "equirect") + "-face-" + face,
    })
    faces.push(readTexture(target).data)
    destroyTexture(target)
  }
  return createCubeTexture(faces, size, opts)
}

let placeholder: TextureId | undefined

/** The 1x1 black cube a scene binds as uEnv while it has no environment
 * (uEnvOn 0 makes the term vanish; the binding only keeps the sampler
 * complete). App-lifetime, shared by every scene. */
export function environmentPlaceholder(): TextureId {
  if (placeholder === undefined) {
    let black = new Uint8Array([0, 0, 0, 255])
    placeholder = createCubeTexture([black, black, black, black, black, black], 1, { autoFree: false, label: "scene-env-none" })
  }
  return placeholder
}
