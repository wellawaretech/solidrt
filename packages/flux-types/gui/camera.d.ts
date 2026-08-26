// Camera capture (gui-enabled runtime only). The imperative primitive;
// @solidrt/core's createCamera wraps it with SolidJS reactivity. `open` resolves
// to a bound session object, so the raw handle never leaves the runtime.

declare module "flux:camera" {
  import type { TextureId } from "flux:gpu"

  /** A camera device from {@link listCameras}. */
  type CameraDevice = {
    /** Device id to pass as `open({ camera })`. */
    id: number
    /** Human-readable device name. */
    name: string
    /** Which way the camera faces. */
    facing: "front" | "back" | "unknown"
  }

  /** Options for {@link open}. */
  type CameraOpenOptions = {
    /** Device id from {@link CameraDevice}. Omit to let the runtime choose. */
    camera?: number
    /** Prefer a front- or back-facing camera. */
    facing?: "front" | "back"
    /** Requested capture width in pixels. */
    width?: number
    /** Requested capture height in pixels. */
    height?: number
    /** Barcode formats to scan each frame for. Only "qr" is supported. */
    scan?: "qr"[]
  }

  /** A decoded barcode. */
  type Barcode = {
    /** The decoded payload. */
    data: string
    /** The barcode format (currently always "qr"). */
    format: "qr"
  }

  /** An opened camera session: the frame texture plus controls bound to it. */
  type CameraSession = {
    /** GPU texture id the latest frame is uploaded into (use as a texture source). */
    texture: TextureId
    /** Frame width in pixels. */
    width: number
    /** Frame height in pixels. */
    height: number
    /**
     * Register (or replace) the callback that receives decoded barcodes (only
     * fires when the session was opened with a `scan` option).
     */
    onBarcode(callback: (barcode: Barcode) => void): void
    /** Release the device. */
    close(): void
  }

  /**
   * List the available camera devices. The first call also starts the camera
   * subsystem, which comes up asynchronously: expect an empty list until the
   * initial cameraDeviceChange events arrive.
   */
  export function listCameras(): CameraDevice[]
  /**
   * Open a camera. Opening is also the permission request: the promise rejects
   * if permission is denied, and resolves once the first frame is ready. On
   * Linux a session that delivers neither within 10 seconds rejects with a
   * timeout error and releases the device (a wedged capture backend would
   * otherwise hold it and never settle). The first open also starts the
   * camera subsystem and waits for it (on every platform, up to 10 seconds,
   * then rejects with a timeout), so there is no need to poll listCameras
   * before opening.
   */
  export function open(options?: CameraOpenOptions): Promise<CameraSession>
  /**
   * One-shot scan of an RGBA8 pixel buffer (exactly width*height*4 bytes).
   * Returns every decoded barcode.
   */
  export function scanImage(data: Uint8Array, width: number, height: number): Barcode[]
}