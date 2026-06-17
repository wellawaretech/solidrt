// Camera capture. A camera streams into a texture id, so a viewfinder is just
// <texture src={cam.texture} />. Opening the camera IS the permission request
// (SDL semantics): the promise resolves once the stream is configured and
// rejects if the user denies access.

import { createSignal, onCleanup } from "@solidjs/signals"
import { on } from "srt:events"

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

let devicesAccessor: (() => CameraInfo[]) | undefined

/**
 * Current camera list as a reactive accessor: re-enumerates on hotplug.
 * Also initializes the camera subsystem (required before hotplug events fire).
 * App-lifetime: there is one camera subsystem, so no cleanup is needed.
 *
 * Coverage caveat (SDL 3.4.8): only Android delivers both add and remove. On
 * Linux you get add events but not remove (removal is broken upstream);
 * on macOS/Windows there is no camera hotplug at all.
 */
export function cameraDevices(): CameraInfo[] {
  if (!devicesAccessor) {
    let [devices, setDevices] = createSignal<CameraInfo[]>(listCameras())
    on("cameraDeviceChange", () => setDevices(listCameras()))
    devicesAccessor = devices
  }
  return devicesAccessor()
}

/**
 * Low-level fallback: subscribe to camera hotplug events with a callback.
 * Prefer `cameraDevices()` unless the reactive accessor does not fit your use
 * case. Re-enumerate with `listCameras()` inside the callback to get the new
 * device set. Events only flow once the camera subsystem is up (after the first
 * `listCameras()` or `openCamera()` call). Returns an unsubscribe function.
 *
 * Coverage caveat (SDL 3.4.8): only Android delivers both add and remove. On
 * Linux you get `added=true` but not `added=false` (removal is broken upstream);
 * on macOS/Windows there is no camera hotplug at all, so nothing fires.
 */
export function onDeviceChange(callback: (event: { added: boolean }) => void): () => void {
  return on("cameraDeviceChange", callback)
}

/**
 * One-shot scan of an RGBA8 pixel buffer for QR codes; composes with
 * `decodeImage`: `scanBarcodes(img.data, img.width, img.height)`.
 */
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

/** A live camera as reactive accessors. */
export type CameraStream = {
  /** Texture id once the stream is up, undefined while opening; render with <texture src={...}>. */
  texture(): number | undefined
  /** Actual stream size, undefined while opening. */
  width(): number | undefined
  height(): number | undefined
  /** The most recently decoded barcode (requires the scan option). */
  barcode(): BarcodeResult | undefined
  /** Set if opening failed (e.g. permission denied). */
  error(): Error | undefined
}

/**
 * Opens a camera and exposes it as reactive signals: read texture() in JSX and
 * it appears once the stream is configured. Closes automatically when the
 * reactive owner is disposed (e.g. the component unmounts). The lower-level
 * openCamera() is the imperative alternative.
 */
export function createCamera(options: CameraOptions = {}): CameraStream {
  let [texture, setTexture] = createSignal<number | undefined>(undefined)
  let [width, setWidth] = createSignal<number | undefined>(undefined)
  let [height, setHeight] = createSignal<number | undefined>(undefined)
  let [barcode, setBarcode] = createSignal<BarcodeResult | undefined>(undefined)
  let [error, setError] = createSignal<Error | undefined>(undefined)
  let handle: number | undefined
  let disposed = false

  camera
    .open(options)
    .then((opened) => {
      if (disposed) {
        camera.close(opened.handle)
        return
      }
      handle = opened.handle
      if (options.scan) camera.setBarcodeCallback(opened.handle, (result) => setBarcode(result))
      setTexture(opened.texture)
      setWidth(opened.width)
      setHeight(opened.height)
    })
    .catch((e) => setError(e instanceof Error ? e : new Error(String(e))))

  onCleanup(() => {
    disposed = true
    if (handle !== undefined) {
      camera.close(handle)
      handle = undefined
    }
  })

  return { texture, width, height, barcode, error }
}