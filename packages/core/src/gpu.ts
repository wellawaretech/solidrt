export type DecodedImage = {
  data: Uint8Array
  width: number
  height: number
}

export function decodeImage(bytes: Uint8Array): DecodedImage {
  return gpu.decodeImage(bytes)
}

export function createTexture(data: Uint8Array, width: number, height: number): number {
  return gpu.createTexture(data, width, height)
}

// The texture keeps reading from `data` (which may hold multiple frames):
// mutate it in place, then call uploadTexture to push the pixels to the GPU.
export function createMutableTexture(data: Uint8Array, width: number, height: number): number {
  return gpu.createMutableTexture(data, width, height)
}

export function uploadTexture(textureId: number, offset: number = 0): void {
  gpu.uploadTexture(textureId, offset)
}

// Compile a GLSL ES 3.00 fragment shader and render it into a texture, returning
// the texture id (usable anywhere a normal texture id is, e.g. <texture src>).
// The fragment body may reference vUV (0..1, top-left origin), iResolution,
// iTime, and any `uniform float` it declares; pass their values via `params`.
// `textures` binds each declared `uniform sampler2D` to an existing texture id
// (e.g. a camera or decoded image) so the shader can read it; those inputs are
// re-sampled on every setShaderParams call, so live sources stay current.
export function createShader(
  fragmentSrc: string,
  width: number,
  height: number,
  params?: Record<string, number>,
  textures?: Record<string, number>,
): number {
  return gpu.createShader(fragmentSrc, width, height, params, textures)
}

// Re-render an existing shader texture with new param values and request a
// frame. Use this to animate (e.g. update iTime each frame).
export function setShaderParams(textureId: number, params: Record<string, number>): void {
  gpu.setShaderParams(textureId, params)
}