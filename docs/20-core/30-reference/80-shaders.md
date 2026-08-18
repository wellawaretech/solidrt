# Shaders

Two elements take a `shader` prop: `window` runs a program over the finished
frame, and `view` runs one over its own rasterized subtree. Both are
declarative - the pass exists while the prop is declared and the resources go
away when it is removed.

Everything about the program itself (compiling, linking, lifetime) belongs to
the raw shading layer: `compileShader` and `linkProgram` from
`@solidrt/core/gpu`.

## The shared contract

Both passes bind their input as `uniform sampler2D uSource` with a top-left
origin, like every sampled texture in the engine, and fill `iResolution` by
name with the pass size in physical pixels. Both fill `params` uniforms by
name, paced to the next real repaint: a number drives a scalar, and a flat
number array drives the declared GLSL type (2, 3 or 4 for `vec2`, `vec3`,
`vec4`, and 16 column-major for `mat4`).

## Window

{{ decl packages/core/src/types.d.ts WindowShaderProps }}

The window pass draws attributeless triangles with `vertexCount` vertices
fetched via `gl_VertexID`, defaulting to a single covering triangle.

## View

A boundary shader requires a snapshot boundary (`repaintBoundary="snapshot"`
or `"snapshot-no-aa"`). The cost is snapshot semantics, and it is kept
explicit: declared without one, the shader is ignored with a warning.

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
