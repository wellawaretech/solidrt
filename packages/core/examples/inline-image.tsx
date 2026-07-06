// Displaying an image whose bytes are already in memory - here a
// `with { type: "binary" }` import that inlines the file into the bundle. Because
// the bytes are on hand, the whole path is synchronous: decodeImage(bytes) turns
// the encoded PNG into raw RGBA8 pixels, and createTexture uploads them to the
// GPU and returns an id. No fetch, no async value, so no <Loading> boundary - the
// texture is ready in the same tick the component builds.
//
// Contrast image.tsx, which uses createImage for a string URL: that fetches at
// runtime, so it suspends and must be read inside <Loading>. The rule: createImage
// (async) for URLs or reactive sources; decodeImage + createTexture (sync) for
// bytes you already hold. Called here in the component body, the texture is freed
// automatically when the owner is disposed.
import { render, decodeImage, createTexture } from "@solidrt/core"
import bytes from "./logo.png" with { type: "binary" }

function App() {
  let { data, width, height } = decodeImage(bytes)
  let id = createTexture(data, width, height)

  return (
    <window alignItems="center" justifyContent="center">
      <texture src={id} />
    </window>
  )
}

render(() => <App />)
