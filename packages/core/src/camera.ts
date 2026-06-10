// Camera capture. A camera streams into a texture id, so a viewfinder is just
// <texture src={cam.texture} />. Opening the camera IS the permission request
// (SDL semantics): the promise resolves once the stream is configured and
// rejects if the user denies access.

export type CameraFacing = "front" | "back" | "unknown"

export type CameraInfo = {
  id: number
  name: string
  facing: CameraFacing
}

export type BarcodeFormat = "qr"

export type BarcodeResult = {
  data: string
  format: BarcodeFormat
}

export type CameraOptions = {
  /** Explicit device id from listCameras(); takes precedence over facing. */
  camera?: number
  /** Pick the first camera with this facing (falls back to the first camera). */
  facing?: "front" | "back"
  /** Size hint; the device picks the closest supported mode. */
  width?: number
  height?: number
  /** Decode these barcode formats from the stream (delivered via onBarcode). */
  scan?: BarcodeFormat[]
}

export type Camera = {
  /** Texture id updated every frame while open; render with <texture src={...}>. */
  texture: number
  /** Actual stream size (may differ from the requested hint). */
  width: number
  height: number
  /** Receive decoded barcodes (requires the scan option; replaces any previous callback). */
  onBarcode(callback: (result: BarcodeResult) => void): void
  /** Release the device. The texture keeps showing the last frame. */
  close(): void
}

export function listCameras(): CameraInfo[] {
  return camera.listCameras()
}

// One-shot scan of an RGBA8 pixel buffer for QR codes; composes with
// decodeImage: scanBarcodes(img.data, img.width, img.height).
export function scanBarcodes(data: Uint8Array, width: number, height: number): BarcodeResult[] {
  return camera.scanImage(data, width, height)
}

export async function openCamera(options: CameraOptions = {}): Promise<Camera> {
  let opened = await camera.open(options)
  return {
    texture: opened.texture,
    width: opened.width,
    height: opened.height,
    onBarcode: (callback: (result: BarcodeResult) => void) => camera.setBarcodeCallback(opened.handle, callback),
    close: () => camera.close(opened.handle),
  }
}

export { CameraView, type CameraViewProps } from "./camera-view"