// CPU image codec: decode encoded image bytes into raw RGBA8 pixels (and, in
// future, encode them back). Kept separate from the GPU/texture APIs because no
// GPU is involved; pair decodeImage with createTexture from "@solidrt/core/gpu"
// to upload the result.

export type DecodedImage = {
  data: Uint8Array
  width: number
  height: number
}

/**
 * Decodes encoded image bytes (PNG, JPEG, and the other formats the runtime's
 * image decoder supports) into raw, tightly-packed RGBA8 pixels plus the
 * decoded dimensions. Feed the result straight into `createTexture`.
 */
export function decodeImage(bytes: Uint8Array): DecodedImage {
  return image.decodeImage(bytes)
}