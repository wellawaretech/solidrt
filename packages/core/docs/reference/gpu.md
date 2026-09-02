# GPU

`@solidrt/core/gpu` is the programmable half of rendering: textures you
upload, shader passes that render into textures, and pipelines that draw
your own geometry into them. Everything it produces is a texture id, and a
texture id goes anywhere a texture goes: `<texture src={id} />` to display
it, a `textures` binding to sample it from another pass, `readTexture` to
bake it.

The imperative primitives (`uploadTexture`, `setTargetParams`, `addDraw`,
`destroyTexture`, ...) are the `flux:gpu` module and are documented in the
[runtime reference](/runtime/gui/gpu/). This page is the reactive layer
over them and the contracts that hold across both.

## The model

A target is retained, not redrawn. Creating one renders it once; after that
it re-renders exactly when something it depends on changes - a param, a
bound texture, a buffer it draws from, its size - and never otherwise. A
static shader costs zero passes per frame.

Sampler bindings are live dependencies. A target bound as another's
`textures` input re-renders its consumer whenever it re-renders itself, in
topological order, so a chain (a plasma pass feeding a mesh pipeline) is
driven by writing only the first target's uniforms. A cycle throws.

Targets whose pass is state rather than a function of its inputs
(accumulation, feedback, simulation) opt out with `render: "manual"`: the
runtime never renders them, only `renderTarget(id)` does, in call order,
normally from `onFrame`.

## Three layers

- **Fused.** `createShaderTexture` compiles a fragment source and renders it
  fullscreen; `createPipelineTexture` does the same over your own vertex
  buffer. One call, one texture, program and target sharing a lifetime. The
  shader-toy shape.
- **Raw.** `compileShader`, `linkProgram`, `createRenderPipeline`, then
  `createShaderTarget` per target. A pipeline (program plus draw state:
  attribute layout, topology, blend, cull, depth) backs any number of targets
  and compiles nothing per target. Reach for it when programs are shared or
  lifetimes differ.
- **Draw list.** `createDrawTarget` renders many entries in one pass, each
  entry its own pipeline, buffer, params and textures, added and removed with
  `addDraw` / `removeDraw` and sorted with `setDrawOrder`. A scene, in one
  texture.

`@solidrt/3d` builds its scene graph on the third layer; nothing there is
hidden from this one.

## Creating

Every `create*` helper frees its resource when the reactive owner that
created it is disposed. Created outside a reactive scope (after an `await`,
in an event handler with no owner) nothing is registered and the matching
`destroy*` is yours to call; `{ autoFree: false }` opts out of the auto-free
inside a scope for resources rebuilt by hand. `label` names the resource in
the dev tooling's GPU inventory and in engine log messages.

{{ decl packages/core/src/gpu.ts createTexture }}

{{ decl packages/core/src/gpu.ts createCubeTexture }}

{{ decl packages/core/src/gpu.ts createMutableTexture }}

{{ decl packages/core/src/gpu.ts createShaderTexture }}

{{ decl packages/core/src/gpu.ts createPipelineTexture }}

{{ decl packages/core/src/gpu.ts createShaderTarget }}

{{ decl packages/core/src/gpu.ts createDrawTarget }}

{{ decl packages/core/src/gpu.ts createShaderTextureMemo }}

{{ decl packages/core/src/gpu.ts ShaderSpec }}

## Buffers

{{ decl packages/core/src/gpu.ts createBuffer }}

{{ decl packages/core/src/gpu.ts beginBufferWrite }}

{{ decl packages/core/src/gpu.ts writeBuffer }}

A buffer's size is fixed at creation, an entry's buffers are not: `setDraw`
(single-draw targets) and `setDrawBuffers` (draw-list entries) re-point a
role the entry already fills - `buffer`, `indexBuffer` + `indexFormat`,
`instanceBuffer` - at another buffer. That is how a population grows past
its reservation: create a larger buffer, write it, swap, destroy the old.
The swap is replace-only (roles are pipeline layout state) and keeps the
entry's draw range, rechecked against the new sizes.

## Uniforms

Uniforms are driven by name. Declaratively, `<texture src={id}
params={{ uTime: t() }} />` writes the target's params paced to the next real
repaint, which is the preferred form; `setTargetParams` and `setDrawParams`
are the imperative forms for targets no element holds. A number drives a
scalar, a flat number array drives the declared type (2, 3, 4 for `vec2`,
`vec3`, `vec4`, 16 column-major for `mat4`, element size times length for
arrays). `textures` binds `sampler2D` uniforms to texture ids the same way.

Every write is validated at the call site against the linked program:

- a name the program never declares throws, listing the active uniforms;
- a value whose length does not fit the declared type throws;
- a `sampler2D` named in `params` (or a non-sampler named in `textures`)
  throws;
- a name the source declares but the compiler optimized out is accepted
  with a warning and skipped, so one param object can drive several shader
  variants that do not all read every uniform.

On a draw target, target-level `params` and `textures` are the shared set
every entry reads (a camera's view-projection written once per move), applied
before each entry's own, and a shared name only some entries declare applies
where declared.

The fused preamble declares exactly what the runtime provides: `#version
300 es`, precision, `vUV` (fragment path), `fragColor`, and `iResolution`,
filled with the target size in physical pixels. Anything app-driven - a time
uniform - is the source's own declaration, driven like any other uniform, so
forgetting to drive it is a compile error rather than a value stuck at zero.
A source starting with its own `#version` line gets no preamble and compiles
as written.

## Sampling

`filter` (`"linear"` default, `"nearest"`), `wrap` (`"clamp"` default,
`"repeat"`) and `mipmap` (`false` default) are declared at creation and are
a property of the texture id: `<texture>` display and shader sampling follow
the same state, so a nearest texture upscales with hard pixels everywhere.
Changing any of them means a new id.

Without a mip chain, shader sampling of a minified texture skips texels and
aliases (3d surfaces at distance, a target sampled at a fraction of its
size). `mipmap: true` keeps the chain on the id and the runtime rebuilds it
after every upload (data textures) and every render (targets) - there is
nothing to schedule. Shader sampling then minifies through it (trilinear
for `"linear"`, per-level nearest for `"nearest"`). The `<texture>` display
draw samples the full-size level only, so a supersampled target shown
through `<texture>` should stay at 2x. Rebuilding is one GPU pass per
upload or render; a per-frame texture pays it per frame.

One binding can deviate: a `textures` value may be `{ id, filter?, wrap? }`
instead of a bare id, sampling that texture with a different filter or
wrap in this binding only - blur a `"nearest"` atlas linearly, tile a
clamped target in one consumer. The texture's own state stays what
`<texture>` paints and what every other binding uses. `mipmap` is not
overridable: the chain either exists on the id or it does not.

A cube map (`createCubeTexture`) is the same id currency with a different
sampler: declare `uniform samplerCube` and look it up by direction. The
declared sampler type is what a binding is checked against - a cube map on
a `sampler2D` throws, as does a 2D texture on a `samplerCube` - and a cube
map is sampler-only: nothing displays, reads back, copies or re-uploads it.
`wrap` does not apply to it (cube filtering is seamless across faces);
`filter`, `mipmap` and `anisotropy` do.

## Blending

Combining passes is a render-tree job: stack `<texture>` elements and set
`blendMode` on them. Within one pipeline draw, `blend` on the pipeline
decides how overlapping geometry combines: `"add"` and `"multiply"` are
order-independent (glows, shadows); `"alpha"` composites over in draw-list
order with premultiplied output, normally after the opaques with
`depthWrite: false`, and nothing sorts for you (`setDrawOrder` does, or a
scene layer above). Anything else overwrites.

## The pixel contract

Three facts hold for every texture and target:

- **Clip space is y-down.** `gl_Position` y = -1 is the top of the target.
  A vertex stage carrying camera-up geometry negates y or folds the flip into
  its projection. The fragment path absorbs the flip, so `vUV` is 0..1 with
  a top-left origin.
- **Color is premultiplied alpha.** A target's RGB is already multiplied by
  its A: write `vec4(rgb * a, a)`. `vec4(rgb, a)` composites as opaque.
  `clearColor` is premultiplied too, and so are uploaded pixels:
  `decodeImage` premultiplies at the codec boundary (image files store
  straight alpha) and `encodeImage` converts back, so pixels inside the app
  are premultiplied everywhere.
- **Values are non-linear RGBA8** (or `"r8"` for single-channel data), with
  no color-space conversion anywhere. Filtering and blending operate on the
  stored values.

## UI as a texture

`snapshotTexture(ref)` (from `@solidrt/core`) is the texture id behind a
`repaintBoundary="snapshot"` view: the subtree's rasterized pixels, usable
wherever a texture id is - a `<texture>`, a shader or draw target binding,
a 3d material. The id is allocated on first call and stable for the node's
lifetime; the runtime re-points it after every re-rasterization, which
happens only when the subtree changes, so a static panel sampled by an
animated consumer costs no repaint. Premultiplied, top-left origin, cropped
to the layout box. Empty until the boundary's first paint. The boundary owns
it: `destroyTexture` on it throws, and unmounting the boundary releases it
on the deferred-destroy path (a consumer still sampling it keeps the last
pixels). A boundary showing its own texture is not a feedback loop: its
rasterization is the change, so it does not re-invalidate itself.

## Readback

`captureSnapshot` renders a render-tree node to pixels and `readTexture`
reads any texture back; both resolve `{ width, height, data }`. This is the
one-shot bake path (a glyph atlas, a processed image), paid for with a
readback stall and a paint pass of latency per call. Never per frame; to
feed live content into a shader, bind a texture that updates in place
instead - for a UI subtree, `snapshotTexture`.

## Limits

`limits` holds the device ceilings queried at startup: maximum texture and
target size, cube map face size, sampler inputs per pass, vertex attributes
per pipeline. Creates
and binds validate against them and throw naming the limit; read them to
size within the device instead.
