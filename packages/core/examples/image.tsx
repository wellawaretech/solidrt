// Displaying an image is a three-step primitive pipeline: get the encoded bytes
// (here via fetch; flux:fs or a bundled asset work too), decodeImage them to raw
// RGBA8 pixels, then createTexture to upload to the GPU and show the returned id
// with <texture>. (@solidrt/components wraps this as <Image>; this is what it
// does underneath.)
//
// Loading is async, so hold the texture id in a signal and render once it exists.
// Note: createTexture auto-frees only when called inside a reactive scope; here it
// runs after an await (no owner), so this texture lives for the app lifetime. For
// images you swap out, destroyTexture (from @solidrt/core/gpu) the old id.
import { render, decodeImage, createSignal, Show } from "@solidrt/core"
import { createTexture } from "@solidrt/core/gpu"

function App() {
  let [id, setId] = createSignal<number>()

  async function load() {
    let res = await fetch("https://picsum.photos/seed/solidrt/400/300")
    let { data, width, height } = decodeImage(new Uint8Array(await res.arrayBuffer()))
    setId(createTexture(data, width, height))
  }
  load()

  return (
    <window alignItems="center" justifyContent="center">
      <Show when={id()}>{texId => <texture src={texId()} width={400} height={300} />}</Show>
    </window>
  )
}

render(() => <App />)