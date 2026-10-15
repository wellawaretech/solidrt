// CPU image codec plus the reactive load-and-upload convenience. The codec is
// flux:image, re-exported here (like the flux:gpu re-exports in gpu.ts) so
// applications import everything image-shaped from one place; createImage is
// the owner-aware layer on top that fetches/decodes/uploads for you and swaps
// the texture when the source changes.

import { createMemo, onCleanup } from "@solidjs/signals"
import { decodeImage, type DecodedImage } from "flux:image"
import type { TextureCodec } from "flux:image"
import { on } from "sol:events"
import { createTexture, destroyTexture, uploadTexture, type TextureId } from "./gpu"
import type { TextureCodec as SettingsCodec } from "./textures"

export { decodeImage, encodeImage, encodeTexture, transcodeTexture } from "flux:image"
export type { DecodedImage, TextureCodec, TranscodedTexture } from "flux:image"

// The codecs the texture settings name (./textures, which takes no type
// from the runtime) are the encoder's: a codec added to one side alone
// stops this line from compiling.
type SameType<A, B> = [A] extends [B] ? ([B] extends [A] ? true : false) : false
const SETTINGS_NAME_THE_ENCODERS_CODECS: SameType<SettingsCodec, TextureCodec> = true

export type ImageSource = string | Uint8Array

// Shared loader for URL sources. Every mount of the same URL shares one
// fetch/decode/texture (refcounted; the texture is destroyed when the last
// mount releases it). Byte caching and fetch politeness live below, in the
// runtime's fetch layer (disk cache + per-host limit); this map exists for
// what a byte cache cannot provide, sharing the decoded GPU texture.
// Uint8Array sources bypass all of this: no key, per-mount texture.
type ImageEntry = {
  refs: number
  texture: TextureId | undefined
  promise: Promise<TextureId>
}

let imageCache = new Map<string, ImageEntry>()

async function loadImage(url: string): Promise<TextureId> {
  // Images are assets: cache to disk, no freshness. Use a versioned URL (or
  // fetch + decodeImage manually) when a URL's content must be re-checked.
  let res = await fetch(url, { cache: "force-cache" })
  if (!res.ok) throw new Error(`Image fetch failed: HTTP ${res.status} for ${url}`)
  let bytes = await res.bytes()
  let decoded: DecodedImage
  try {
    decoded = decodeImage(bytes)
  } catch (e) {
    throw new Error(`Image decode failed for ${url} (first bytes: ${sniffBytes(bytes)}): ${e}`)
  }
  return createTexture(decoded.data, decoded.width, decoded.height)
}

// A restored GPU context brings every uploaded texture back empty (see
// onContextRestored): the shared URL images reload from their source into the
// same id, so their mounts see nothing. A byte-sourced mount re-decodes its
// own bytes (registered per mount in createImage).
on("contextRestored", (e: { textures: TextureId[] }) => {
  let lost = new Set(e.textures)
  for (let [url, entry] of imageCache) {
    if (entry.texture !== undefined && lost.has(entry.texture)) reloadImage(url, entry.texture)
  }
})

async function reloadImage(url: string, id: TextureId): Promise<void> {
  try {
    let res = await fetch(url, { cache: "force-cache" })
    if (!res.ok) throw new Error(`HTTP ${res.status}`)
    let decoded = decodeImage(await res.bytes())
    // Still the live texture of this entry (a release and remount in the
    // meantime made a new one, which loaded fresh).
    if (imageCache.get(url)?.texture === id) uploadTexture(id, decoded.data)
  } catch (e) {
    console.error(`Image reload after GPU context loss failed for ${url}: ${e}`)
  }
}

function acquireImage(url: string): ImageEntry {
  let entry = imageCache.get(url)
  if (!entry) {
    let e: ImageEntry = { refs: 0, texture: undefined, promise: undefined as never }
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
        // Concurrent mounts shared this rejection; dropping the entry lets a
        // later remount retry (a transient failure recovers with the network).
        imageCache.delete(url)
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
  if (entry.texture !== undefined) {
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
 * (freed when the last user is disposed). The bytes are fetched with
 * `cache: "force-cache"` - images are assets, cached on disk with no
 * freshness check - so use a versioned URL when the content behind a URL can
 * change. A failed load rejects every mount sharing it; a later remount
 * retries.
 *
 * For bytes you already hold (a `with { type: "binary" }` import, or anything in
 * memory) this suspends needlessly: `decodeImage` + `createTexture` are both
 * synchronous, so reach for them directly and skip the `<Loading>` boundary.
 * `createImage` earns its async only for a fetched string URL or a reactive
 * source.
 */
export function createImage(src: ImageSource | (() => ImageSource)): () => TextureId {
  let getSrc = typeof src === "function" ? src : () => src

  return createMemo<TextureId>(async () => {
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
    let holder: { id: TextureId | undefined } = { id: undefined }
    onCleanup(() => {
      if (holder.id !== undefined) destroyTexture(holder.id)
    })
    let decoded: DecodedImage
    try {
      decoded = decodeImage(source)
    } catch (e) {
      throw new Error(`Image decode failed (first bytes: ${sniffBytes(source)}): ${e}`)
    }
    holder.id = createTexture(decoded.data, decoded.width, decoded.height)
    // After a GPU context loss the texture is empty again: decode and
    // upload once more, for as long as this mount owns it.
    onCleanup(
      on("contextRestored", (e: { textures: TextureId[] }) => {
        if (holder.id !== undefined && e.textures.includes(holder.id)) {
          uploadTexture(holder.id, decodeImage(source).data)
        }
      }),
    )
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