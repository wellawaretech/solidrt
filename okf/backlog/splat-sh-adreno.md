---
title: The splat SH vertex shader draws nothing on Adreno 610 and breaks the program's reflection, at every degree and under both encodings
description: On the Samsung SM-T500 (Adreno 610, OpenGL ES 3.2 V@0502, 2021 driver) a splat cloud at SH degree 0 draws, but a cloud at degree 1, 2 or 3, as u8 or pq, does not - the first SH program of a process links and renders nothing (60 presents per second, the window crop all zero), every later SH program reflects zero active uniforms (addDraw refuses "no active uniform named uSplatRecords"), and the attribute reflection lists two nameless FLOAT inputs; the Pixel 7 (Mali) renders the same programs. Something in the SH block (uCamPos, transpose(mat3(uModel)), the second integer sampler, the basis arithmetic) breaks this compiler silently and wants a shader bisect on the device.
created: 2026-10-18
---

# The splat SH vertex shader draws nothing on Adreno 610

## Symptom

Found 2026-10-18 while pricing the SH palette
(okf/plans/splat-palette-sh.md) on the tablet:

- The train (1M splats, SH degree 0) draws and orbits on the tablet;
  a window-root readback crop holds 1854 distinct colors.
- The lizard at SH degree 3 as u8, loaded first in a fresh process:
  loads, no error, 49140 instances of the 16-splat quad drawn at 60
  presents per second, and the crop reads all zero - a black window with
  the HUD alone, as the human saw it. The same as pq.
- The next SH program in that process (degree 2 or 1, pq) fails at
  addDraw: `no active uniform named 'uSplatRecords' (active: )` - the
  program linked (link status true, no info log surfaced) and reflects no
  uniforms at all. Before alloy's reflection skipped nameless attributes
  (same day), the first SH program also failed the layout check with two
  nameless FLOAT attributes that the SH0 program never showed.
- 40 s of idle after start before the first SH load changed nothing
  about the empty render (one earlier run with degree 1 and 2 first read
  no errors, but was never checked for pixels).
- The Pixel 7 (Mali G710) draws every one of these bakes; the Linux
  client too.

## Shape

- Bisect the SH1 vertex source on the device: the `uCamPos` uniform, the
  `transpose(mat3(uModel))`, the second `usampler2D`, the basis
  expression, each removed in turn in a scratch material over the stock
  geometry, until the program reflects its uniforms and draws.
- Surface the driver: after a successful link, a program whose declared
  uniforms reflect as none should be an error naming the driver string
  and the source, not a silent `addDraw` refusal later; and the shader
  info log of a successful compile may carry the warning that explains
  it (read it in dev).
- The tablet also kills the client under memory pressure when clouds are
  swapped (2.7 GB; two engines with a cloud each during a reload, or a
  few loads in a row), which made this bisect slow; a probe for the
  shader question should load one small SH cloud.
