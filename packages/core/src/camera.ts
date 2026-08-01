// Camera capture, reactive (SolidJS) layer. A camera streams into a texture id,
// so a viewfinder is just <texture src={cam.texture} />. Opening the camera IS
// the permission request (SDL semantics): it resolves once the stream is
// configured and rejects if the user denies access.
//
// The imperative primitive lives in the `flux:camera` module; import { open,
// listCameras, scanImage } from "flux:camera" for non-reactive use.

import { createSignal, onCleanup } from "@solidjs/signals"
import { listCameras, open } from "flux:camera"
import type { TextureId } from "flux:gpu"
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
  /** Explicit device id from flux:camera listCameras(); takes precedence over facing. */
  camera?: number
  /** Pick the first camera with this facing (falls back to the first camera). */
  facing?: "front" | "back"
  /** Size hint; the device picks the closest supported mode. */
  width?: number
  height?: number
  /** Decode these barcode formats from the stream (delivered via the barcode signal). */
  scan?: BarcodeFormat[]
}

let devicesAccessor: (() => CameraInfo[]) | undefined

/**
 * Current camera list as a reactive accessor: re-enumerates on hotplug. Also
 * kicks off the camera subsystem (required before hotplug events fire), which
 * starts asynchronously: the list is empty until the initial device events
 * arrive - moments later normally, never if the platform's capture backend is
 * wedged. App-lifetime: there is one camera subsystem, so no cleanup is
 * needed.
 *
 * Coverage caveat (SDL 3.4.8): only Android delivers both add and remove. On
 * Linux you get add events but not remove (removal is broken upstream); on
 * macOS/Windows there is no camera hotplug at all.
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
 * Low-level fallback: subscribe to camera hotplug events with a callback. Prefer
 * `cameraDevices()` unless the reactive accessor does not fit. Re-enumerate with
 * `listCameras()` (from flux:camera) inside the callback to get the new set.
 * Returns an unsubscribe function.
 */
export function onDeviceChange(callback: (event: { added: boolean }) => void): () => void {
  return on("cameraDeviceChange", callback)
}

/** A live camera as reactive accessors. */
export type CameraStream = {
  /** Texture id once the stream is up, undefined while opening; render with <texture src={...}>. */
  texture(): TextureId | undefined
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
 * reactive owner is disposed (e.g. the component unmounts). For imperative use,
 * call open() from "flux:camera" directly.
 */
export function createCamera(options: CameraOptions = {}): CameraStream {
  let [texture, setTexture] = createSignal<TextureId | undefined>(undefined)
  let [width, setWidth] = createSignal<number | undefined>(undefined)
  let [height, setHeight] = createSignal<number | undefined>(undefined)
  let [barcode, setBarcode] = createSignal<BarcodeResult | undefined>(undefined)
  let [error, setError] = createSignal<Error | undefined>(undefined)
  let session: Awaited<ReturnType<typeof open>> | undefined
  let disposed = false

  open(options)
    .then((cam) => {
      if (disposed) {
        cam.close()
        return
      }
      session = cam
      if (options.scan) cam.onBarcode((result) => setBarcode(result))
      setTexture(cam.texture)
      setWidth(cam.width)
      setHeight(cam.height)
    })
    .catch((e) => setError(e instanceof Error ? e : new Error(String(e))))

  onCleanup(() => {
    disposed = true
    if (session) {
      session.close()
      session = undefined
    }
  })

  return { texture, width, height, barcode, error }
}