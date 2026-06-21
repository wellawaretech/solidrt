// GPU textures and shaders, reactive (SolidJS) layer: the create* helpers free
// their texture automatically when the reactive owner is disposed. The imperative
// primitive lives in the `flux:gpu` module; import { uploadTexture,
// setShaderParams, destroyTexture, ... } from "flux:gpu" for non-reactive use.

import { getOwner, onCleanup } from "@solidjs/signals"
import * as gpu from "flux:gpu"

/**
 * Uploads raw RGBA8 pixels to an immutable GPU texture and returns its id (use
 * it as `<texture src={id} />`). `data` must be exactly `width * height * 4`
 * bytes; a mismatch throws. For pixels you intend to mutate and re-upload, use
 * `createMutableTexture` instead. When called inside a reactive scope the
 * texture is freed automatically once that owner is disposed; when called
 * outside one (e.g. after an `await`, where the owner is no longer current)
 * nothing is registered and you must call `destroyTexture` (from flux:gpu)
 * yourself.
 */
export function createTexture(data: Uint8Array, width: number, height: number): number {
  let id = gpu.createTexture(data, width, height)
  if (getOwner()) onCleanup(() => gpu.destroyTexture(id))
  return id
}

/**
 * Creates a GPU texture you intend to update over time: seed it with `data`,
 * then call `uploadTexture(id, data)` (from flux:gpu) to push new pixels. `data`
 * is RGBA8 and must hold at least `width * height * 4` bytes (it may hold several
 * frames). Like `createTexture`, the texture is freed automatically when the
 * reactive owner is disposed; created outside a reactive scope you must call
 * `destroyTexture` (from flux:gpu) yourself.
 */
export function createMutableTexture(data: Uint8Array, width: number, height: number): number {
  let id = gpu.createMutableTexture(data, width, height)
  if (getOwner()) onCleanup(() => gpu.destroyTexture(id))
  return id
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