---
type: backlog-item
title: A snapshot boundary's retained texture as a texture id
description: repaintBoundary="snapshot" already keeps its subtree's rasterization in an adopted texture, but only the boundary shader can sample it; exposing it as an ordinary TextureId that updates as the subtree repaints would make any UI subtree usable as live content in the GPU stack - the load-bearing piece of UI-on-3D-geometry.
status: open
timestamp: 2026-08-05T00:00:00Z
---

# A snapshot boundary's retained texture as a texture id

`repaintBoundary="snapshot"` already rasterizes its subtree into an adopted
texture and keeps it until the subtree changes (`raster/capture.rs`, shared
with node captures; re-rasterized in place at unchanged dimensions per
`raster/cmd.rs`). Two consumers can reach that pixel data today and neither
is general:

- the boundary `shader` prop, which binds it as `uSource` for one pass and
  composites the result in place;
- `captureSnapshot(nodeId)`, which renders a node into a fresh texture id -
  but one-shot and async, so it is a still, not a feed.

What is missing is the obvious third: **the boundary's own retained texture,
as an ordinary `TextureId`, updating as the subtree repaints.** Everything
in the GPU stack takes texture ids as its universal currency, so that one
addition makes any UI subtree - text, layout, components, images, an entire
running screen - available as live content to shader targets, draw targets
and 3D materials, at the cost of a boundary that already exists.

The motivating consumer is UI mapped onto 3D geometry (see
../research/3d-differentiators.md, section 3): real interactive panels on
curved or animated meshes, sharing the scene's depth buffer. That is a
capability the browser stack cannot offer at all, and the damage model makes
it cheap here - the boundary re-rasterizes only when its subtree changes, so
a static panel on a spinning mesh costs one texture rather than a repaint
per frame. Secondary consumers are just as plausible: feeding a UI subtree
through the app's own multi-pass chain, or into a shader target that a
sibling element displays.

Shape questions to settle when picked up, in rough order:

- **Identity and lifetime.** The boundary owns the texture and replaces it
  on layout-size or display-scale change, so a naive id goes stale exactly
  when content resizes. Either the vended id is an indirection the runtime
  re-points at the current backing, or the id is invalidated and the app
  must re-acquire (which pushes a reactive dependency onto every consumer).
  The indirection reads better against the universal-currency rule.
- **Ownership against `destroyTexture`.** The app must not free a texture
  the rendertree owns. Options: reject destroy on borrowed ids with a clear
  error (the format-as-id-state precedent), or hand out a retaining handle.
  Whatever is chosen must survive the boundary being unmounted while a draw
  entry still references the id - the deferred-destroy work
  ([gpu-deferred-texture-destroy](gpu-deferred-texture-destroy.md)) is the
  relevant machinery.
- **Acquisition API.** A node-id function mirroring `captureSnapshot`
  (`boundaryTexture(nodeId)`), or a reactive primitive in the `createX`
  family that tracks validity. The reactive form fits the consumer better,
  since a material wants to re-bind if the backing changes.
- **First frame.** The texture does not exist until the subtree has been
  rasterized once. Either acquisition is async like `captureSnapshot`, or
  the id is valid immediately and samples empty until first paint. The
  second keeps consumers synchronous but needs a documented contract.
- **Ordering within a frame.** A target sampling the boundary must render
  after that boundary re-rasterizes. This is the same hazard as
  [gpu-target-dependency-propagation](gpu-target-dependency-propagation.md)
  (pixel-observing commands flush first) and should reuse that mechanism
  rather than grow a second one.
- **Declaring intent.** Only boundaries someone asked for should keep a
  texture alive for external sampling; a bare `repaintBoundary="snapshot"`
  used for its own sake should not become externally retained by accident.
  Acquisition itself can be the signal.
- **Sampling contract.** Top-left origin, premultiplied, cropped to the
  layout box, edge-clamped - already what the boundary shader's `uSource`
  documents, so this is a matter of restating it rather than deciding it.

Out of scope here: routing pointer events back through a mesh into the
mapped subtree. That is the picking half of the same feature and belongs
with the scene-graph raycast work (../research/scene-graph-3d.md).

Related: [gpu-deferred-texture-destroy](gpu-deferred-texture-destroy.md),
[gpu-target-dependency-propagation](gpu-target-dependency-propagation.md),
[capture-detached-nodes](capture-detached-nodes.md),
../research/3d-differentiators.md.
