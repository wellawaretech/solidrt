---
title: A ramp atlas so gradient batches merge
description: Every gradient draw is a sealed batch of its own because the one ramp texture is rewritten per batch, and on the Mali-T860 that rewrite stalls the pipeline (thirty gradient rects a frame run at 29 fps where solid ones run at 50 with the same GPU time); a row-per-gradient ramp atlas uploaded once per draw would remove the stall and let gradient quads and meshes batch like solid ones.
created: 2026-11-19
---

# A ramp atlas so gradient batches merge

Part of stage 5 of [own-renderer](../plans/own-renderer.md). Closed 2026-11-19 (59baf506 the atlas, c4481ee3, e9b2d2c3 and f7619a58 the gradient program, the TV reads in the stage's Findings): one upload per draw, thirty plain gradient panels draw in three calls at 21.8 ms of GPU where stage 4 sealed a batch per gradient at 21.3; the 1.5 ms residue against stage 4 is the atlas sample and is recorded, not chased.

anvil evaluates a gradient per device pixel from a `RAMP_WIDTH` x 1 ramp
texture that the executor rewrites for every gradient batch
(`exec.rs` `gradient_state`), so the walk seals a batch per gradient op
(`walk.rs`): a screen of thirty gradient panels with a path in each draws
in 61 calls where solid panels would draw in three (the stage 4 desktop
read in okf/plans/own-renderer.md). Glyph gradient runs seal the same way.

The stage 4 TV read made it more than a draw-call count: `glTexSubImage2D`
into the ramp the previous batch's draw still samples makes the Mali
driver wait or copy per batch. The path bench's live route with plain
gradient rects reads 29 fps with 204 to 211 missed presents per 10 s and
15.6 ms of GPU a frame; the same route with solid rects reads 50 fps with
0 to 6 missed and 14.1 ms. Glyph gradient runs rewrite the ramp the same
way since stage 2, so gradient text pays it too.

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
