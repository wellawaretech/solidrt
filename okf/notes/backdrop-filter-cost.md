---
title: What a frosted panel costs on the small targets
description: Measured 2026-09-27 on the Pixel 7, the SM-T500 tablet and the Philips TV - blur-28 backdrop panels of 320x140 logical px over a bar that moves every frame - one panel holds the refresh everywhere (TV 3 to 13 ms GPU per frame), four panels drop the tablet and the TV to 22 fps with hundreds of missed presents.
created: 2026-09-27
---

# What a frosted panel costs on the small targets

`backdropFilter` captures and filters the pixels beneath a panel once per
frame it is painted in; on a tiled GPU that mid-frame read is the
expensive operation (okf/done/backdrop-filter-android-cost.md). Measured
with probes/glass-cost-probe.tsx: N panels of 320x140 logical px, `blur:
28, saturate: 1.4` (the lit prototype's glass), a tinted fill and a
hairline, over five stripes and a 40 px bar swept across the window by a
frame loop at 400 px/s for 12 s; release client, cadence hold off, the
last 10 s read from `/stats?window=10000`. `frames` is the presented count
in that window (600 = 60 fps; the TV presents at 50).

| Device | 0 panels | 1 panel | 4 panels |
|---|---|---|---|
| Pixel 7, Mali-G710, 1080x2400 | 60 fps, GPU 3.3 ms | 60 fps, 7.6 ms | 60 fps, 6.3 ms |
| SM-T500, Adreno 610, 1200x2000, 60 Hz | 60 fps, 13.1 ms | 60 fps, 14.0 ms | 22 fps, 35.2 ms, 379 missed presents |
| Philips TV, MediaTek, 1920x1080, 50 Hz | 50 fps, 3.3 ms | 50 fps, 13.3 ms | 22 fps, 280 missed presents |

GPU is `gpuFrameExecMsPerFrame`, on Android the compositor's queue-to-fence
span (the tablet's 13 ms baseline is mostly its swap throttle). The
phone's Mali clock was not pinned, so its rows are ceilings at whatever
clock the governor chose; skin temperature stayed at thermal status 0.

## What it says

- **One frosted panel is affordable on every measured device.** The TV
  pays about 10 ms of GPU for it and still presents every frame.
- **Four are not** on the tablet or the TV: 22 fps, with a missed present
  for most frames. The cost is not linear in panels there (tablet 1 to 14
  ms, 4 to 35 ms), consistent with each capture forcing a tile resolve.
- So glass is a deliberate overlay (a dialog, a sheet, one floating panel),
  never a surface material for cards or rows. The lit components model
  keeps it as the overlay role's option
  ([lit-components](../design/lit-components.md)).
