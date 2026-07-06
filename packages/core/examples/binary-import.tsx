// Importing a file with `with { type: "binary" }` inlines its bytes into the
// bundle as a Uint8Array. The data travels inside the compiled bytecode, so it
// is available synchronously - no runtime fetch, works offline. Reach for this
// with small assets you want baked in; for large or many assets prefer a string
// URL source loaded at runtime instead.
//
// This example shows only the binary import itself: it reports the imported
// file's length and leading bytes. What you do with the bytes afterwards is a
// separate concern. Since inlined bytes are already in memory, the synchronous
// path fits: decodeImage(bytes) + createTexture (no fetch, no <Loading>) - see
// inline-image.tsx. Reach for createImage instead when the source is a string
// URL loaded at runtime, or when you want the source to swap reactively.
import { render } from "@solidrt/core"
import bytes from "./logo.png" with { type: "binary" }

// The PNG magic number: 89 50 4e 47 ... - proof the real bytes are inlined.
let head = Array.from(bytes.slice(0, 8), (b) => b.toString(16).padStart(2, "0")).join(" ")

function App() {
  return (
    <window alignItems="center" justifyContent="center" gap={8}>
      <text fontSize={18} color="#e6e6e6">{bytes.length} bytes inlined</text>
      <text color="#888">starts with {head}</text>
    </window>
  )
}

render(() => <App />)
