# Shaders

Three places take a shader. The `shader` prop on `window` runs a program
over the finished frame. The `shader` prop on `view` runs a program over the
rasterized subtree of the view. The `shader` element draws a paint shader
into the frame. All three are declarative: the shader draws while it is
declared, and its resources go away when it is removed.

The window and view programs come from the raw shading layer:
`compileShader` and `linkProgram` from `@solidrt/core/gpu`. The `shader`
element takes a paint shader from `createPaintShader` (see Paint below).

## The shared contract

The window and view passes bind their input as `uniform sampler2D uSource`
with a top-left origin, like every sampled texture in the engine, and fill
`iResolution` by name with the pass size in physical pixels. Both fill `params` uniforms by
name, paced to the next real repaint: a number drives a scalar, and a flat
number array drives the declared GLSL type (2, 3 or 4 for `vec2`, `vec3`,
`vec4`, and 16 column-major for `mat4`).

## Window

{{ decl packages/core/src/types.d.ts WindowShaderProps }}

The window pass draws attributeless triangles with `vertexCount` vertices
fetched via `gl_VertexID`, defaulting to a single covering triangle.

## View

A boundary shader requires a snapshot boundary (`repaintBoundary="snapshot"`).
The cost is snapshot semantics, and it is kept explicit: declared without
one, the shader is ignored with a warning.

{{ decl packages/core/src/types.d.ts ViewShaderProps }}

The pass is split from content invalidation, so animating `params` over a
static subtree re-runs only the pass against the cached snapshot and never
re-rasterizes it.

The effect samples the subtree's own pixels and nothing else. Grading,
warping and dissolving a panel work; anything that needs what is *behind* the
panel does not. Hit testing stays on layout geometry, so a distortion moves
pixels, not hit targets.

`outset` grows the rasterized canvas by a transparent margin on every side,
for effects that write past the edge such as a glow or a drop shadow.

## Paint

A paint shader is a fragment program that a `shader` element draws straight
into the frame. It draws as one quad over the box of the element. It draws
under the transform and the clips of the frame. It uses no offscreen texture.

Make the program with `createPaintShader`. Give the id to `src`.

```tsx
let plasma = createPaintShader(glsl`
  uniform float uTime;
  void main() {
    vec2 p = vUV * 2.0 - 1.0;
    fragColor = vec4(vec3(0.5 + 0.5 * sin(p.x * 6.0 + uTime)), 1.0);
  }
`)

<shader src={plasma} params={{ uTime: t() }} width={240} height={160} />
```

The fragment contract is the contract of `createShaderTexture`. Thus a source
for a shader texture works without changes:

- `vUV` is 0..1 over the box. Its origin is the top-left corner.
- `iResolution` is the box in device pixels.
- `fragColor` is premultiplied. The runtime composites it source-over.
- `params` sets the uniforms by name. `textures` sets the `sampler2D` inputs
  by name.
- The source has no `#version` line. The runtime supplies the header.
- Read the position from `vUV` or `vUV * iResolution`. Do not read
  `gl_FragCoord`: in the frame it is the window pixel, not the box pixel.

{{ decl packages/core/src/types.d.ts ShaderProps }}

Use the correct route for each program:

- A program that only generates the pixels of one box is a `shader` element.
- A program whose output something else samples is a shader texture.
- A program that samples its own output (`uSource`, `uPrevious`, a feedback
  pass) is a shader texture.

A shader texture draws into a texture first, and a `texture` element then
draws that texture. A paint shader removes these costs: one render pass and
one texture for each panel, and the resampling of that texture. It draws at
the resolution of the box.

`destroyPaintShader` frees a paint shader. Inside a reactive scope,
`createPaintShader` frees it when the owner is disposed.
