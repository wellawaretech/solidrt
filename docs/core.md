# Core API

All functions are imported from `@solidrt/core`:

```ts
import { render, onFrame, onResize } from "@solidrt/core"
```

The core package provides low-level primitives. For higher-level components built on top of these, see [Components](components.md).

## render

```ts
render(code: () => any): void
```

Entry point for a SolidRT application. Accepts a function that returns a `<window>` element. Must be called once at the top level.

```jsx
render(() => <App />)
```

## onFrame

```ts
onFrame(fn: (tick: number, frame: number) => void): () => void
```

Registers a callback that fires on every rendered frame. `tick` is the current timestamp in milliseconds. `frame` is the frame count since the application started.

Returns a cleanup function that stops the callback. When called inside a reactive scope (a component or `createEffect`), cleanup is automatic when the scope is destroyed.

```jsx
onFrame((tick, frame) => {
  setAngle(tick * 0.001)
})
```

## onResize

```ts
onResize(fn: ({ width, height, safeArea, displayScale }) => void): () => void
```

Registers a callback that fires whenever the window is resized. `safeArea` describes OS-reserved insets (e.g. notches, status bars). `displayScale` is the device pixel ratio.

Returns a cleanup function. When called inside a reactive scope, cleanup is automatic.

## onLayout

```ts
onLayout(fn: () => void): () => void
```

Registers a callback that fires after layout has been computed for the current frame, but before paint. Setting layout-affecting properties from this callback triggers one additional layout pass before painting.

Returns a cleanup function. When called inside a reactive scope, cleanup is automatic.

## onPointerMove

```ts
onPointerMove(fn: (e: GlobalPointerEvent) => void): () => void
```

Observes every pointer move, unattached to any node - for ambient tracking such as cursor followers, idle detection, or debug overlays. The event carries window coordinates (`clientX`/`clientY`), the deepest node id under the pointer (`target`, 0 when nothing is hit), pointer identity (`pointerId`, `pointerType`) and modifier flags. There are no per-node fields.

For element interaction use the per-node `onPointerMove` prop instead: it delivers exact local coordinates, and during a drag, moves keep flowing to the pressed element and its ancestors even off-element. Moves are frame-batched: the runtime delivers at most one move per pointer per frame, all of a frame's moves the same age, followed by the `pointerFrame` bus event. While a global subscriber exists, every such move crosses into JS - the runtime otherwise skips moves that no element listens to - so prefer node handlers when the interest is spatial.

Returns a cleanup function. When called inside a reactive scope, cleanup is automatic.

## onWindowFocus

```ts
onWindowFocus(fn: () => void): () => void
```

Registers a callback that fires when the OS window gains focus.

## onWindowBlur

```ts
onWindowBlur(fn: () => void): () => void
```

Registers a callback that fires when the OS window loses focus.

## onBack

```ts
onBack(fn: (e: { preventDefault: () => void }) => void): () => void
```

Registers a callback for the user's back intent: the Android back button or gesture, or the desktop dev chord (Ctrl/Cmd+Shift+Backspace). Call `e.preventDefault()` when back means in-app navigation right now (close a modal, go to the previous screen). When no handler prevents it, the default action runs: `exit()`.

Apps without a handler exit on back everywhere - the correct zero-effort default.

Handlers form a stack: the most recently registered one runs first, and the first to prevent ends the dispatch - handlers registered before it do not run, and neither does the default. Back is a pop, so the thing most recently put on screen answers for it: a dialog that opens over a screen registers after that screen and takes the next back press, then unregisters when it closes and hands the step back. Registration order tracks mount order, so this reads as innermost-first for handlers registered while mounting.

Each screen or overlay therefore owns exactly one step of the back stack and needs to know nothing about the others. The rule that makes that work: a handler that does not prevent must not act either, because the event is still travelling to whoever will handle it.

Returns a cleanup function. When called inside a reactive scope, cleanup is automatic.

```jsx
onBack((e) => {
  if (modalOpen()) {
    e.preventDefault()
    setModalOpen(false)
  }
})
```

There is no way to trap the user: a hung app is force-exited by the client, and the desktop window close button never enters JS.

## exit

```ts
exit(): void
```

Leaves the current app, unconditionally. What that means is the host's decision: back to the launcher in a dev client, quitting when standalone or at the launcher itself (on Android the client moves to the background instead of dying, matching the platform's back-at-root behavior).

This is the default action of an unprevented `back` event. Call it directly to exit programmatically - for example after intercepting back with `preventDefault()` to show an unsaved-changes dialog, then exiting on "discard". Without it, intercepting back would be a one-way trap.

## env

```ts
env: {
  windowSize, safeArea, displayScale, windowFocused, keyboardHeight,
  inputDevices, systemTheme, textScale, orientation, visibility,
  mouseSeen, touchSeen, keyboardSeen
}
```

Reactive facts about the environment the app is running in. Read properties inside a tracked scope (JSX, a memo, an effect) to re-run when they change; a top-level untracked read freezes at the initial value. Behavior decisions should normally go through `capabilities` and the policy layer - read `env` directly when the raw fact itself is needed.

### env.visibility

```ts
env.visibility: "visible" | "hidden"
```

Whether the app is on screen: `"hidden"` while backgrounded (Android) or minimized (desktop), `"visible"` again on return. The web's `visibilityState` vocabulary without the `document` machinery.

**This is the persistence moment.** There is no close event on any platform - Android gives apps no time when killing them, and the desktop window close never enters JS - so save state when `visibility` goes `"hidden"`:

```jsx
createEffect(
  () => env.visibility,
  (v) => {
    if (v === "hidden") saveState()
  },
)
```

The transition is delivered at background time (not on return), so the handler really runs before the OS may kill the process. While hidden, timers keep running but no frames are produced.

---

## Elements

Elements are the building blocks of a SolidRT UI. They map directly to native rendering commands via Lattice.

Every painting element comes in two forms. The layout form (`<rect>`, `<text>`, ...) participates in flexbox: its geometry is its layout box, sized with the layout props (`width`, `height`, flex). The detached form (`<d-rect>`, `<d-text>`, ...) is invisible to layout and owns its geometry through paint-space props (`x`, `y`, `w`, `h`; endpoints on `<d-line>`) that never cause reflow - use it for anything that moves at animation frequency. The two vocabularies do not mix: geometry props are rejected on layout forms, layout props on detached forms.

### `<window>`

The root element. Maps to a native OS window. Every application must have exactly one `<window>` as the root, passed to `render()`. Supports layout props, plus `title`, `fullscreen`, and `shader`.

`shader` runs the window's finished frame through a GPU program as the last step before it reaches the screen (see [Window shader](#window-shader) below).

### `<view>`

The primary container element. Supports layout, transform, and pointer event props. Use it to compose and structure the UI. With `repaintBoundary="snapshot"` it can also run its rendered subtree through a GPU program via the `shader` prop (see [Boundary shader](#boundary-shader)).

```jsx
<view width={200} height={100} background="#eee">
  <text>hello</text>
</view>
```

### `<text>`

Renders text. Children are the text content.

```jsx
<text color="#333" fontSize={16}>Hello</text>
```

### `<rect>`

Draws a rectangle filling its layout box; size it with the layout props (`width`, `height`, flex). Supports paint and pointer event props. `radius` sets the corner radius - a single number applies to all corners, or pass `[top-left, top-right, bottom-right, bottom-left]`. On the detached form `<d-rect>`, `w` and `h` set the size and `x` and `y` offset the origin, in the parent's coordinates.

```jsx
<rect width={80} height={80} radius={8} color="#0077ff" />
<d-rect x={10} y={10} w={80} h={80} radius={8} color="#0077ff" />
```

### `<oval>`

Draws an oval (ellipse) inscribed in its layout box; size it with the layout props. Supports paint and pointer event props. On `<d-oval>`, `w` and `h` set the bounding box (not radii) and `x` and `y` offset the origin.

```jsx
<oval width={80} height={80} color="#0077ff" />
```

### `<line>`

Draws a straight line spanning its layout box, from the top-left to the bottom-right corner; a box with `height={0}` gives a horizontal rule. Supports paint and pointer event props. Set `onLength` and `offLength` together to draw a dashed line. On `<d-line>`, the endpoints are set explicitly with `x1`, `y1`, `x2`, `y2`.

```jsx
<line width={100} height={0} color="#0077ff" strokeWidth={2} />
<d-line x1={0} y1={0} x2={100} y2={100} color="#0077ff" strokeWidth={2} />
```

### `<path>`

Draws an SVG path. `d` is the SVG path data string; its bounds give the element its intrinsic size. `fillRule` controls how overlapping subpaths are filled (`"nonzero"` by default, or `"evenodd"`). Supports paint and pointer event props. On `<d-path>`, `x` and `y` offset the entire path.

```jsx
<path d="M 10 10 L 90 10 L 50 80 Z" color="#0077ff" />
```

### `<texture>`

Draws a GPU texture. `src` is a texture ID returned by `createTexture`. The texture's pixel size (or the source crop, when set) is the element's intrinsic size, with HTML `<img>` sizing rules; the layout props override it. Supports source crop props (`srcX`, `srcY`, `srcW`, `srcH`; on `<d-texture>` also `x`, `y`, `w`, `h` for the destination box) and `params` for shader parameters. The prop means "the target's params" on every target kind - the same channel `setTargetParams` drives: on a fragment or pipeline target the one program's uniforms, on a draw target the shared params every entry reads. A param value is a number for a scalar uniform (`float`, or `int`/`bool`, truncated) or a flat number array for a typed one - the shader's own declaration decides the dispatch, so `vec2`/`vec3`/`vec4` take 2/3/4 numbers and `mat4` takes 16 in column-major order. An array uniform (`vec3 uLight[4]`) goes by its bare name and takes one flat array of element length times array size (12 here) - a light list or palette is one write. Every params write is validated against the program's active uniforms: an unknown name or a value whose length does not fit the declared type throws - the `params` prop writes through the same channel as `setTargetParams`, so prop and imperative writes validate and error identically (set `src` before `params`: a params write with no src to route to throws). However often a signal writes, the target renders once per frame at the raster flush. A uniform that is declared but optimized out reflects as inactive and counts as unknown. The same value shapes and validation apply everywhere params appear, the window shader included.

`fit` maps the pixels into the element box with CSS object-fit semantics: `"fill"` (default) stretches, `"cover"`/`"none"` crop, `"contain"`/`"scale-down"` letterbox, everything centered. Paint-only: the box (and hit testing) is unchanged.

`blendMode` (the full Skia set: `"plus"`, `"screen"`, `"multiply"`, ...) is how several GPU passes composite in the tree. Stack absolutely-positioned `<texture>` elements - a base pass, then an additive `blendMode="plus"` pass over it - instead of writing a shader that samples both targets. Texture alpha is premultiplied, so additive modes need no manual premultiplication. Within one pipeline's own draw, `createPipelineTexture`'s `blend: "add"` option makes overlapping geometry accumulate additively (order-independent, so no sorting; a depth-tested additive pass pairs it with `depthWrite: false`, stated explicitly - neither option implies the other); without it a target's draw runs with GL blending disabled and overwrites.

For loading images from URLs or bytes without working directly with textures, use the [`<Image>`](components.md#image) component instead.

---

## GPU

Functions for loading image data onto the GPU.

```ts
import { decodeImage, createTexture } from "@solidrt/core"
```

### decodeImage

```ts
decodeImage(bytes: Uint8Array): DecodedImage
```

Decodes an image file (JPEG, PNG, etc.) from raw bytes. Returns an object with `data` (raw RGBA pixels), `width`, and `height`.

### encodeImage

```ts
encodeImage(img: DecodedImage, options?: { format?: "png" | "jpeg", quality?: number }): Uint8Array
```

The reverse of `decodeImage`: encodes raw RGBA pixels into an image file. `format` defaults to `"png"`. `"jpeg"` has no alpha channel (it is dropped) and takes `quality` in 0..1, default 0.9; `quality` is ignored for png. Throws when `data.length` does not match `width * height * 4`.

### createTexture

```ts
createTexture(data: Uint8Array, width: number, height: number): TextureId
```

Uploads raw RGBA pixel data to the GPU and returns a texture ID. Pass the returned ID as the `src` prop on a `<texture>` element.

Every GPU id space is a branded number type - `TextureId`, `BufferId`, `ShaderStageId`, `ProgramId`, `RenderPipelineId`, all exported from `@solidrt/core/gpu` - so a cross-space slip like `destroyBuffer(textureId)` is a compile-time error instead of an operation on an unrelated live resource. At runtime they are plain numbers and widen to `number` freely; use the exported types to annotate storage (`let ids: TextureId[]`).

Sampling is a per-texture property declared at creation: every create helper (`createTexture`, `createMutableTexture`, `createShaderTexture`, `createPipelineTexture`, `createShaderTarget`) accepts `filter` (`"linear"` default, or `"nearest"`) and `wrap` (`"clamp"` default, or `"repeat"`) in its options. The state belongs to the texture id and applies everywhere it is sampled - shader `sampler2D` inputs and `<texture src>` display alike - so a `"nearest"` texture upscales with hard pixels on screen: render at 320x200, display window-sized, and you have the pixel-art path. `wrap` only matters to shaders sampling outside `0..1` (the display draw never tiles). The state survives id-stable resizes and cannot be changed after creation; no mipmaps exist.

The pixel-upload creates (`createTexture`, `createMutableTexture`) additionally accept `format`: `"rgba8"` (default, 4 bytes per pixel) or `"r8"` (single-channel, 1 byte per pixel). Like the sampler state, the format belongs to the id, is fixed at creation, and sizes every later `uploadTexture`/`resizeTexture` frame. `"r8"` is the palette-indexed / grayscale path: upload raw indices and look the color up in a shader (a `sampler2D` reads an r8 texture as `(v, 0, 0, 1)` - take `.r`), so a palette effect is a 1 KB palette write instead of a full-frame conversion, uploads shrink 4x, and any width works - r8 uploads are alignment-free, no packing indices four-to-a-texel or padding rows to multiples of four. Displaying an r8 texture directly via `<texture src>` shows the same red-channel reading; shader/pipeline targets and readbacks stay RGBA8.

```js
let screen = createShaderTexture(src, 320, 200, null, { filter: "nearest" })
// <texture src={screen} /> filling the window shows hard pixels, not smoothing
```

```js
let img = decodeImage(bytes)
let id = createTexture(img.data, img.width, img.height)
// <texture src={id} imageWidth={img.width} imageHeight={img.height} />
```

When called inside a reactive scope the texture is freed automatically once that owner is disposed. When called outside one (for example after an `await`, where the owner is no longer current), nothing is registered and you must free it yourself with `destroyTexture(id)`. The same rule applies to `createMutableTexture` and `createShaderTexture`. Every create helper also accepts `{ autoFree: false }` to skip the auto-free when you manage disposal yourself (for example a resource rebuilt on signal changes inside a long-lived component, where each rebuild would otherwise stack another cleanup on the component owner).

Every create also accepts `label`, a free-form debug name (WebGPU's label idea): the dev server's GPU inventory and the engine's log messages then name the resource `7 (bloom-h)` instead of a bare id, which is what makes a chain of six targets readable. Labels are never interpreted, need not be unique, and survive id-stable resizes (`resizeTexture`, `setTargetSize`).

To change a texture's size without invalidating its id (for example a data texture backing a window-sized grid), use `resizeTexture(id, data, width, height)`; render targets of every kind resize with `setTargetSize(id, width, height)` (programs, params, bindings, and draw state carry over). Both keep the id stable, so `<texture src>` references, sampler bindings, and the owner-scoped auto-free registered at creation all keep working - nothing needs re-creating.

`destroyTexture` is frame-safe: the runtime reclaims the id only once the render tree no longer references it. Destroying the old id in the same update that repoints `<texture src>` at its replacement is therefore always safe - whichever order the destroy and the swap land in, no frame paints blank. A destroyed id that stays mounted keeps drawing (and stays allocated) until it is unmounted or repointed.

For a shader whose spec is itself reactive, `createShaderTextureMemo(() => ({ fragmentSrc, width, height, params?, textures? }))` returns an accessor for the current texture id and keeps the GPU resource in step: size changes route to `setTargetSize` and params changes to `setTargetParams` (id stays stable), while a new fragment source or new sampler bindings rebuild at a fresh id, update the accessor, and frame-safely destroy the old one.

Pass `{ onError }` as a second argument when the source is not known-good - a shader editor, live coding, a dialect ported from elsewhere. A shader that fails to compile then hands you the error and leaves the last shader that *did* compile current (id, size, params and accessor all unchanged), so the app keeps drawing instead of tearing down. Without `onError` the failure throws from inside the effect, where no caller can catch it and the reactive system halts. The initial compile is not covered either way: it throws at the call site, where an ordinary `try`/`catch` works and there is no previous shader to keep.

A live target's sampler2D inputs can also be retargeted directly with `setTargetTextures(id, { samplerName: textureId })` - the sampler analog of `setTargetParams`: the target re-renders with its current params against the new sources, without recompiling. Bindings not named keep their current source.

Sampler bindings are live dependencies. A target may sample another target's output, and when a source re-renders - a params write, a vertex-buffer write, a data-texture upload, a rebind - every target sampling it re-renders too, transitively through the chain, before the next frame or readback observes them. Each target renders at most once per frame no matter how many of its inputs changed, so a multi-pass chain (a plasma target feeding a cube pipeline) stays current without any consumer writing a uniform per frame. A binding that would close a sampling cycle among runtime-rendered targets throws, as does binding a shader's own target (same-pass feedback).

That model rests on a stated contract: **a target's contents are a pure function of its inputs** (params, bound textures, geometry). The runtime renders a target whenever its inputs change - zero, one, or many times per frame, at its discretion - so rendering twice must be indistinguishable from rendering once. A pass that is *state* rather than a function - accumulation over its own previous output, feedback between targets, a simulation step - breaks that contract: how many times it ran would silently depend on scheduling details (a resize, a snapshot, a readback each trigger renders), and the output would drift between machines and runs.

For that class, declare the target `render: "manual"` (on `createShaderTarget` or `createPipelineTexture`). The runtime then never renders it: it starts cleared to its `clearColor`, and only an explicit `renderTarget(id)` runs the pass - once per call, in call order relative to every other GPU call. Inputs are fresh (pending runtime renders of sampled targets resolve first), a `readTexture` issued after a render observes it, and targets sampling a manual target stay live dependencies - they update after each explicit render. Its own `setTargetParams` / `writeBuffer` / `setDraw` writes take effect at its next render. Step manual targets from `onFrame`, which also keeps steps deterministic under load shedding and in recorded playback (steps count calls, never frames). Two caveats: binding a target to itself still throws (that is same-pass GL feedback, undefined pixels no matter who schedules it), and `setTargetSize` clears a manual target - state cannot survive the storage reallocation - so re-seed after a resize.

Two companions complete the class. `loadOp` chooses what each render finds in the target: `"clear"` (the default) clears to `clearColor` first; `"load"` keeps the previous contents and draws over them - single-target accumulation (with the pipeline's `blend: "add"`, an additive trail; without blending, draws simply land over old pixels). `"load"` requires `render: "manual"` and throws otherwise - on a runtime-rendered target the output would depend on how often the runtime happened to render, which is exactly the invisible failure the contract exists to prevent. Depth stays per-render scratch and always clears. And state that must read-modify-write its *own* pixels (decay, blur, simulation) still ping-pongs across two manual targets bound to each other - a pass can never sample the texture it writes, and the pair is legal exactly because the runtime never renders either. `copyTexture(src, dst)` seeds both shapes GPU-side: it overwrites a manual target with any texture's current pixels (uploaded, camera, another target's output), exact and same-size only (a size mismatch throws; a scaling copy is an ordinary pass), landing in call order like renders - seed a `loadOp` accumulator, snapshot one ping-pong buffer, reset state to a known image.

### Pixel contract

Three facts hold for every texture and target. None of them is configurable, and each is a thing pipeline authors otherwise discover from a wrong-looking frame.

- **Clip space is y-down.** `gl_Position` y = -1 is the top of the target, +1 the bottom (GL's row 0 is clip y = -1, and Impeller samples row 0 as the top). A vertex stage carrying camera-up geometry must negate y - `gl_Position = vec4(x, -y, z, w)`, or the same flip folded into its projection matrix - or it draws upside down. That is Vulkan's convention, not desktop GL's. The fragment path absorbs the same flip already: `vUV` is 0..1 with top-left origin, so a fragment-only shader never sees it.
- **Color is premultiplied alpha.** A target's RGB is expected already multiplied by its A: write `vec4(rgb * a, a)`, not `vec4(rgb, a)`, which composites as opaque. That is what Impeller composites and what [`blendMode`](#texture) blends. `clearColor` is premultiplied too, so the default transparent black needs no thought.
- **Values are non-linear RGBA8, with no color-space concept.** Every texture and target holds 8-bit RGBA UNORM exactly as the shader wrote it; nothing converts to or from linear light. So `filter: "linear"` averages and `blend: "add"` accumulates non-linear values - the usual approximation, stated here so shaders written today stay correct if a format vocabulary ever arrives.

### Device limits

Every limit a driver imposes is a hard cliff, and GL's native failure modes for crossing one are famously unhelpful (an oversize target fails as `framebuffer incomplete 0x8cd6`; a binding past the sampler unit cap silently draws garbage). SolidRT queries the ceilings once at startup and exposes them:

```ts
import { limits } from "@solidrt/core"

limits.maxTextureSize    // largest width/height of any texture or target, in pixels (>= 2048)
limits.maxTextureUnits   // sampler inputs one pass may bind (>= 16)
limits.maxVertexAttribs  // vertex attributes one pipeline may declare (>= 16)
```

Every create and bind is validated against them at the call site, naming the limit in the thrown error: texture and target sizes (creates, `resizeTexture`, `setTargetSize`) against `maxTextureSize`, a target's `textures` count (creation and the merged result of `setTargetTextures`; on a window shader the runtime-filled `uSource`/`uPrevious` count too) against `maxTextureUnits`, and a pipeline's `attributes` length against `maxVertexAttribs`. Read `limits` to size within the device - clamp a supersampled target to `maxTextureSize` - and treat the GLES 3.0 floors (2048 / 16 / 16) as the portable baseline every device guarantees.

### Raw shading layer

`createShaderTexture` and `createPipelineTexture` are fused conveniences: one call compiles, links, and creates a render target, with a curated preamble injected into the sources - both named for what they return, a texture id (the object that creates a shader-stage object is `compileShader`, below). The preamble declares exactly what the runtime provides (`#version 300 es`, precision, `vUV` on the fragment path, `fragColor`, `iResolution`); anything app-driven - a time uniform, say - is the source's own declaration, driven through params like any other uniform, so forgetting to drive it is a compile error rather than a value silently stuck at 0.

That injection is conditional. A source that starts with its own `#version` line is compiled exactly as written, so `createShaderTexture` doubles as the complete-source path: a shader that declares its own uniform names - one ported from elsewhere - runs unchanged without dropping to the raw layer below. The built-in vertex stage still supplies `vUV` to such a source; declare `in vec2 vUV;` yourself to read it. Reach for the raw layer for what it alone gives: sharing one compile across several targets, or holding a program with no target yet.

Underneath sits the raw GPU model, exposed directly:

```ts
compileShader(stage: "vertex" | "fragment", source: string, opts?: { header?: boolean }): ShaderStageId
linkProgram(vertexShader: ShaderStageId, fragmentShader: ShaderStageId): ProgramId
createRenderPipeline(program: ProgramId, opts?: { attributes?, instanceAttributes?, topology?, blend?, cull?, depth?, depthWrite? }): RenderPipelineId
createShaderTarget(pipeline: RenderPipelineId, width: number, height: number, params?, opts?): TextureId
renderTarget(id: TextureId): void                       // step a render: "manual" target, in call order
copyTexture(src: TextureId, dst: TextureId): void       // overwrite a manual target GPU-side (exact, same size)
destroyShader(id: ShaderStageId): void                  // stages; safe right after linking
destroyProgram(id: ProgramId): void                     // programs; live pipelines keep theirs alive
destroyRenderPipeline(id: RenderPipelineId): void       // pipelines; live targets keep theirs alive
```

`compileShader` compiles one stage from complete GLSL ES - the source declares its own `#version 300 es`, precision, varyings, and uniforms; nothing is injected. `{ header: true }` explicitly prepends the standard header (`#version 300 es`, highp precision, `iResolution`, and `out vec4 fragColor` for fragment stages - the same text `createPipelineTexture` injects); do not combine it with your own `#version`. Compile and link errors throw at the call, so a bad shader fails where it was written, not later at a prop write.

`linkProgram` yields a program handle in its own id space. `createRenderPipeline` pairs a program with draw state - the vertex layout (`attributes`), the per-instance layout (`instanceAttributes`, below), `topology`, `blend`, `cull`, and `depth`/`depthWrite` - into a pipeline handle: how it draws, the pipeline state object of every modern GPU API. `cull: "back" | "front"` discards one set of triangle faces by winding: a closed mesh culling its back faces halves its fragment work, while the default `"none"` rasters both faces - what open surfaces need. The winding rule is WebGPU's: counter-clockwise as displayed (screen coordinates) = front, measured after every flip - so a mesh exported CCW-front for a y-up world, drawn through a standard right-handed camera (looking down -z) with the usual y negation for display, culls correctly with `"back"`. Seeing the mesh's inside means the winding reaching the screen is mirrored: a clockwise exporter, or a left-handed hand-rolled rig (camera looking toward +z without mirroring x) - fix the rig or use `"front"`. `createShaderTarget` then builds a texture-backed target over the pipeline with the per-target half: size, the concrete vertex `buffer` the pipeline's layout describes, the draw range, uniforms, `clearColor`, and the render mode (`render: "manual"` for a target the app steps itself via `renderTarget` - see the render contract above) - where and when it draws. The draw range is WebGPU's draw arguments as data: `vertexCount` (default: the rest of the buffer) from `firstVertex` (default 0) picks the vertices, `instanceCount` draws the range as N instances (`glDrawArraysInstanced`, native ES 3.0) told apart by `gl_InstanceID` in the vertex stage - the standard answer to particles, tiles, and repeated meshes without duplicating vertices. `instanceCount: 0` draws nothing (a cheap off switch); `gl_VertexID` includes `firstVertex`, and `gl_InstanceID` always counts from 0 (ES 3.0 has no base instance).

Instances carry real state through `instanceAttributes` (WebGPU's `stepMode: "instance"`): a second layout on the pipeline, one interleaved record per INSTANCE, fetched from the target's `instanceBuffer` - every vertex of instance N reads record N (vertex divisor 1), so per-instance offsets, colors, or a packed transform arrive as plain `in` attributes instead of `gl_InstanceID` arithmetic against uniforms. The two layouts share one attribute namespace (a name in both throws at pipeline creation); a pipeline declaring `instanceAttributes` requires `instanceBuffer` on every entry drawn with it, and an `instanceBuffer` without declared instance attributes throws (it would never be read). `instanceCount` then defaults to one instance per record of the instance buffer - the whole-buffer rule instances were missing - and is bounds-checked against it like every fetch (without an instance buffer the default stays 1, the plain draw). A mat4 per instance is its four vec4 columns, reassembled in the shader (attributes have no matrix formats, as in WebGPU); `writeBuffer` into an instance buffer re-renders the targets fetching from it, exactly like a vertex buffer. Passing a draw-state key to `createShaderTarget` throws, as do create-time `params`/`textures` naming anything but the program's active uniforms and a range whose vertex fetch would run past the buffer's end; `setDraw(id, { firstVertex?, vertexCount?, instanceCount? })` updates the range later under the same bound (absent keys keep their current value, like params; a negative value or `(firstVertex + vertexCount) x stride > buffer size` throws - GL itself never checks that fetch). One compiled stage can back many programs, one program many pipelines, one pipeline many targets, and only `compileShader` compiles - which is what makes precompiling all programs at startup and swapping between them free of compilation. A raw-linked program carries its own vertex stage, so a fullscreen pass is a default pipeline plus `{ vertexCount: 3 }` over a covering-triangle vertex stage; a uniform named `iResolution`, if declared, is filled with the target size at render - as `vec2 (w, h)`, or `vec3 (w, h, 1.0)` for a source that declares it vec3 (a common convention in ported shaders).

In every target create (`createShaderTexture`, `createPipelineTexture`, `createShaderTarget`), `params` is its own argument ahead of the options - pass `null` when there are none. It seeds the live uniform channel the `params` prop and `setTargetParams` drive afterwards; the options bag holds only fixed creation-time configuration (`textures`, sampling, `label`, draw state where it applies).

`compileShader`, `linkProgram`, `createRenderPipeline`, and their destroyers are re-exported raw - the app owns those lifetimes (the runtime still reclaims them on reload). `createShaderTarget` produces a texture and gets the usual owner-scoped auto-free.

### Draw targets: many draws into one target

`createShaderTarget` draws one pipeline. A scene frame is N draws - one per mesh and material - sharing one depth buffer, and that is `createDrawTarget`: a render target whose contents are an ordered, mutable list of draws, rendered as one pass (clear once, then every entry in list order into the same storage). It is the render pass of every 3D API, retained the way everything here is retained: where WebGPU re-encodes the pass each frame, this target holds the list as state and re-renders on demand.

```ts
createDrawTarget(width, height, params?, opts?: { depth?, textures?, clearColor?, render?, loadOp?, filter?, wrap?, label? }): TextureId
addDraw(target, pipeline, params?, opts?: { buffer?, instanceBuffer?, textures?, before?, firstVertex?, vertexCount?, instanceCount? }): DrawId
addDraw(target, pipeline, params?, opts?: { indexBuffer, indexFormat, firstIndex?, indexCount?, ... }): DrawId  // the indexed form
removeDraw(target, draw): void
setDrawParams(target, draw, params): void      // setTargetParams, addressed to one entry
setTargetParams(target, params): void          // here: the target's SHARED params, read by every entry
setTargetTextures(target, textures): void      // here: the target's SHARED sampler bindings, ditto
setDrawTextures(target, draw, textures): void  // setTargetTextures, addressed to one entry
setDrawRange(target, draw, update): void       // setDraw, addressed to one entry
setDrawOrder(target, order): void              // full permutation of the live DrawIds
```

`addDraw` adds an entry - the same per-entry shape `createShaderTarget` takes (a pipeline, its concrete `buffer` and `instanceBuffer`, a draw range, `params`, `textures`) - and returns a stable `DrawId`: the handle the per-entry setters take, unaffected by other adds and removes, erroring after its entry is removed rather than aliasing. List order is draw order (later entries land over earlier ones where depth does not decide), so painter-style layering is append order; `before` inserts ahead of an existing entry instead, and `setDrawOrder` replaces the whole order with a permutation of the live ids (a missing, duplicate, or unknown id throws) - the sorting verb: opaque front-to-back, transparent back-to-front, re-issued when the camera moves, with entry state riding along untouched. Per-entry `params` is where per-object state lives - a moved mesh is one `setDrawParams` with its new model matrix - and entries bind textures independently: two entries may bind the same uniform name to different sources.

A value every entry shares - a camera's view-projection above all - is target state, not entry state: `setTargetParams` writes it once per target (`createDrawTarget`'s positional `params` seeds it), where per-entry writes would cost one call and one matrix multiply per mesh every camera move. Shared values apply before each entry's own params, so an entry naming the same uniform overrides the shared value (specific beats general), and they survive entry add/remove/rebuild - a geometry or material swap cannot lose them. Coverage may be partial, since a target legitimately mixes material classes: a name only some entries' programs declare is applied where declared and skipped elsewhere, exactly like `iResolution` - and coverage may be zero: a name no current entry declares is stored and skips everywhere until a declaring entry arrives. Shared state is therefore independent of write order - a value seeded before any entry exists and one written after entries attached are the same state - which is what lets a scene publish a standard set (a camera position beside the view-projection) whatever materials are present. Validation is arity where declared: a name must match the declared component count in every entry program that declares it; an entry added later never retroactively errors on a shared name its program lacks.

`setTargetTextures` is the sampler analog (`createDrawTarget`'s `opts.textures` seeds it): sources every entry reads - an environment map, a shadow map, a LUT - bound once per target, with the same precedence, coverage, and validation story, plus the usual binding checks (sources exist, each entry's effective inputs fit the device's texture units, no flush-rendered sampling cycles). Shared sources are live dependencies exactly like entry bindings: the target re-renders when one changes. The one place shared state gates a later write: `addDraw` (and a `setDrawTextures` rebind) checks the entry's own bindings PLUS the shared names its program declares against the unit limit, so an over-budget combination throws at its call site instead of silently dropping inputs.

One GL reality to seed against: uniform state lives on the program object, not the entry. The runtime re-applies shared then entry params at every pass precisely so entries sharing a program cannot clobber each other's *written* values - but a declared uniform that nothing writes (not seeded at `addDraw`, not covered by a shared value, never set later) is left holding whatever the last draw through that program applied, from any entry or target sharing it; only a freshly linked program reads the link-time zero. So seed everything a program declares, per entry or shared. This is not fixable automatically: zero-filling unset names at `addDraw` would overwrite shared values (entry-beats-shared is apply order), and demanding full coverage at `addDraw` would reject the legitimate add-first-then-set-shared ordering.

Depth splits the way WebGPU splits it: the target owns the storage (`depth: true`, one buffer cleared once per render and shared by every entry - what makes cross-entry occlusion work), while each entry's pipeline owns the behavior (`depth`/`depthWrite`: whether that draw tests and writes). Adding a depth-testing pipeline to a target without storage throws at `addDraw`.

An entry draws indexed by binding `indexBuffer` + `indexFormat: "uint16" | "uint32"` - any `createBuffer` buffer; the buffer kind is one, as in WebGPU and WebGL, so the format declares the element type. Vertices are then fetched through the index values (`glDrawElements`), which is what real meshes want: shared vertices stored and shaded once instead of triplicated per triangle. The range switches to WebGPU's indexed spelling - `firstIndex` + `indexCount` (default: the rest of the index buffer), same `instanceCount` - and the vertex-named keys throw on an indexed entry (and vice versa), in `addDraw` and `setDrawRange` both, so a range never silently counts the wrong thing. The `firstIndex`/`indexCount` fetch is bounds-checked against the index buffer like every range; the index values themselves are not checked against the vertex buffer (that would mean reading them back) - an out-of-range index is GL's undefined fetch. `writeBuffer` into an index buffer re-renders the targets indexing through it, exactly like a vertex buffer. The single-draw creates (`createShaderTarget`, `createPipelineTexture`) accept the same binding; there is no base vertex (ES 3.2 territory), so subranges of a shared vertex pool bake their offsets into the indices.

The render contract is unchanged, and that is the point: the list is input data like params, so "render twice = render once" still holds, and an ordinary (`render: "auto"`) draw target re-renders exactly when its entries or their inputs change. A static scene costs zero passes however many entries it holds, and one render is one pass however many entries it draws - which matters on hardware where pass count is the budget. `render: "manual"` and `loadOp: "load"` compose exactly as on `createShaderTarget`; with no entries a render is the clear alone. The target registers like any other (display via `<texture src>`, `setTargetSize`, owner-scoped auto-free); its entries die with it, while their pipelines and buffers are yours and outlive it. `setDraw` still throws on a draw target with a pointer to `setDrawRange` (there is no single range to update); `setTargetParams`/`setTargetTextures` route to the shared channel described above, on this kind as on every other.

### Inline shader sources

Shader sources are strings, and a shader small enough to read at a glance belongs in the file beside the code that uses it. Tag it with `glsl` so an editor can highlight it:

```ts
import { glsl } from "@solidrt/core/gpu"

let RINGS = glsl`
  in vec2 vUV;
  uniform float uTime;
  void main() {
    float d = length(vUV - 0.5);
    fragColor = vec4(vec3(0.5 + 0.5 * sin(d * 40.0 - uTime * 3.0)), 1.0);
  }
`
```

`glsl` returns the source unchanged - it is a marker, not a preprocessor, and every source stays exactly as valid without it. The name is the marker, so it has to be spelled `glsl`: that is what editor grammars key on. Highlighting needs an editor extension for it (in VS Code, one that injects into tagged templates, such as glsl-literal, plus one that supplies the GLSL grammar itself, such as WebGL GLSL Editor). Indentation is free - GLSL ignores it - so a source can sit at the indent level of the code around it.

Interpolation is verbatim, with no GLSL-aware formatting: `${2}` splices in the int literal `2`, which will not assign to a float. Pass anything that varies as a uniform rather than building it into the source, which also keeps the source constant - a source that changes is a recompile.

### Window shader

The `shader` prop on `<window>` draws the finished frame through a linked program before present - a whole-app effect (warp, dissolve, color grade) for the cost of one extra fullscreen pass:

```tsx
let vs = compileShader("vertex", FULLSCREEN_VERTEX)
let fs = compileShader("fragment", WARP_FRAG, { header: true })
let warp = linkProgram(vs, fs)

<window shader={{ program: warp, params: { uAmount: amount() } }}>
  ...
</window>
```

The declaration is `{ program, params?, textures?, vertexCount?, previous? }`; setting it to `null` (or omitting it) restores the direct path. While declared, the frame renders into a runtime-owned, window-sized layer texture the program samples - the layer has no id, no lifetime to manage, and is freed when the prop clears.

The program's contract:

- `uniform sampler2D uSource`, filled by name, is the frame. Top-left origin like every sampled texture, so a vertex stage mapping it onto the window flips the v coordinate (`vUV = vec2(uv.x, 1.0 - uv.y)` for the standard covering triangle).
- `uniform vec2 iResolution`, filled by name, is the window size in physical pixels - what the pass actually covers, unlike the logical points the rest of the API speaks.
- `params` are uniforms filled by name (numbers for scalars, flat number arrays for `vec2`/`vec3`/`vec4`/`mat4`, as on [`<texture>`](#texture)), paced to the next real repaint like every params prop. `textures` adds extra sampler2D inputs (a noise texture, a mask) by uniform name. Both validate against the program's active uniforms like every params site; as a prop the declaration applies deferred, so a bad name surfaces as a runtime warning here rather than a throw.
- The draw is attributeless: `vertexCount` vertices (default 3, the covering triangle) as triangles, positions fetched via `gl_VertexID`. The window is cleared to opaque black first, so geometry that does not cover it still presents a defined frame.

An identity program (`fragColor = texture(uSource, vUV)`) is pixel-identical to no shader at all. Swapping between two precompiled program handles compiles nothing; compile and link cost sits at the `compileShader`/`linkProgram` call sites. MCP `get_snapshot` renders the tree offscreen and shows the pre-shader image; the screen (and playback capture) shows the post-shader result.

One opt-in layer behavior:

- `previous` (default false): retains the last frame as a second layer the program samples as `uniform sampler2D uPrevious` - one-frame history (motion echo, frame differencing). Costs one extra window-sized texture while declared. Until a second frame exists `uPrevious` is opaque black. Declare the `uPrevious` uniform only together with this flag - without it the uniform defaults to unit 0 and aliases `uSource`.

Animating only the shader is cheap by design: frames where nothing changed but the shader's `params` skip the whole app pipeline - no layout, no repaint, no re-rasterization - and just re-run the pass over the retained layer. This happens automatically; any real change (tree content, a texture upload, a resize, a program swap) takes the full path again on that frame. Frames with `previous` declared always re-rasterize (the history must track the last frame). `get_gpu_resources` reports the skipped frames as `windowShader.passOnlyFrames`.

### Boundary shader

The `shader` prop on a `<view>` with `repaintBoundary="snapshot"` runs the view's rasterized subtree through a linked program and composites the result in its place - a region-sized effect (grade, warp, dissolve a panel) for the cost of one pass over the boundary's pixels:

```tsx
<view repaintBoundary="snapshot" shader={{ program: warp, params: { uAmount: amount() } }}>
  ...
</view>
```

The declaration is `{ program, params?, textures?, outset? }`; `null` (or omitting it) restores the plain snapshot. The boundary is required, not implied: a snapshot's semantics are the prop's real cost (retained pixels, crop at the layout box, re-rasterization on size and scale changes), so declaring `shader` without `repaintBoundary="snapshot"` does nothing except warn.

`outset` (logical px, default 0) adds a transparent margin on every side of the layout box for the effect to write into - glow, drop shadow, blur that bleeds past the edge. It grows the rasterized canvas and the composited quad by the margin; the subtree's own paint stays clipped to the layout box either way, so the margin always starts transparent. The pass simply sees the bigger `iResolution` - declare an app uniform (and pass the value through `params`) if the program needs to know where the content region sits. Changing `outset` reallocates the boundary's textures, so drive animations through `params`, not by animating the margin.

`previous` (default false) retains the prior rasterization of the subtree as `uniform sampler2D uPrevious`. It is source history, not output history, and it rotates when the content actually re-rasterizes - not per frame. That makes it transition material: on a content change, `uPrevious` holds exactly the old look and `uSource` the new, and a param can sweep a cross-dissolve between them (`mix(texture(uPrevious, vUV), texture(uSource, vUV), uMix)`). Two consequences of the rotation cadence, both by design: for a static subtree with animated params `uPrevious` equals `uSource` (the previous rasterization *is* the same content), and feedback/accumulation cannot be built from it - self-referential passes stay with `render: "manual"` targets. The history costs one canvas-sized texture while declared, samples transparent until the first rotation, and resets to transparent on a size or scale change. As with the window shader, only declare the `uPrevious` uniform together with the flag - without it the uniform stays at unit 0 and aliases `uSource`.

The program's contract matches shader targets, not the window pass:

- `uniform sampler2D uSource`, filled by name, is the subtree's rasterization - top-left origin like every sampled texture, and a target pass's `vUV` origin already matches, so unlike the window pass there is no flip anywhere (`vUV = p` in the covering-triangle vertex stage).
- `uniform vec2 iResolution`, filled by name, is the boundary in physical pixels.
- `params` and `textures` work as on the window shader, validated the same way; as a prop the declaration applies deferred, so a bad name surfaces as a runtime warning.
- The draw is one covering triangle; sampling outside the content clamps to the edge.

The pass is split from content invalidation: a change inside the subtree re-rasterizes the snapshot and re-runs the pass, while a params-only write re-runs just the pass against the cached snapshot. Animating an effect over a static panel therefore never re-rasterizes it - cheaper than the same panel without a boundary.

Three limits define the feature. The effect samples only the subtree's own pixels: anything that needs what is *behind* the panel (frosted glass over the background) is a different mechanism, not a use of this one. Hit-testing stays on layout geometry: a distortion moves pixels, not hit targets. And the view's own transform and group opacity apply to the composited result, so the program sees unrotated, opaque content - the property that makes the snapshot a good effect source.

---

## Utilities

### measureText

```ts
measureText(text: string, options?: MeasureTextOptions): { width: number, height: number }
```

Synchronously measures the rendered size of a text string. Accepts the same font options as `<text>`: `fontFamily`, `fontSize`, `fontStyle`, `fontWeight`, `maxLines`.

### getBoundingBox

```ts
getBoundingBox(node: { id: number }): BoundingBox | null
```

Returns the window-relative bounding box of a node from the most recently computed layout, or `null` if the node has no layout or has not been laid out yet. This is a snapshot - call it from `onLayout` or an event handler to get values for the current frame.

### setFocus

```ts
setFocus(nodeId: number | null): void
```

Programmatically moves focus to the given node, or clears focus when passed `null`. Triggers `onFocus` and `onBlur` handlers. When the focused node has an `onTextInput` handler, focus also scopes its text-entry session - but focus alone never raises an on-screen keyboard: where one would appear (a screen-keyboard platform with no physical keyboard attached), the session waits for a tap on the focused node or an explicit `startTextInput()`. Everywhere else (desktop, or any device with a physical keyboard) the session starts invisibly at focus, so text arrives from the first keystroke.

### startTextInput

```ts
startTextInput(): void
```

Begins text entry on the focused node, raising the on-screen keyboard where one is used. A tap on the focused node triggers this automatically; call it for any other interaction that should start typing - a remote's select on a focused field, a search button. Throws when the focused node has no `onTextInput` handler.

### textInputActive

```ts
textInputActive(): boolean
```

Whether a text-entry session is active on the focused node (text events flowing; the on-screen keyboard up, where one is used), as a reactive accessor. Distinct from focus: a field focused by navigation is not editing until a tap or `startTextInput()` begins the session - which is how a text field tells its focused and editing states apart (select starts editing in the former; Enter submits in the latter).

### focusedNode

```ts
focusedNode(): number | null
```

The id of the currently focused node, or `null`, as a reactive accessor: read it inside a tracked scope (JSX, a memo, an effect) to re-run when focus moves; a read in an event handler just sees the current value. `setFocus` is the only writer.

### getFocusables

```ts
getFocusables(): number[]
```

Node ids currently declaring the `focusable` prop, for building focus navigation (spatial/D-pad movement, tab order). A snapshot, not reactive; pair with `getBoundingBoxViewport` for their geometry. The prop declares candidacy only - navigation moves focus itself via `setFocus`.