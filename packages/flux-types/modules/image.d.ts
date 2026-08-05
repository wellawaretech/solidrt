declare module "flux:image" {
  /** Decoded pixels: tightly-packed RGBA8 plus the pixel dimensions. */
  export type DecodedImage = {
    data: Uint8Array
    width: number
    height: number
  }

  /**
   * Decodes encoded image bytes (png, jpeg, webp, gif, bmp, ico) into raw
   * RGBA8 pixels plus the decoded dimensions. Synchronous, pure CPU. Throws
   * when the bytes are not a decodable image.
   */
  export function decodeImage(bytes: Uint8Array): DecodedImage

  /**
   * Encodes raw RGBA8 pixels into an image file, the reverse of `decodeImage`
   * (`encodeImage(decodeImage(bytes))` round-trips). `format` defaults to
   * `"png"` (lossless, keeps alpha); `"jpeg"` drops the alpha channel and
   * takes `quality` in 0..1 (default 0.9, ignored for png). Throws when
   * `data.length` does not match `width * height * 4`.
   */
  export function encodeImage(img: DecodedImage, options?: { format?: "png" | "jpeg"; quality?: number }): Uint8Array
}
