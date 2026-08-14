---
title: R8 / indexed uploadTexture format
description: Done 2026-08-03 - createTexture/createMutableTexture accept format "r8" (1 byte/pixel, any width, alignment-free); the format is id state sizing every later upload/resize. Makes the authentic path the cheap path for every emulator and retro port - measured 2.45x on a whole game tick when the lookup moved to the GPU.
created: 2026-08-02
completed: 2026-08-03
---

# R8 / indexed uploadTexture format

Source: the wasm game-port demo feedback (2026-08-02), where moving the
per-pixel palette lookup onto the GPU was the single largest optimisation
in the whole port, by a distance: 6.75 -> 2.75 ms/tick (the 32bpp
conversion was ~59% of an entire tick), a 4x smaller per-frame upload, and
palette effects (damage flashes, fades) became a 1 KB palette upload
instead of touching every pixel.

The friction: with no single-channel texture format, the route was packing
4 indices per RGBA texel and unpacking in the fragment shader. The port
paid no repacking only by luck - its 320-wide framebuffer is exactly 80
RGBA texels - while any width not divisible by 4 must repack per frame on
the CPU, eating much of the win.

## Shipped (2026-08-03)

`format?: "rgba8" | "r8"` on `createTexture`/`createMutableTexture`
(default rgba8). The format is a property of the id like the sampler state:
fixed at creation, sizes every later `uploadTexture`/`resizeTexture` frame
(width*height*1 for r8), survives id-stable resizes, and is surfaced in the
get_gpu_resources inventory. R8 uploads set GL unpack alignment 1 (restored
to 4 after, for Impeller's own uploads), so ANY width works - the packing
trick and its width%4 tax are gone. GLES 3.0 core, no extension gate.

Contract notes, documented in flux-types gpu.d.ts and docs/core.md: a
`sampler2D` reads an r8 texture as `(v, 0, 0, 1)` (take `.r`); displaying
one directly via `<texture src>` shows that same red-channel reading;
shader/pipeline targets and readbacks stay RGBA8. The palette-lookup shader
itself stays userland (already proven there by the port).

Distinct from [[gpu-compressed-textures]] (ETC2, further out).
