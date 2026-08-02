---
type: backlog-item
title: R8 / indexed uploadTexture format
description: uploadTexture is RGBA8-only, so palette-indexed content must pack four indices per RGBA texel and unpack in the shader - free only when the width divides by four; an R8 upload format makes the authentic path the cheap path for every emulator and retro port. Measured 2.45x on a whole game tick.
status: open
timestamp: 2026-08-02T00:00:00Z
---

# R8 / indexed uploadTexture format

Source: the wasm game-port demo feedback (2026-08-02), where moving the
per-pixel palette lookup onto the GPU was the single largest optimisation
in the whole port, by a distance: 6.75 -> 2.75 ms/tick (the 32bpp
conversion was ~59% of an entire tick), a 4x smaller per-frame upload, and
palette effects (damage flashes, fades) became a 1 KB palette upload
instead of touching every pixel.

The friction: with no single-channel texture format, the route is packing
4 indices per RGBA texel and unpacking in the fragment shader. The port
paid no repacking only by luck - its 320-wide framebuffer is exactly 80
RGBA texels - while any width not divisible by 4 must repack per frame on
the CPU, eating much of the win. R8 is core in GL ES 3.0, so engine-side
this is a format parameter at the upload/create site.

Userland already proves the shader half (the palette lookup, and the
`vec4(t.bgr, 1.0)` swizzle for alpha-less BGRA sources, live there
anyway); the format is the missing piece. Distinct from
[[gpu-compressed-textures]] (ETC2, further out): this one is small and
nearer-term.
