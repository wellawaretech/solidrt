---
title: A splat cloud's SH3 texture is 48 bytes per splat where a palette holds it in 3
description: The .sol3s SH block stores a byte per coefficient (16/32/48 bytes per splat at degree 1/2/3, 48 MB at 1M splats with SH3) while sogs and Unity's importer quantize the 45-coefficient vectors to a k-means palette at 2-3 bytes per splat; a product-quantized encoding behind the SH block's `encoding` field would cut the dominant texture 12x, at the price of a bake-time k-means to engineer and a quality judgement of its own.
created: 2026-10-18
---

# A splat cloud's SH3 texture is 48 bytes per splat where a palette holds it in 3

## Symptom

okf/done/splat-record-compression.md left this as phase 2: at 1M
splats with SH3 the SH texture is 48 MB of the cloud's 84 MB resident,
and the lizard capture's file is 54 MiB of which 36 are SH bytes.
sogs (PlayCanvas) and Unity's importer cluster the SH vectors into a
palette (64k entries in sogs, on a GPU; Unity's in Burst jobs over
minutes) and store a label per splat.

## Shape

- Product quantization at bake: three 15-dimensional subspaces (the
  three bands, or three channel groups), 256 centroids each by k-means
  over the cloud's vectors - 3 bytes per splat plus a 46 KB codebook,
  SH3 from 48 MB to 3 MB. Plain TypeScript at bake scale: 1M x 256 x 15
  per iteration, seconds to a minute under bun.
- The header's SH block already carries `encoding: "u8"`; a `"pq"`
  encoding adds the codebook block and a label texture. The vertex
  stage fetches the three labels and the codebook rows (a small
  texture, cache-resident).
- Judge it as this item was judged: the snapshot diff against the u8
  bake and a human flip on a glossy capture (the lizard's metal).
