import { createSignal, onCleanup } from "@solidjs/signals"
import { openCamera, type BarcodeResult, type Camera } from "./camera"

// Convenience viewfinder over openCamera: opens on mount, renders the stream
// texture, closes on cleanup. Use openCamera directly for anything it does not
// cover; this is just composition, no extra capability.

export interface CameraViewProps {
  /** Explicit device id from listCameras(); takes precedence over facing. */
  camera?: number
  facing?: "front" | "back"
  /**
   * Size hint for the stream and explicit size of the view. Omit height to
   * follow the stream's aspect ratio, which can flip when a phone rotates.
   */
  width?: number
  height?: number
  scan?: "qr"[]
  onReady?: (cam: Camera) => void
  onError?: (error: Error) => void
  onBarcode?: (result: BarcodeResult) => void
}

export function CameraView(props: CameraViewProps) {
  let [texture, setTexture] = createSignal<number | undefined>(undefined)
  let cam: Camera | undefined
  let disposed = false

  openCamera({
    camera: props.camera,
    facing: props.facing,
    width: props.width,
    height: props.height,
    scan: props.scan,
  })
    .then((opened) => {
      if (disposed) {
        opened.close()
        return
      }
      cam = opened
      if (props.onBarcode) opened.onBarcode(props.onBarcode)
      setTexture(opened.texture)
      props.onReady?.(opened)
    })
    .catch((e) => props.onError?.(e instanceof Error ? e : new Error(String(e))))

  onCleanup(() => {
    disposed = true
    cam?.close()
    cam = undefined
  })

  return <texture src={texture()} width={props.width} height={props.height} />
}