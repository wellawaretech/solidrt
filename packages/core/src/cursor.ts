// Image cursors: decode once, register with the platform, reference from any
// element's `cursor` prop by handle. The named shapes need no setup
// (`cursor="pointer"`); this is the `url()` half of CSS `cursor` through the
// SolidRT lens: a handle created once rather than a URL resolved per element,
// since the platform holds the image and the app decodes it.

import { getOwner, onCleanup } from "@solidjs/signals"
import { decodeImage } from "flux:image"
import {
  createCursor as registerCursor,
  dropCursor,
  type CursorFrame as NativeFrame,
  type CursorImage as NativeImage,
} from "flux:rendertree"

/** A registered image cursor (createCursor), for an element's `cursor` prop. */
export type CursorId = number & { readonly __cursor: unique symbol }

/**
 * The named cursors: the CSS `cursor` keywords the platform has a shape for,
 * plus `"none"` to hide the cursor. Keywords without a platform shape
 * (grab, zoom-in, ...) are image cursors here (createCursor).
 */
export type CursorName =
  | "default"
  | "pointer"
  | "text"
  | "wait"
  | "progress"
  | "crosshair"
  | "move"
  | "not-allowed"
  | "ns-resize"
  | "ew-resize"
  | "nesw-resize"
  | "nwse-resize"
  | "n-resize"
  | "e-resize"
  | "s-resize"
  | "w-resize"
  | "ne-resize"
  | "nw-resize"
  | "se-resize"
  | "sw-resize"
  | "none"

/** One frame of an animated cursor (see CursorOptions.frames). */
export interface CursorFrame {
  /** The frame's encoded image (png, webp, ...). Every frame decodes to the same size. */
  image: Uint8Array
  /** HiDPI alternates keyed by scale: `{ 2: bytes }` must decode to exactly twice the image's size. */
  alternates?: Record<number, Uint8Array>
  /** How long the frame shows, in milliseconds. 0 holds it, which ends a one-shot animation. */
  duration: number
}

export interface CursorOptions {
  /** The cursor's encoded image (png, webp, ...). Use `frames` instead for an animated cursor. */
  image?: Uint8Array
  /** HiDPI alternates of `image` keyed by scale: `{ 2: bytes }` must decode to exactly twice its size. */
  alternates?: Record<number, Uint8Array>
  /** Frames of an animated cursor, played in order and looped (unless a frame's duration is 0). */
  frames?: CursorFrame[]
  /** The click point, in the 1x image's pixels. Default: the top-left corner. */
  hotspot?: [number, number]
  /** Pass false to keep the cursor past the owning scope and drop it yourself. */
  autoFree?: boolean
}

/**
 * Registers an image cursor and returns the handle an element's `cursor` prop
 * takes: `<view cursor={id}>`. Synchronous, from bytes you hold (a
 * `with { type: "binary" }` import, or a file read); the platform keeps the
 * image, so one handle serves any number of elements. Supply a 2x alternate
 * for HiDPI displays: without one the platform scales the 1x image. A 32x32
 * base is the common size; keep it under 128x128, the ceiling shared across
 * platforms. The cursor is dropped with the owning reactive scope; outside
 * one (or with `autoFree: false`) it lives until the app ends.
 */
export function createCursor(opts: CursorOptions): CursorId {
  if (!opts || typeof opts !== "object") throw new Error("createCursor: expected an options object")
  let hasImage = opts.image !== undefined
  let hasFrames = opts.frames !== undefined
  if (hasImage === hasFrames) throw new Error("createCursor: pass either image or frames")
  let frames: CursorFrame[] = hasFrames ? opts.frames! : [{ image: opts.image!, alternates: opts.alternates, duration: 0 }]
  if (frames.length === 0) throw new Error("createCursor: frames is empty")
  let native = frames.map(decodeFrame)
  let [hotX, hotY] = opts.hotspot ?? [0, 0]
  let id = registerCursor(native, hotX, hotY) as CursorId
  if (opts.autoFree !== false && getOwner()) onCleanup(() => dropCursor(id))
  return id
}

// Decodes a frame's image and alternates to straight-alpha RGBA8 (what the
// platform's cursor surfaces take) and checks each alternate against its
// scale, so a mismatched asset fails here with the sizes named.
function decodeFrame(frame: CursorFrame): NativeFrame {
  if (!(frame.image instanceof Uint8Array)) throw new Error("createCursor: image must be a Uint8Array")
  if (!(Number.isInteger(frame.duration) && frame.duration >= 0))
    throw new Error(`createCursor: duration must be a whole number of milliseconds, got ${frame.duration}`)
  let base = decodeImage(frame.image, { alpha: "straight" })
  let images: NativeImage[] = [base]
  for (let [key, bytes] of Object.entries(frame.alternates ?? {})) {
    let scale = Number(key)
    if (!(scale > 1)) throw new Error(`createCursor: alternate scale ${key} must be above 1`)
    let alt = decodeImage(bytes, { alpha: "straight" })
    let [w, h] = [base.width * scale, base.height * scale]
    if (alt.width !== w || alt.height !== h)
      throw new Error(`createCursor: the ${scale}x alternate is ${alt.width}x${alt.height}, expected ${w}x${h}`)
    images.push(alt)
  }
  return { images, duration: frame.duration }
}
