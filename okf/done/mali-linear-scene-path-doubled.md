---
title: The linear-blend scene path costs twice the display path on the Pixel 7 where it used to cost the same
description: On 2026-09-27 the 1M splat train at full resolution read 42 ms in a linear scene and 41 in a display scene on the Pixel 7; on 2026-10-18 HEAD reads 88 and 44 at the same clock and pose, so something since then doubled the half-float scene buffer path (the buffer, its resolve, or what the view does with it) on Mali, and every linear 3d scene on a phone pays it.
created: 2026-10-18
completed: 2026-10-18
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

## Findings (2026-10-18)

- The cause is the quality row. Since 01463ec1 (2026-10-17, "3d quality
  settings") a scene that sets no `samples` takes its row's count, and
  the medium row an app that says nothing gets carries 4. The
  2026-09-27 rows were single-sample (`samples: opts?.samples`, unset =
  1); the 2026-10-18 rows ran the same probe on a 4x rgba16f target
  (the inventory reports `samples: 4` for the splat scene on the Pixel).
  Nothing else on the linear path moved in the window: resolve.ts, the
  GL target and storage code and the probe are otherwise unchanged.
- Confirmed on the Pixel 7 (Mali-G710, HEAD's client, cold: status 0,
  skin 25-27 C, every clock sample 848 MHz, cadence hold off, the 1M
  train, 1079x2399, orbit 0.3 rad/s, 6 s settle plus a 10 s window)
  with a `samples` arg added to the probe's bench rung (null = the
  row's):

  | rung, 1M full-res stock | ms/frame |
  |---|---|
  | linear, row default (4x) | 88.2 |
  | linear, 2x | 87.5 |
  | linear, 1x | 33.7 |
  | display, row default (4x) | 44.4 |
  | display, 1x | 31.7 |

- The shape: on the half-float buffer multisampling is a cliff, not a
  per-sample cost. 2x costs the same +54 ms as 4x (+160%), while on the
  rgba8 buffer 4x costs +12.7 ms (+40%). The term is fixed and
  resolution-bound (the 300k row paid the same ~40 ms), which fits the
  tile budget: 64 bits of color per sample plus depth exceeds what the
  Mali tile buffer holds per pixel at its default tile size for any
  count above 1, so the driver renders smaller tiles or leaves the
  on-tile path; inferred from the cliff, not read from the driver. The
  in-tile flavor (EXT_multisampled_render_to_texture) is the one in use.
- At one sample linear reads 33.7 against display's 31.7, within 2 ms
  as the 2026-09-27 A/B found (both below the record because the splat
  draw got cheaper with the record compression): the blend space is not
  the phone's lever, the sample count is.
- Beyond splats: every linear scene on a Mali phone that says nothing
  about quality pays ~54 ms per 2.6 Mpx frame for the medium row's 4x,
  more than the whole 1M splat draw. The row's "MSAA 4x costs ~9 ms at
  2 Mpx" was the Adreno 610 figure; the Mali figure on the linear
  buffer is six times that and does not shrink at 2x.

## Outcome

No phone bisect was needed: the code bisect named the quality row and
the phone confirmed it in five rungs. Decided 2026-10-18: the medium
row's `samples` is 1 (high keeps 4), since 2x buys nothing on this GPU
and the low row would cut far more than the one knob that hurts. The
splat probe's bench rung took a `samples` arg (null = the row's) for
the A/B.
