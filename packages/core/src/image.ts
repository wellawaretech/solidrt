// CPU image codec plus the reactive load-and-upload convenience. decodeImage is
// the raw primitive (no GPU involved); createImage is the owner-aware layer on
// top that fetches/decodes/uploads for you and swaps the texture when the source
// changes - the same relationship createTexture/createShader have to flux:gpu.

import { createMemo, onCleanup } from "@solidjs/signals"
import { createTexture, destroyTexture } from "./gpu"

export type DecodedImage = {
  data: Uint8Array
  width: number
  height: number
}

/**
 * Decodes encoded image bytes (PNG, JPEG, and the other formats the runtime's
 * image decoder supports) into raw, tightly-packed RGBA8 pixels plus the
 * decoded dimensions. Feed the result straight into `createTexture`. Use this
 * when you want manual control; for the common case reach for `createImage`.
 */
export function decodeImage(bytes: Uint8Array): DecodedImage {
  return image.decodeImage(bytes)
}

export type ImageSource = string | Uint8Array

// Shared loader for URL sources. Every mount of the same URL shares one
// fetch/decode/texture (refcounted; the texture is destroyed when the last
// mount releases it). Failed loads stay cached for the session so remounts
// do not re-hammer a dead or rate-limited endpoint. Uint8Array sources
// bypass all of this: no key, per-mount texture.
type ImageEntry = {
  refs: number
  texture: number
  failed: boolean
  promise: Promise<number>
}

let imageCache = new Map<string, ImageEntry>()

// Uncoordinated fetch floods (one per mounted image) slow every transfer and
// get clients rate-limited; a small global gate keeps loads polite.
const MAX_CONCURRENT_FETCHES = 4
let activeFetches = 0
let fetchWaiters: (() => void)[] = []

async function acquireFetchSlot(): Promise<void> {
  if (activeFetches < MAX_CONCURRENT_FETCHES) {
    activeFetches++
    return
  }
  await new Promise<void>(resolve => fetchWaiters.push(resolve))
  activeFetches++
}

function releaseFetchSlot(): void {
  activeFetches--
  let next = fetchWaiters.shift()
  if (next) next()
}

async function loadImage(url: string): Promise<number> {
  await acquireFetchSlot()
  let bytes: Uint8Array
  try {
    let res = await fetch(url)
    if (!res.ok) throw new Error(`Image fetch failed: HTTP ${res.status} for ${url}`)
    bytes = await res.bytes()
  } finally {
    releaseFetchSlot()
  }
  let decoded: DecodedImage
  try {
    decoded = decodeImage(bytes)
  } catch (e) {
    throw new Error(`Image decode failed for ${url} (first bytes: ${sniffBytes(bytes)}): ${e}`)
  }
  return createTexture(decoded.data, decoded.width, decoded.height)
}

function acquireImage(url: string): ImageEntry {
  let entry = imageCache.get(url)
  if (!entry) {
    let e: ImageEntry = { refs: 0, texture: -1, failed: false, promise: undefined as never }
    e.promise = loadImage(url).then(
      id => {
        // Everyone released while the load was in flight: nothing owns the
        // texture, so drop it here instead of recording it.
        if (e.refs === 0) {
          destroyTexture(id)
          imageCache.delete(url)
        } else {
          e.texture = id
        }
        return id
      },
      err => {
        e.failed = true
        throw err
      },
    )
    // Awaiters observe the rejection; this keeps a fully-released failed
    // entry from surfacing as an unhandled rejection.
    e.promise.catch(() => {})
    imageCache.set(url, e)
    entry = e
  }
  entry.refs++
  return entry
}

function releaseImage(url: string): void {
  let entry = imageCache.get(url)
  if (!entry) return
  entry.refs--
  if (entry.refs > 0) return
  if (entry.failed) return
  if (entry.texture >= 0) {
    destroyTexture(entry.texture)
    imageCache.delete(url)
  }
  // Still pending: the settle handler above sees refs === 0 and cleans up.
}

/**
 * Loads an image as an async computation and returns a reactive accessor for its
 * GPU texture id. This is a SolidJS 2.0 async value: reading it suspends until
 * the image is ready, so read it inside a `<Loading>` boundary (a load failure
 * surfaces to `<Errored>`). A string source is fetched; a Uint8Array is decoded
 * directly. Pass an accessor instead of a value to make the source reactive -
 * the image reloads and the old texture is freed whenever it changes; the
 * current texture is freed when the owner is disposed. Display it with
 * `<texture src={id()} />`; the texture carries its own pixel size, so no
 * width/height is needed unless you want to scale it.
 *
 * URL loads are shared: mounts of the same URL reuse one fetch and one texture
 * (freed when the last user is disposed), at most four fetches run at once,
 * and a failed URL stays failed for the session instead of refetching per
 * mount.
 *
 * For bytes you already hold (a `with { type: "binary" }` import, or anything in
 * memory) this suspends needlessly: `decodeImage` + `createTexture` are both
 * synchronous, so reach for them directly and skip the `<Loading>` boundary.
 * `createImage` earns its async only for a fetched string URL or a reactive
 * source.
 */
export function createImage(src: ImageSource | (() => ImageSource)): () => number {
  let getSrc = typeof src === "function" ? src : () => src

  return createMemo<number>(async () => {
    let source = getSrc()

    if (typeof source === "string") {
      // Acquire and register cleanup synchronously, before the await: an
      // onCleanup added after an await is orphaned because the reactive owner
      // is not restored across it.
      let entry = acquireImage(source)
      onCleanup(() => releaseImage(source))
      return await entry.promise
    }

    // Byte sources decode and upload synchronously; this run owns the texture.
    let holder = { id: -1 }
    onCleanup(() => {
      if (holder.id >= 0) destroyTexture(holder.id)
    })
    let decoded: DecodedImage
    try {
      decoded = decodeImage(source)
    } catch (e) {
      throw new Error(`Image decode failed (first bytes: ${sniffBytes(source)}): ${e}`)
    }
    holder.id = createTexture(decoded.data, decoded.width, decoded.height)
    return holder.id
  })
}

// A payload that fails to decode is usually not an image at all (an HTML error
// page, a JSON error body); showing its first bytes makes that recognizable in
// the log without a debugger.
function sniffBytes(bytes: Uint8Array): string {
  let head = ""
  for (let i = 0; i < Math.min(bytes.length, 24); i++) {
    let b = bytes[i] ?? 0
    head += b >= 32 && b < 127 ? String.fromCharCode(b) : "."
  }
  return JSON.stringify(head)
}