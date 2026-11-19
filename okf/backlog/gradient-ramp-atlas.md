---
title: A ramp atlas so gradient batches merge
description: Every gradient draw is a sealed batch of its own because the one ramp texture is rewritten per batch; a row-per-gradient ramp atlas with the row index on the instance would let gradient quads and meshes batch like solid ones, turning the path bench's 61 draw calls a frame into a few.
created: 2026-11-19
---

# A ramp atlas so gradient batches merge

anvil evaluates a gradient per device pixel from a `RAMP_WIDTH` x 1 ramp
texture that the executor rewrites for every gradient batch
(`exec.rs` `gradient_state`), so the walk seals a batch per gradient op
(`walk.rs`): a screen of thirty gradient panels with a path in each draws
in 61 calls where solid panels would draw in three (the stage 4 desktop
read in okf/plans/own-renderer.md). Glyph gradient runs seal the same way.

## Done looks like

- A `RAMP_WIDTH` x N atlas of ramps, one row per distinct gradient in the
  frame (keyed on the ramp bytes, or on the `Arc<RampGradient>`), uploaded
  once per draw; the instance (and the mesh vertex, or the mesh batch)
  carries the row and the rows of its device map, so consecutive gradient
  draws share a batch and solid draws join them.
- The path bench's live route at 30 panels draws in a handful of calls,
  pixels unchanged (zero diff against the branch's output before).

## Involves

- The instance layout: the gradient's two affine rows (6 floats) and the
  ramp row would take two more vec4 attributes (eleven of GLES 3.0's
  sixteen), or a per-batch uniform array indexed by a slot on the
  instance; a mesh batch takes the same choice.
- The variants: `fill_gradient`, `shape_gradient`, `mesh_gradient` read
  the rows from the instance instead of uniforms; the glyph program the
  same.
- An atlas height cap and the eviction when a frame holds more gradients
  than rows (fall back to sealing).
