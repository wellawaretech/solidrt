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
}
