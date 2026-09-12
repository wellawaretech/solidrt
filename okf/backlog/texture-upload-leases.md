---
title: Zero-copy texture upload leases
description: The remaining half of the staging work - the raster thread now uploads through mapped pixel-unpack buffers, but the pixels are still memcpy'd into them on that thread (~5.6 ms per 1080p frame on the TV). Leasing the mapped memory out to the producer, so a decoder or a guest writes straight into it, is the only honestly zero-copy shape. Its urgent consumer left on 2026-09-12 when fullscreen video moved to punch-through; camera and the wasm/JS upload API remain.
created: 2026-09-12
---

# Zero-copy texture upload leases

Split out of [[texture-upload-staging]], whose stages 1+2 landed in August
and whose stage 3 was deferred until measured necessary. It was measured
necessary on 2026-09-11 and the GL half of it is now done; this item is the
rest.

## What landed (2026-09-11)

`alloy/src/gl/staging.rs`: a ring of three pixel-unpack buffers that
per-frame uploads above 256 KiB stage through - map, memcpy, unmap, then a
`glTexSubImage2D` whose source is GPU memory.

Measured on the Philips TV, per 1 MB plane:

| | direct | staged |
|---|---|---|
| total | 13.4 ms | 4.3 ms |
| of which the GL upload | 13.4 ms | 0.5 ms |
| of which our memcpy | - | 2.8 ms |
| of which map + unmap | - | 1.0 ms |

Per 1080p NV12 frame (two planes) that is 27.5 ms of raster thread down to
10.4 ms. The driver's copy out of client memory was the entire cost; it
moves at ~78 MB/s there, while a memcpy into mapped storage moves at ~530
MB/s and the DMA upload is effectively free.

Map flags make no difference on this driver: invalidate-buffer,
invalidate-range and plain reuse all measured within noise of each other
(map ~0.3 ms, copy identical), so the ring keeps the flag that states the
intent.

## Scope changed 2026-09-12

The case that made this measured-necessary was fullscreen 1080p video on
the TV, and that case left: fullscreen playback goes to
[[android-video-punch-through]], which uploads nothing. What remains is
camera frames (`alloy/src/camera.rs` uploads through the same path), the
JS `updateTexture` API, and the original wasm/JS motivation below - all
real, none of them yet measured against a budget the way video was. Worth
re-checking who the consumer is before building this, rather than
inheriting video's urgency. The numbers above stay valid; they are facts
about the upload path, not about video.

## What is left

The ~2.8 ms per plane we still spend copying is on the raster thread, and
it is a copy of bytes the producer had just finished writing somewhere
else. The original design: leases flowing caller -> player -> decode
worker, so the decoder's stride-repack writes directly into GPU-visible
staging and the raster side becomes unmap + upload.

- alloy: a `Send` lease over the mapped range, with map/unmap staying on
  srt-raster (the GL contract), plus the command shape that returns a
  lease and the one that consumes it.
- forge: `VideoDecoder` gains decode-into-borrowed-buffer, still
  engine-free - a plain `&mut [u8]` destination instead of a returned
  `Vec`.
- flux: request a lease, hand it to the player, hand it back filled.

Worth ~5.6 ms per 1080p frame on the TV, on the thread that is the
bottleneck. Note this alone does not make 1080p fit the TV's 20 ms budget:
the window draw is 11-13.5 ms of the same budget and that is
[[live-texture-content-damage]], which is the larger item.

Stage 4 of the original note also rides this machinery: the JS
`beginTextureUpload` / `endTextureUpload` pair for a guest (wasm or JS)
rendering straight into memory the GPU path reads.

## Done looks like

A 1080p video frame costs the raster thread the unmap and the upload only,
with no per-frame copy on it, and `cmd_micros` at 1080p is within a few ms
of the 360p figure.

Related: [[texture-upload-staging]], [[video-playback]],
[[live-texture-content-damage]], [[flux-wasm-memory-access]].
