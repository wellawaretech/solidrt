declare module "flux:image" {
  /**
   * Decoded pixels: tightly-packed RGBA8 plus the pixel dimensions. Alpha is
   * premultiplied unless the call that produced them said otherwise.
   */
  export type DecodedImage = {
    data: Uint8Array
    width: number
    height: number
  }

  /**
   * Which alpha convention a pixel buffer follows. Image files store
   * `"straight"` alpha; every texture and target on the GPU is
   * `"premultiplied"` (color already multiplied by alpha), and so is what
   * `readTexture` / `captureSnapshot` hand back.
   */
  export type AlphaMode = "premultiplied" | "straight"

  /**
   * Decodes encoded image bytes (png, jpeg, webp, gif, bmp, ico) into raw
   * RGBA8 pixels plus the decoded dimensions. Synchronous, pure CPU. Throws
   * when the bytes are not a decodable image.
   *
   * `alpha` selects what comes out: `"premultiplied"` (default) is ready for
   * `createTexture` as-is; `"straight"` is the file's pixels verbatim, for CPU
   * processing that wants color under transparent pixels preserved. Opaque
   * pixels are identical either way.
   */
  export function decodeImage(bytes: Uint8Array, options?: { alpha?: AlphaMode }): DecodedImage

  /**
   * Encodes raw RGBA8 pixels into an image file, the reverse of `decodeImage`
   * (`encodeImage(decodeImage(bytes))` round-trips: exactly for opaque and
   * fully transparent pixels, within rounding for translucent ones). `format`
   * defaults to `"png"` (lossless, keeps alpha); `"jpeg"` drops the alpha
   * channel and takes `quality` in 0..1 (default 0.9, ignored for png).
   * `alpha` names what `img.data` holds: `"premultiplied"` (default, a decode
   * or a readback) is converted to the straight alpha PNG stores;
   * `"straight"` is written verbatim. Throws when `data.length` does not
   * match `width * height * 4`.
   */
  export function encodeImage(
    img: DecodedImage,
    options?: { format?: "png" | "jpeg"; quality?: number; alpha?: AlphaMode },
  ): Uint8Array

  /**
   * A transcoded compressed texture, shaped as the arguments of
   * `createTexture` in flux:gpu: `createTexture(t.data, t.width, t.height,
   * { format: t.format, mipmap: t.mipmap })`. `data` holds every level of
   * the file, level 0 first; `mipmap` says it is the full chain down to
   * 1x1 rather than the base level alone. `format` is the target with the
   * "-srgb" qualifier when the file's texels are sRGB-encoded (the file
   * says, not the caller).
   */
  export type TranscodedTexture = {
    data: Uint8Array
    width: number
    height: number
    format: "etc2-rgba8" | "etc2-rgba8-srgb" | "bc7-rgba8" | "bc7-rgba8-srgb" | "rgba8" | "rgba8-srgb"
    mipmap: boolean
  }

  /**
   * The Basis Universal codec inside a KTX2 file. "etc1s" is the small one:
   * lossy at JPEG-class quality, around 1 bit per texel, for color maps
   * (base color, emissive). "uastc" is the high quality one, for normal
   * maps and data maps, where ETC1S artifacts show: 8 bits per texel
   * before the file's zstd compression, which takes it to 2 to 6 on a
   * real map at the default quality.
   */
  export type TextureCodec = "etc1s" | "uastc"

  /**
   * Transcodes a KTX2 file of Basis Universal texel data (ETC1S or UASTC, a
   * 2D texture with one level or its full mip chain - what `encodeTexture`
   * writes and what glTF's KHR_texture_basisu carries) into the block
   * format a GPU samples. This is the load-time half of compressed
   * textures: an app ships ONE file for every platform and each device
   * turns it into its own native format, a quarter of the GPU memory of
   * the same image as "rgba8".
   *
   * Without `target` the runtime picks this device's format: "bc7-rgba8"
   * where the GPU reports it (`limits.bc7Textures` in flux:gpu, every
   * desktop), "etc2-rgba8" everywhere else (GLES 3.0 core, every mobile
   * GPU). A runtime with no GPU cannot pick and throws; name the target
   * there. "rgba8" is the uncompressed form, for inspection and comparison;
   * no device needs it as a fallback.
   *
   * The texels come through as the file holds them: a file baked here is
   * premultiplied like every texture, a file from elsewhere (glTF stores
   * straight alpha) samples as straight alpha.
   *
   * Asynchronous: the work runs off the JS thread, and a scene's worth of
   * textures is seconds of it on a slow device. Start as many as there are
   * textures: the runtime transcodes four at a time and queues the rest, so
   * a `Promise.all` over a scene neither floods the device's cores nor
   * multiplies the memory in flight. Rejects when the bytes are
   * not such a file (another codec, a cube map, an array, a partial mip
   * chain). Throws on a runtime built without compressed textures
   * (`Flux.capabilities` lacks "ktx2").
   */
  export function transcodeTexture(
    ktx2: Uint8Array,
    options?: { target?: "etc2-rgba8" | "bc7-rgba8" | "rgba8" },
  ): Promise<TranscodedTexture>

  /**
   * Encodes RGBA8 pixels into a KTX2 file of Basis Universal texel data,
   * the bake-time half of compressed textures. The file is device
   * independent - the same bytes whatever machine encodes it and whatever
   * device loads it - and `transcodeTexture` turns it into each device's
   * format at load.
   *
   * `img` holds the pixels as the GPU will sample them, which means
   * premultiplied: pass a `decodeImage` result as it is (blocks cannot be
   * premultiplied after the fact, and mips only filter correctly on
   * premultiplied color). `codec` is required. `srgb` (default false) says
   * the pixels are sRGB-encoded color: set it for base color and emissive
   * maps, never for normal or data maps; it flags the file, makes the
   * error metric perceptual and filters the mips in linear light. `mipmap`
   * (default false) generates and stores the full mip chain, filtered
   * across the edges as `wrap` says ("clamp" default, or "repeat" for a
   * texture sampled tiled). `quality` is 0..1 (default 0.5), higher is
   * larger and closer to the source. For "uastc", 1 is the encoder's best
   * blocks and anything below it trades block error for a file that
   * compresses better.
   *
   * Asynchronous and slow by design (seconds for a large image, on every
   * core): bake-time work, never something to do at load. Rejects when
   * `data.length` does not match `width * height * 4`. Throws on a runtime
   * built without compressed textures.
   */
  export function encodeTexture(
    img: DecodedImage,
    options: { codec: TextureCodec; srgb?: boolean; mipmap?: boolean; wrap?: "clamp" | "repeat"; quality?: number },
  ): Promise<Uint8Array>
}
