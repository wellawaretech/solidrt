// CPU image codec plus the reactive load-and-upload convenience. decodeImage is
// the raw primitive (no GPU involved); createImage is the owner-aware layer on
// top that fetches/decodes/uploads for you and swaps the texture when the source
// changes - the same relationship createTexture/createShader have to flux:gpu.

import { createEffect, createSignal, onCleanup } from "@solidjs/signals"
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

export type LoadedImage = {
  id: number
  width: number
  height: number
}

/**
 * Loads an image and returns a reactive accessor for it (undefined until ready):
 * the GPU texture `id` plus the decoded `width`/`height`, so you can size the
 * `<texture>` to its natural dimensions. A string source is fetched; a
 * Uint8Array is decoded directly. Pass an accessor instead of a value to make
 * the source reactive - the image reloads and the old texture is freed whenever
 * it changes. The current texture is also freed when the reactive owner is
 * disposed. Display the result with
 * `<texture src={img().id} width={img().width} height={img().height} />`.
 */
export function createImage(src: ImageSource | (() => ImageSource)): () => LoadedImage | undefined {
  let getSrc = typeof src === "function" ? src : () => src
  let [loaded, setLoaded] = createSignal<LoadedImage>()

  createEffect(getSrc, (source) => {
    let stale = false
    ;(async () => {
      try {
        let bytes: Uint8Array
        if (typeof source === "string") {
          let res = await fetch(source)
          bytes = await res.bytes()
        } else {
          bytes = source
        }
        if (stale) return
        let { data, width, height } = decodeImage(bytes)
        let id = createTexture(data, width, height)
        let old = loaded()
        setLoaded({ id, width, height })
        if (old !== undefined) destroyTexture(old.id)
      } catch (err) {
        if (!stale) console.error("createImage: failed to load", source, err)
      }
    })()
    return () => {
      stale = true
    }
  })

  onCleanup(() => {
    let cur = loaded()
    if (cur !== undefined) destroyTexture(cur.id)
  })

  return loaded
}