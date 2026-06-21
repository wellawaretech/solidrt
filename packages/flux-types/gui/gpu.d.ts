// Low-level GPU texture and fragment-shader globals (gui-enabled runtime only).
// Bare globals, not a `flux:*` module.

declare global {
  /**
   * Low-level GPU texture and fragment-shader access. Texture ids returned here
   * are used as texture sources in the render tree and as shader sampler inputs.
   * Available only on a gui-enabled runtime.
   */
  const gpu: {
    /**
     * Create an immutable texture from an RGBA8 pixel buffer (exactly
     * width*height*4 bytes). Returns the texture id.
     */
    createTexture(data: Uint8Array, width: number, height: number): number
    /**
     * Create a texture intended to be updated later via {@link uploadTexture}. The
     * seed buffer must hold at least one frame (width*height*4 bytes) and may hold
     * more (uploadTexture selects a frame by offset).
     */
    createMutableTexture(data: Uint8Array, width: number, height: number): number
    /**
     * Replace a mutable texture's pixels. `data` may hold several frames; `offset`
     * (default 0) selects which frame to upload.
     */
    uploadTexture(id: number, data: Uint8Array, offset?: number): void
    /** Destroy a texture (immutable, mutable, or shader). */
    destroyTexture(id: number): void
    /**
     * Compile a GLSL ES fragment shader into an offscreen texture of the given
     * size. `params` sets float uniforms by name; `textures` binds sampler2D
     * uniforms to texture ids. Returns the resulting texture id.
     */
    createShader(
      fragmentSrc: string,
      width: number,
      height: number,
      params?: Record<string, number>,
      textures?: Record<string, number>,
    ): number
    /** Update a shader texture's float uniforms by name and re-render it. */
    setShaderParams(id: number, params: Record<string, number>): void
  }
}

export {}