// CPU image codec plus the reactive load-and-upload convenience. decodeImage is
// the raw primitive (no GPU involved); createImage is the owner-aware layer on
// top that fetches/decodes/uploads for you and swaps the texture when the source
// changes - the same relationship createTexture/createShader have to flux:gpu.

import { createMemo, onCleanup, NotReadyError } from "@solidjs/signals"
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
 * For bytes you already hold (a `with { type: "binary" }` import, or anything in
 * memory) this suspends needlessly: `decodeImage` + `createTexture` are both
 * synchronous, so reach for them directly and skip the `<Loading>` boundary.
 * `createImage` earns its async only for a fetched string URL or a reactive
 * source.
 */
export function createImage(src: ImageSource | (() => ImageSource)): () => number {
  let getSrc = typeof src === "function" ? src : () => src
  let generation = 0

  return createMemo<number>(async () => {
    let source = getSrc()
    let mine = ++generation

    // Register cleanup synchronously, before the await: an onCleanup added after
    // an await is orphaned because the reactive owner is not restored across it.
    // The holder is filled in once the texture exists.
    let holder = { id: -1 }
    onCleanup(() => {
      if (holder.id >= 0) destroyTexture(holder.id)
    })

    let bytes = typeof source === "string" ? await (await fetch(source)).bytes() : source

    // If the source changed while we were loading, this run is superseded. Skip
    // the GPU upload and stay pending: a texture created here would leak, since
    // superseded async runs are not otherwise cleaned up. The newer run wins.
    if (mine !== generation) throw new NotReadyError(source)

    let { data, width, height } = decodeImage(bytes)
    holder.id = createTexture(data, width, height)
    return holder.id
  })
}