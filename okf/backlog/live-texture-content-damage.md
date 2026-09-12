---
title: Content damage for live-sampled textures
description: A texture whose pixels change behind an unchanged id produces no damage, because nodes displaying it hold a live reference and need none to show the new pixels. Correct for the tree, wrong for the repaint - every video, camera or GPU-content frame repaints the whole window, measured at 11-13.5 ms of a 20 ms budget on the TV whatever the content's size.
created: 2026-09-12
---

# Content damage for live-sampled textures

Found while measuring video fluency on the Philips TV
([[video-playback]]), where it is the single largest item in the frame.

## What happens

`Context::note_content` records that a texture's pixels changed and walks
the sampler graph so every flush-rendered target downstream re-renders.
What it does NOT produce is damage: the frame build applies content changes
"as damage on the snapshot boundaries that baked those pixels ... everything
else keeps live texture references and needs no damage for a content
change" (alloy/src/context/content.rs).

That is true as far as showing the pixels goes - a `<texture>` node samples
the live texture, so re-executing the display list picks up the new content
with no tree change at all. But the damage tracker is how the repaint is
scoped, and with no damage rect for the region that changed, the frame
falls back to repainting the whole window.

Measured on the TV (1920x1080 panel, a 640x360 video node, everything else
static): `damagePx` 0 and `partialPresents` 0 for the entire run, and the
window draw costs 11-13.5 ms of raster thread per frame against a 20 ms
refresh period. The cost does not change with the video's resolution
(360p and 1080p both display at 640x360 and both pay it), which is what
identifies it as the window composite rather than the content.

The video node covers 12% of that window.

Fullscreen video leaves this path on 2026-09-12
([[android-video-punch-through]]), which removes the case that found the
problem but none of the problem: camera and every non-fullscreen video or
GPU-content app still repaint the whole window per frame.

## Why it matters beyond video

Every producer of GPU content behind a stable texture id has the same
shape: camera frames, shader targets driven by params, pipeline output,
data textures a game writes per frame. Each one repaints the whole window
per frame no matter how small its node is. Partial repaint
([[partial-repaint]]) is implemented and its buffer-age prerequisite is
available on the device ("[alloy] partial repaint: EGL buffer age
available") - it simply never engages for these apps, because nothing tells
it what changed.

## Shape of the fix

A content change knows its texture id; the rendertree knows where that
texture is painted. The missing link is an index from texture id to the
painted boxes of the nodes displaying it, so `note_content` can damage
those boxes the way any other node change damages its own.

Open questions, none of them settled:

- Where the index lives and what maintains it as nodes move, resize, or
  are removed. The painted box is already computed per node.
- Transforms: a rotated or scaled node's footprint is its quad, not its
  box (the tree already carries `quad` for exactly this).
- Whether the in-tile MSAA window path can honour a damage patch at all on
  this driver, or whether partial repaint and MSAA are exclusive there.
  The window backbuffer is 4x multisampled on the TV.
- Interaction with the window shader path, which reads the finished frame.

## Done looks like

A video or camera app on the TV shows `partialPresents` climbing and a
window draw whose cost tracks the changed region, not the window. The same
measurement that found this (raster phase trace at SRT_LOG=debug, plus
`damagePx`/`partialPresents` in get_stats) is the check.

Related: [[video-playback]], [[partial-repaint]], [[content-damage-perf]],
[[frame-driver-pacing-contract]].
