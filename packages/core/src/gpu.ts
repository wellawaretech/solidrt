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

// The texture keeps reading from `data` (which may hold multiple frames):
// mutate it in place, then call uploadTexture to push the pixels to the GPU.
export function createMutableTexture(data: Uint8Array, width: number, height: number): number {
  return gpu.createMutableTexture(data, width, height)
}

export function uploadTexture(textureId: number, offset: number = 0): void {
  gpu.uploadTexture(textureId, offset)
}