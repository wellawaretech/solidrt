---
title: The linear-blend scene path costs twice the display path on the Pixel 7 where it used to cost the same
description: On 2026-09-27 the 1M splat train at full resolution read 42 ms in a linear scene and 41 in a display scene on the Pixel 7; on 2026-10-18 HEAD reads 88 and 44 at the same clock and pose, so something since then doubled the half-float scene buffer path (the buffer, its resolve, or what the view does with it) on Mali, and every linear 3d scene on a phone pays it.
created: 2026-10-18
---

# The linear-blend scene path costs twice the display path on the Pixel 7 where it used to cost the same

## Symptom

Found while measuring okf/plans/splat-record-compression.md (its
Findings hold the rows): probes/splat-mesh-probe.tsx, the 1M train,
portrait 1079x2399, orbit at 0.3 rad/s, cold phone, cadence hold off,
every clock sample 848 MHz, HEAD's client (9101fef9):

| blend | 2026-09-27 (gaussian-splats notes) | 2026-10-18 HEAD |
|---|---|---|
| linear | 41.96 | 88.1 |
| display | 40.94 | 43.7 |

The display row is unchanged, so the splat draw is unchanged; the
linear row doubled. 300k reads 63 ms linear where it read 23.5. The
vertex side (noraster) is unchanged too, so the extra 44 ms is in what
the linear mode adds over display: the half-float scene buffer, the
resolve pass, or how the scene texture reaches the window.

## Shape

Bisect on the phone between 2026-09-27 and HEAD with the same probe
and protocol (okf/notes/gaussian-splats.md has the traps); the
candidates by date are the fill mode and render scale work
(2026-10-13), the target MSAA and sub-target work, and the pixel-snapped
paint boxes. The display-blend row is the control.
