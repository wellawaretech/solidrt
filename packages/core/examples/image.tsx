// createImage loads an image and returns a reactive accessor for it (undefined
// until ready): the GPU texture id plus the decoded width/height. It handles
// fetch, decode, GPU upload, and cleanup for you. A string source is fetched; a
// Uint8Array is decoded directly. Show it with <texture> once it exists, sized
// to the image's natural dimensions.
//
// Pass an accessor (createImage(() => src())) instead of a value to make the
// source reactive - the image reloads and the old texture is freed on change.
// For manual control, decodeImage + createTexture (from @solidrt/core/gpu) are
// the primitives underneath.
import { render, createImage, Show } from "@solidrt/core"

function App() {
  let img = createImage("https://picsum.photos/seed/solidrt/400/300")

  return (
    <window alignItems="center" justifyContent="center">
      <Show when={img()}>{m => <texture src={m().id} width={m().width} height={m().height} />}</Show>
    </window>
  )
}

render(() => <App />)