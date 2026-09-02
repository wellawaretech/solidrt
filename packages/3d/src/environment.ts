// Environment helpers: the equirectangular-to-cube conversion a fetched
// panorama needs at runtime (the build-time form belongs to the srt asset
// pipeline; see okf/backlog/3d-environment.md), and the placeholder cube a
// scene binds as uEnv while no environment is set.

import { createCubeTexture, createShaderTexture, destroyTexture, glsl, readTexture } from "@solidrt/core/gpu"
import type { CreateOptions, SamplerOptions, TextureId } from "@solidrt/core/gpu"

// One face of the cube: the texel's sampling direction from the GL
// cube-map table (t = 0 the first row), with the x flip CUBE_LOOKUP
// applies at lookup, so the face holds what a lookup of that world
// direction should find. The panorama's center column faces -Z (the
// camera's default forward; Godot's PanoramaSkyMaterial and Unity's
// Skybox/Panoramic agree; Three centers +X, a quarter turn away), its top
// row +Y. At the seam column the uv derivative jumps by a full turn;
// zeroing that component keeps a mipmapped source from drawing a line.
const EQUIRECT_FACE = glsl`
  uniform sampler2D uMap;
  uniform float uFace;
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
    d.x = -d.x;
    d = normalize(d);
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
 * of a lat-long image. The six faces render on the GPU and are read back
 * and uploaded once, synchronously: a few milliseconds, the same cost as
 * the upload itself. `opts` are createCubeTexture's (`mipmap: true` for an
 * environment shininess can blur; `label`; `autoFree: false` to own it).
 * The panorama's center column faces -Z and its top row is +Y. Leave its
 * wrap at the default clamp: `repeat` would also wrap vertically and
 * bleed the poles across the top and bottom rows, while the clamped seam
 * column costs at most a texel-wide blend at +Z. Three centers its
 * panoramas on +X: a rotation tuned there differs by a quarter turn here.
 */
export function equirectToCube(map: TextureId, size: number, opts?: CreateOptions & SamplerOptions): TextureId {
  if (!Number.isInteger(size) || size < 1) throw new Error("equirectToCube: size must be a positive integer, got " + size)
  let faces: Uint8Array[] = []
  for (let face = 0; face < 6; face++) {
    let target = createShaderTexture(EQUIRECT_FACE, size, size, { uFace: face }, {
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
