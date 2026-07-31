// Importing a file with `with { type: "text" }` inlines its contents into the
// bundle as a string. Like the binary form, the text travels inside the
// compiled bytecode, so it is available synchronously - no runtime read, works
// offline. The attribute works on any extension; `.svg` is text-loaded without
// one, and `.glsl`/`.vert`/`.frag` are declared as text modules so shader
// sources typecheck with no setup.
//
// This example shows only the text import itself: it reports the imported
// file's size and first line. Shader sources are the motivating case - the
// string is exactly what gpu-shader.tsx passes to createShaderTexture, moved
// out of the .tsx so it can be edited as GLSL. Inlining trades update granularity for
// zero I/O, so keep big or streamable files in assets/ and read them at
// runtime instead.
import { render } from "@solidrt/core"
import source from "./wave.glsl" with { type: "text" }

// The file's own first line - proof the real text is inlined, not a path.
let firstLine = source.split("\n")[0] ?? ""

function App() {
  return (
    <window alignItems="center" justifyContent="center" gap={8}>
      <text fontSize={18} color="#e6e6e6">{source.length} characters inlined</text>
      <text color="#888">starts with {firstLine}</text>
    </window>
  )
}

render(() => <App />)
