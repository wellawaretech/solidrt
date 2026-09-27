---
title: The splat fragment skips two of the reference rasterizer's rules
description: The 3DGS rasterizer caps each splat's alpha at 0.99 and, in the antialiased training mode (Mip-Splatting, gsplat's antialiased rasterization), scales opacity by sqrt(det(cov) / det(cov + dilation)); the shipped fragment applies neither, so captures trained antialiased read slightly too opaque at small sizes and a fully opaque splat blocks everything behind it instead of leaking 1%.
created: 2026-09-27
---

# The splat fragment skips two of the reference rasterizer's rules

## Symptom

okf/notes/gaussian-splats-against-the-field.md: two per-splat rules of
the reference rasterizer are missing from SPLAT_FRAGMENT / the vertex
stage:

- `alpha = min(0.99, opacity * gaussian)`: the reference never lets a
  splat fully occlude, and trained opacities assume the cap.
- The antialiasing dilation (LOW_PASS, 0.3 px^2) without the
  Mip-Splatting compensation factor `sqrt(det(sigma2d) / det(sigma2d +
  dilation))`. Vanilla 3DGS trains without the compensation and matches
  what ships; nerfstudio's `antialiased` mode and Mip-Splatting train
  WITH it, and those captures read too opaque at small footprints here.

## Shape

The cap is one line. The compensation is a per-capture fact the bake
should carry (a header flag from the trainer's mode, `--antialiased` on
the tool) and the vertex stage multiplies into the alpha. Judge both
against a reference render, side by side (the sRGB blend verdict's
protocol).
