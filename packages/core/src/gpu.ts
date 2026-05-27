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