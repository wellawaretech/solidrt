---
type: backlog-item
title: Paint properties on the texture element
description: texture/d-texture carried no PaintProps, so two GPU layers could not be composited additively in the tree; fixed by giving the texture kind the same PaintState every other kind has, after verifying what a paint actually does to a raster draw.
status: done
timestamp: 2026-07-27T00:00:00Z
---

# Paint properties on the texture element

`texture` and `d-texture` are typed as `TextureProps & LayoutProps`, where
TextureProps is `Position & PointerProps` plus src/fit/srcRect/params. Every
vector primitive (rect, oval, path, svg, and their d- forms) extends
PaintProps and can therefore set `blendMode`. The one raster element in the
tree cannot. Since textures are also how images reach the screen, this is
not just a GPU-app concern.

The practical consequence is that GPU layers cannot be stacked. Draw a solid
pass and an additive glow pass into two pipelines and there is no way to say
"composite the second one with plus" - the only route is a third
createShader that samples both targets and does the blend in GLSL, which
means an extra full-screen pass, a hand-written compositor per effect, and
manual premultiplication rules, to reproduce something Impeller already does
in its own paint. `opacity` is reachable (wrap in a `<view>`, TransformProps
group opacity), but that costs a save-layer and only covers the fade case.

Evidence: projects/organism. Filling the flower's petals into a real surface
gives up the airy, gauzy quality that partial point coverage produced for
free. The natural recovery is a solid mesh pass plus a sparse additive
sparkle pass on top - two `<texture>` elements, one blendMode - and that is
exactly the thing the type says no to.

Scope question to settle first: is this just plumbing the existing PaintProps
through to the texture node's paint call in the renderer, in which case
blendMode, and possibly a tint `color` and `drawStyle`-free subset, come
along at nearly no cost? Or does the texture node take a different draw path
where the paint is not threaded through? If it is the former, the narrow
version (blendMode only, on both texture and d-texture) is worth doing on
its own.

# Resolution

The scope question resolved to the first branch: plumbing, nothing more.
`Texture::build` already constructed a `Paint` and handed it to
`draw_texture_rect`, it just threw away the chance to configure it. The one
thing actually missing was an arm in `ElementKind::paint_mut` (rendertree
mod.rs), without which the property adapter never offered a paint prop to a
texture and `blendMode` came back as an unknown property.

Done as the full `PaintState`, not a narrow blend-mode field, so the texture
kind reads like every other kind. That choice depended on what a paint does to
a raster draw, which was unknown and is now verified on the real GL path by
`alloy/examples/texture_paint.rs` (nine asserting cases plus a source-texture
readback):

- the paint's RGB does not tint, and a gradient color source does not replace
  the texture. Only alpha reaches the draw, as an opacity multiplier. This is
  what made adoption safe at all: `PaintState::default()` carries grey 0.5,
  and it leaves pixels untouched.
- `drawStyle` and the stroke fields are ignored.
- `blend_mode` applies, which is the whole point.
- texture alpha is stored premultiplied. Additive composites therefore need no
  manual premultiplication, but a layer's own fragment output must itself be
  premultiplied or it over-adds.

Sites: `paint: PaintState` on Texture and `to_paint()` in its build (alloy
rendertree kinds/texture.rs, using `to_paint` rather than `to_paint_in` since
color sources are ignored and resolving a box-relative gradient would allocate
one per frame for nothing); the `paint_mut` arm; `TextureProps extends
PaintProps` with the applicable subset documented (packages/core types.d.ts);
one unit test for the accessor arm. Both `texture` and `d-texture` get it, as
they share the Buildable and the props type.

Two limits worth knowing before designing on top of this. Separate pipelines
have separate depth buffers, so an additive overlay is never occluded by the
base layer's geometry - fine for haze and bloom, wrong for anything that
should hide behind near geometry. And a blend mode inside a
`repaintBoundary="snapshot"` composites against the boundary's own offscreen
surface rather than the backdrop; that is pre-existing behaviour for every
kind, not new here, but it bites when the additive layer and its intended
backdrop end up on opposite sides of a boundary.

Related: [gpu-pipeline-extensions](gpu-pipeline-extensions.md) covers
blending *inside* one pipeline, which is a different problem - that one is
about translucent geometry within a draw, this one is about composing
finished layers in the render tree.
