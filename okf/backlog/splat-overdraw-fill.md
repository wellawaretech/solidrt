---
title: A million splats at phone full resolution spend the frame in fill
description: With the instancing frontend gone the Pixel 7 draws 1M splats in ~36 ms at 1079x2399, of which ~24 ms is fragment work - the gaussian falloff, the discard and the per-tile blend over the deep overdraw of stacked translucents - and the buffer format does not touch it on a tiler; the levers are overdraw cuts (a tighter extent than e^-4, a higher minimum-alpha cut, a per-splat screen-size cull, a count dial by distance), each a picture-quality trade that wants the human side-by-side.
created: 2026-09-27
---

# A million splats at phone full resolution spend the frame in fill

## Symptom

okf/done/gaussian-splats.md, stage D: after the indexed restructure the
1M train scene takes ~36 ms per frame on the Pixel 7 at full resolution
and ~10 ms at half, so ~24 ms of the full-res frame is fill. The
display-space A/B showed the rgba8 buffer buys nothing on a tiler (the
blend never leaves the tile), so this is fragment throughput over
overdraw, not bandwidth. 300k splats fit the frame at 60 Hz already;
this is the wall for phone captures past a few hundred thousand.

## Levers, all quality trades

The structural answer - front-to-back tile blending with early
termination, what the compute renderers do - needs GLES 3.1 compute, and
**3.1 is not on the table** (decided 2026-09-27); this stays a GLES 3.0
problem, so the only lever is drawing fewer fragments.

- The quad extent (SPLAT_EXTENT, e^-4 today) and the fragment's minimum
  alpha cut (MIN_ALPHA, 1/255): each shrinks every splat's footprint.
- A screen-size cull in the vertex stage (splats under a pixel) and a
  count dial by camera distance (the records are importance-sorted).
- The judge is a human side-by-side against the untrimmed render; the
  probe's `bench` ladder and the phone protocol
  (okf/notes/gaussian-splats.md) are the instrument.
