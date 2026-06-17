import { getOwner, onCleanup } from "@solidjs/signals"

/**
 * Uploads raw RGBA8 pixels to an immutable GPU texture and returns its id (use
 * it as `<texture src={id} />`). `data` must be exactly `width * height * 4`
 * bytes; a mismatch throws. For pixels you intend to mutate and re-upload, use
 * `createMutableTexture` instead. When called inside a reactive scope the
 * texture is freed automatically once that owner is disposed; when called
 * outside one (e.g. after an `await`, where the owner is no longer current)
 * nothing is registered and you must call `destroyTexture` yourself.
 */
export function createTexture(data: Uint8Array, width: number, height: number): number {
  let id = gpu.createTexture(data, width, height)
  if (getOwner()) onCleanup(() => gpu.destroyTexture(id))
  return id
}

/**
 * Creates a GPU texture that keeps reading from `data` (which may hold multiple
 * frames): mutate the buffer in place, then call `uploadTexture` to push the
 * pixels to the GPU. Like `createTexture`, `data` is RGBA8 and must hold at
 * least `width * height * 4` bytes, and the texture is freed automatically when
 * the reactive owner is disposed; created outside a reactive scope you must
 * call `destroyTexture` yourself.
 */
export function createMutableTexture(data: Uint8Array, width: number, height: number): number {
  let id = gpu.createMutableTexture(data, width, height)
  if (getOwner()) onCleanup(() => gpu.destroyTexture(id))
  return id
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
 * Frees a GPU texture and its associated resources. When
 * createTexture/createMutableTexture/createShader run inside a reactive scope
 * this is called automatically once that owner is disposed; call it explicitly
 * when a texture is created outside the reactive graph (e.g. after an `await`)
 * or when managing its lifetime manually.
 */
export function destroyTexture(textureId: number): void {
  gpu.destroyTexture(textureId)
}

/**
 * Compiles a GLSL ES 3.00 fragment shader and renders it into a texture,
 * returning the texture id (usable anywhere a normal texture id is, e.g.
 * `<texture src>`). The fragment body may reference `vUV` (0..1, top-left
 * origin), `iResolution`, `iTime`, and any `uniform float` it declares; pass
 * their values via `params`. `textures` binds each declared `uniform sampler2D`
 * to an existing texture id (e.g. a camera or decoded image) so the shader can
 * read it; those inputs are re-sampled on every `setShaderParams` call, so live
 * sources stay current. Frees the texture and shader program when the reactive
 * owner is disposed; create outside any reactive scope for app-lifetime shaders.
 */
export function createShader(
  fragmentSrc: string,
  width: number,
  height: number,
  params?: Record<string, number>,
  textures?: Record<string, number>,
): number {
  let id = gpu.createShader(fragmentSrc, width, height, params, textures)
  if (getOwner()) onCleanup(() => gpu.destroyTexture(id))
  return id
}

/**
 * Re-renders an existing shader texture with new param values and requests a
 * frame. Use this to animate (e.g. update `iTime` each frame).
 */
export function setShaderParams(textureId: number, params: Record<string, number>): void {
  gpu.setShaderParams(textureId, params)
}