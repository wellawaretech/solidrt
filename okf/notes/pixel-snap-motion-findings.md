---
title: What reading pixel snap under motion showed
description: Findings from building pixel-snap-at-draw - the settling frame is bounded by a rounding plus the motion's last step, a read of the picture restarts a text's rest count, the text layer count in the stats reads a frame late, a text must hint when its scale is steady and not wait for rest, and a node under a boundary can rest one device pixel from where it rests bare.
created: 2026-11-10
---

# What reading pixel snap under motion showed

From okf/done/pixel-snap-at-draw.md. The reads are
`packages/core/tests/pixel-snap-motion.test.tsx` and
`alloy/examples/pixel_snap.rs`.

- The frame a track settles on moves the ink by one rounding on top of the
  motion's own last step, so a read of it is bounded by half a device pixel
  plus the step of the frame before, not by half a pixel alone.
- A read of the picture in a frame (`app.painted`, a capture) is a second
  look at every text under it in that frame, and a text counts consecutive
  looks to know its scale rests. A test that counts rasters at rest must
  not read the picture in the same run.
- `paintOps.textLayers` of `/stats` reads one frame late.
- Rest is proven by consecutive looks, and an app at rest makes none. A
  text used to hint only at rest, at the display scale or at a
  transition's end, so a static text under a design-size fit made its
  first raster unhinted and changed when something else first animated;
  the same after a content change, and after one direct scale write past
  the motion bound. Fixed: a text also hints when its scale is steady (the
  scale of its last look, no transition on the chain), and an unhinted
  raster on a display that hints asks for the looks that prove rest. On a
  display that does not hint the flag is never set, so nothing is re-made
  for it. The read is `packages/core/tests/text-hint-steady.test.tsx`.
- A node inside a boundary or a capture can rest one device pixel from
  where the walk lands it without the boundary (text under a snapshot
  against the same text bare: 1 px apart at display scale 2). Accepted
  above; measured in `packages/core/tests/pixel-snap-motion.test.tsx`.
