---
title: Recognizer deltas are window pixels, so a scaled input element pans at the wrong rate
description: createPan and createTransform measure dx/dy in clientX/clientY while every consumer applies them in the element's local frame; under a designSize fit (or any scaled ancestor) a drag moves the content by the wrong amount, for ScrollView and the 2d camera alike, even though each event already carries exact localX/localY.
created: 2026-09-06
---

# Recognizer deltas are window pixels, so a scaled input element pans at the wrong rate

What it looks like when you hit it: a `ScrollView`, or a sprite layer
driven by `createCamera2d`, sits inside a `<view designSize>` that fits a
320x180 design into a 1280x720 window. A drag of 100 window pixels should
move the content 25 design pixels; it moves 100. The same happens under
any scaled ancestor (a `scale` transform, a zoomed inset).

Cause: `createPan` and `createTransform` (packages/core/src/pan.ts,
transform.ts) track positions in `clientX/clientY` and stream deltas in
that frame. Every consumer applies the delta in its own local frame
(ScrollView's `scrollBy`, the 2d camera's `panBy`, the orbit camera's
viewport-relative rotation) and so silently assumes the two frames have
the same scale. The events already carry `localX/localY` - exact even off
the node, per the PointerEvent contract - so the information is there;
the recognizers just do not use it.

## Shape

Measure in the node's local frame: track `localX/localY` instead of
`clientX/clientY` for the delta stream (and the transform's focal point),
keeping the slop test in window pixels (slop is a finger-travel threshold
and should not shrink under a zoomed-out design). `TransformDelta.x/y`
then report the focal point in local pixels too, which removes the 2d
camera's separate local-centroid bookkeeping. Rotation and scale are
frame-independent already.

## Done (2026-09-06)

Both recognizers (packages/core/src/pan.ts, transform.ts) now measure
deltas in the handler node's PARENT frame (`parentX/parentY`, the frame
the node's own x/y live in, so the drag idiom and the content idiom both
apply the delta unchanged), the transform reports its focal point in the
node's LOCAL frame (`localX/localY`, the zoom-about anchor), and the slop,
span filter and rate gates stay in window pixels because they are
finger-travel thresholds. Scale and rotation are ratios and angles, the
same in every uniformly scaled frame. Documented on both recognizers and
in the core AGENTS.md gesture bullet.

Consumers: ScrollView unchanged in code (its deltas are now right under a
fit); the orbit camera's wheel anchors on `localX/localY` and its
`viewport().height` / `zoomAnchor` docs say element-local, which is what
the OrbitCamera component already passed (it had been dividing
window-frame deltas by a local-frame height); the first-person camera
likewise; the 2d camera dropped its own local-centroid bookkeeping and
reads the focal from the delta.

Verified live with a designSize probe (320x180 fitted into 1280x720,
scale 4): a drag of 80 window px past the slop scrolled a ScrollView by
exactly 20 design px and panned a `createCamera2d` view by exactly 20
local px, and a wheel notch at window (800,200) zoomed about local
(40,50) to the hand-computed pose. Before the change both drags would
have moved 80.

The orbit question resolved itself: viewport-relative rotation divides by
the element's local height, the value the component passed all along, so
nothing switched.
