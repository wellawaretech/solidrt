// createImage loads an image as a SolidJS 2.0 async value and returns an accessor
// for its GPU texture id. It handles fetch, decode, GPU upload, and cleanup for
// you. A string source is fetched; a Uint8Array is decoded directly. The texture
// carries its own pixel size, so <texture> needs no width/height to show it at
// natural size - pass them only to scale it.
//
// Reading img() suspends until ready, so read it inside a <Loading> boundary
// (this is the 2.0 async mechanic, not a manual undefined-signal + <Show>); a
// load failure surfaces to <Errored>. Pass an accessor (createImage(() => src()))
// to make the source reactive - the image reloads and the old texture is freed.
// For manual control, decodeImage + createTexture (from @solidrt/core/gpu) are
// the primitives underneath.
import { render, createImage, Loading } from "@solidrt/core"

function App() {
  let img = createImage("https://picsum.photos/seed/solidrt/400/300")

  return (
    <window alignItems="center" justifyContent="center">
      <Loading fallback={<text color="#888">loading...</text>}>
        <texture src={img()} />
      </Loading>
    </window>
  )
}

render(() => <App />)