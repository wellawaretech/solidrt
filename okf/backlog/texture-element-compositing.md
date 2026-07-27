---
type: backlog-item
title: Paint properties on the texture element
description: texture/d-texture carry no PaintProps, so two GPU layers cannot be composited additively in the tree; every layered effect turns into an extra full-screen shader pass.
status: open
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

Related: [gpu-pipeline-extensions](gpu-pipeline-extensions.md) covers
blending *inside* one pipeline, which is a different problem - that one is
about translucent geometry within a draw, this one is about composing
finished layers in the render tree.
