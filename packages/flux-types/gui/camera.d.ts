// Camera capture globals (gui-enabled runtime only). These are bare globals, not
// a `flux:*` module. The helper types stay module-scoped (the trailing
// `export {}` makes this a module); only the `camera` global is exported.

/** A camera device from {@link camera.listCameras}. */
type CameraDevice = {
  /** Device id to pass as `open({ camera })`. */
  id: number
  /** Human-readable device name. */
  name: string
  /** Which way the camera faces. */
  facing: "front" | "back" | "unknown"
}

/** Options for {@link camera.open}. */
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

/** An opened camera: a session handle plus the GPU texture frames upload into. */
type CameraSession = {
  /** Session handle for `close` / `setBarcodeCallback`. */
  handle: number
  /** GPU texture id the latest frame is uploaded into (use as a texture source). */
  texture: number
  /** Frame width in pixels. */
  width: number
  /** Frame height in pixels. */
  height: number
}

/** A decoded barcode. */
type Barcode = {
  /** The decoded payload. */
  data: string
  /** The barcode format (currently always "qr"). */
  format: "qr"
}

declare global {
  /**
   * Camera capture. The lower-level primitive that `@solidrt/core`'s
   * `createCamera` wraps. Available only on a gui-enabled runtime.
   */
  const camera: {
    /** List the available camera devices. */
    listCameras(): CameraDevice[]
    /**
     * Open a camera. Opening is also the permission request: the promise rejects
     * if permission is denied, and resolves once the first frame is ready.
     */
    open(options?: CameraOpenOptions): Promise<CameraSession>
    /** Close a session by its `handle`. */
    close(handle: number): void
    /**
     * Register (or replace) the callback that receives decoded barcodes for a
     * session opened with a `scan` option.
     */
    setBarcodeCallback(handle: number, callback: (barcode: Barcode) => void): void
    /**
     * One-shot scan of an RGBA8 pixel buffer (exactly width*height*4 bytes).
     * Returns every decoded barcode.
     */
    scanImage(data: Uint8Array, width: number, height: number): Barcode[]
  }
}

export {}