/**
 * Uploads raw RGBA8 pixels to an immutable GPU texture and returns its id (use
 * it as `<texture src={id} />`). `data` must be exactly `width * height * 4`
 * bytes; a mismatch throws. For pixels you intend to mutate and re-upload, use
 * `createMutableTexture` instead.
 */
export function createTexture(data: Uint8Array, width: number, height: number): number {
  return gpu.createTexture(data, width, height)
}

/**
 * Creates a GPU texture that keeps reading from `data` (which may hold multiple
 * frames): mutate the buffer in place, then call `uploadTexture` to push the
 * pixels to the GPU. Like `createTexture`, `data` is RGBA8 and must hold at
 * least `width * height * 4` bytes.
 */
export function createMutableTexture(data: Uint8Array, width: number, height: number): number {
  return gpu.createMutableTexture(data, width, height)
}

/**
 * Pushes the current contents of a mutable texture's backing buffer to the GPU.
 * `offset` is a byte offset into that buffer, selecting which frame to upload
 * when the buffer holds several (default 0, the first frame).
 */
export function uploadTexture(textureId: number, offset: number = 0): void {
  gpu.uploadTexture(textureId, offset)
}

/**
 * Compiles a GLSL ES 3.00 fragment shader and renders it into a texture,
 * returning the texture id (usable anywhere a normal texture id is, e.g.
 * `<texture src>`). The fragment body may reference `vUV` (0..1, top-left
 * origin), `iResolution`, `iTime`, and any `uniform float` it declares; pass
 * their values via `params`. `textures` binds each declared `uniform sampler2D`
 * to an existing texture id (e.g. a camera or decoded image) so the shader can
 * read it; those inputs are re-sampled on every `setShaderParams` call, so live
 * sources stay current.
 */
export function createShader(
  fragmentSrc: string,
  width: number,
  height: number,
  params?: Record<string, number>,
  textures?: Record<string, number>,
): number {
  return gpu.createShader(fragmentSrc, width, height, params, textures)
}

/**
 * Re-renders an existing shader texture with new param values and requests a
 * frame. Use this to animate (e.g. update `iTime` each frame).
 */
export function setShaderParams(textureId: number, params: Record<string, number>): void {
  gpu.setShaderParams(textureId, params)
}