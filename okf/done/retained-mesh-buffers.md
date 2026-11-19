---
title: Keep a path's mesh on the GPU across frames
description: anvil transforms and uploads every cached path mesh on every frame (about 10 ms of raster thread per frame for thirty 64-point strokes on the armv7 TV, 29 ms at 256 points, with no tessellation); a mesh kept in a GPU buffer across frames, drawn with its transform as a uniform or a per-draw matrix, would make a retained screen of paths cost its draw calls only.
created: 2026-11-19
---

# Keep a path's mesh on the GPU across frames

Part of stage 5 of [own-renderer](../plans/own-renderer.md). Closed 2026-11-19 (1a3855dd, the TV read in the stage's Findings): the static route at 256 points holds 50 fps with 0 missed presents and the raster thread at 21 percent, where it read 85 to 91 percent with 84 to 136 missed; the cheap orphan check was skipped because the retained design removes the transform and the upload at once.

The stage 4 design (okf/plans/own-renderer.md) chose per-vertex device
positions, color and clip in a stream buffer rebuilt every draw: 40 bytes
a vertex, every cached mesh transformed on the CPU and uploaded again
each frame, so paths batch across colors and clips with no GPU-side
state. The TV read (stage 4 findings) priced it: with every mesh cached
(`rendererTessellatedPerFrame` 0) the raster thread spends about 10 ms a
frame on thirty strokes of 64 points (about 7,800 vertices, 312 KB) and
about 29 ms on thirty of 256 points (about 15,000 vertices, 600 KB),
where the stage 3 paint bench's raster thread spent 2.9 ms on as many
draw calls. The desktop hides it (9 percent of a core at 256 points).
The per-frame `glBufferData` of a buffer the previous frame's draw may
still read is the suspect beside the transform itself: a Mali driver
can wait or copy there.

## Done looks like

- A static screen of paths costs the raster thread its draw calls and
  the walk, not its vertex count: the TV's static route at 256 points
  holds 50 fps with the raster thread under 20 percent.
- Pixels unchanged (zero diff against the branch's output before).

## Involves

- First the cheap check: orphan the stream buffers (`glBufferData` with
  NULL at the frame's size before the upload, or two buffers alternated
  per frame) and read the TV again; the transform alone may be a small
  part.
- Then, if the upload is the cost: a mesh keeps a GPU vertex and index
  buffer (allocated on the raster thread at its first draw, deleted
  through the deletion queue when the `Arc<Mesh>` drops, as textures and
  paint shader programs are), positions in the path's space; the draw
  sets the path's 2x3 map, color and clip as uniforms, one draw call per
  path (no batching across paths) or a small uniform-array slot table
  for a batch of paths. The walk emits a mesh draw by handle instead of
  vertices; the mesh program gains a map uniform. Culling and the scale
  bucket stay as they are.
