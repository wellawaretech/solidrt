---
title: A splat cloud is resident at 32 bytes per splat plus half-float SH where the field ships 8-16
description: The .sol3s record is 32 bytes and SH bands are uncompressed halves (32/48/96 bytes per splat), so a 1M SH3 cloud is ~130 MB of textures and key copy on the phone; spz, sogs and ksplat store the same splat in 8-16 bytes with chunk-quantized positions and 8-bit SH, and a compressed record would also shrink the download and the texture fetch.
created: 2026-09-27
---

# A splat cloud is resident at 32 bytes per splat plus half-float SH where the field ships 8-16

## Symptom

okf/notes/gaussian-splats-against-the-field.md: phone captures of
800k-1M splats with SH3 cost ~130 MB resident (the 96 MB SH texture, the
32 MB record texture, the key copy) and a 100 MB download; the
compressed formats the capture apps export (spz) and the viewers ship
(PlayCanvas sogs, ksplat) hold the same cloud in ~30 MB.

## Shape

- Positions quantized per spatial chunk (sogs/spz style: a chunk's bounds
  plus 16-bit or 24-bit offsets), covariance halves stay, color rgba8
  stays, SH coefficients as 8 bits around zero (spz's `(byte - 128) /
  128`), decoded in the vertex stage from `rgba32ui` texels.
- The core's sort key still wants float positions: decode at load into
  the 12-byte key copy (okf/backlog/splat-key-copy.md), or key on the
  chunk-quantized value.
- The bake stays pack-time; the file version bumps.
