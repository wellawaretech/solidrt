// Low-level GPU textures and fragment shaders (gui-enabled runtime only). The
// imperative primitive; @solidrt/core's gpu helpers add reactive auto-cleanup on
// top. Texture ids are the public token (used as `<texture src>` and shader
// sampler inputs), so there is no handle to hide here.

declare module "flux:gpu" {
  /**
   * Create an immutable texture from an RGBA8 pixel buffer (exactly
   * width*height*4 bytes). Returns the texture id.
   */
  export function createTexture(data: Uint8Array, width: number, height: number): number
  /**
   * Create a texture intended to be updated later via {@link uploadTexture}. The
   * seed buffer must hold at least one frame (width*height*4 bytes) and may hold
   * more (uploadTexture selects a frame by offset).
   */
  export function createMutableTexture(data: Uint8Array, width: number, height: number): number
  /**
   * Replace a mutable texture's pixels. `data` may hold several frames; `offset`
   * (default 0) selects which frame to upload.
   */
  export function uploadTexture(id: number, data: Uint8Array, offset?: number): void
  /** Destroy a texture (immutable, mutable, or shader). */
  export function destroyTexture(id: number): void
  /**
   * Compile a GLSL ES fragment shader into an offscreen texture of the given
   * size. `params` sets float uniforms by name; `textures` binds sampler2D
   * uniforms to texture ids. Returns the resulting texture id.
   */
  export function createShader(
    fragmentSrc: string,
    width: number,
    height: number,
    params?: Record<string, number>,
    textures?: Record<string, number>,
  ): number
  /** Update a shader texture's float uniforms by name and re-render it. */
  export function setShaderParams(id: number, params: Record<string, number>): void
}