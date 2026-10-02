---
title: What lit rows cost in a long list
description: Measured 2026-09-27 on the SM-T500 tablet, the Pixel 7 and the Philips TV - a 300-row list of rows dressed like the lit Item (key shadow, contact shadow, sheen gradient, bevel), scrolled at 900 px/s - a transparent shadow caster costs a save layer per shadow and drops every tiled GPU to 8-19 fps; opaque casters cost about 0.1-0.9 ms per blurred shadow per frame, which the tablet and the TV cannot afford twice per row.
created: 2026-09-27
---

# What lit rows cost in a long list

The lit components model ([lit-components](../design/lit-components.md))
gives every raised surface two blurred shadows. Before the model moves into
the package, the question was what that costs where a screen holds dozens
of them: a long list. Measured with probes/lit-list-probe.tsx on a release
client with the cadence hold off (`SOLIDRT_CADENCE_HOLD=off`), 300 rows, each a
repaint boundary as the stock Item is, scrolled down and up at 900 px/s by
a frame loop for 12 s; the figures are the last 10 s (`/stats?window=10000`).
`frames` is the count of presented frames in that window, so 600 is 60 fps.

Row dressing per mode:

- **flat**: a solid fill rect, an avatar oval, two texts.
- **lit**: plus a key shadow (offset 0.6/2 px, blur 5, alpha 0.56) and a
  contact shadow (offset 0.15/0.5, blur 2.2, alpha 0.4), a sheen gradient
  fill and a gradient bevel stroke. The demo's raised (elevation 2) values
  on a dark scheme.
- **glow**: lit plus a 14 px colored glow.
- **transparent / opaque casters**: the shadow-carrying rects are
  transparent (the demo's idiom) or filled with the row color, hidden under
  the sheen.
- **1 shadow**: lit without the contact shadow.

| Device (visible rows) | flat | lit, transparent | lit, opaque, 2 shadows | lit, opaque, 1 shadow | glow, opaque |
|---|---|---|---|---|---|
| SM-T500, Adreno 610, 1200x2000, 60 Hz (28) | 36 fps, GPU 16.0 ms | 12 fps, 31.0 ms | 20 fps, 35.2 ms | 40 fps, 20.3 ms | 20 fps, 37.9 ms |
| Pixel 7, Mali-G710, 1080x2400 (43) | 60 fps, 2.0 ms | 19 fps, 2.4 ms | 33-47 fps, 7.3 ms | 55-60 fps, 4.8 ms | 49-60 fps, 10.2 ms |
| Philips TV, MediaTek, 1920x1080, 50 Hz (19) | 49 fps, 5.9 ms | 8 fps, 27.9 ms | 27 fps, 25.7 ms | 43 fps, 23.2 ms | 26 fps, 38.8 ms |

The matrix was run twice: once without and once with a per-run proof
that the named probe answered on that client (its `state` reports the
probe and the window size) and that the window held frames. The table is
the second run; the first agreed on the tablet and the TV within a few
frames (the tablet's one-shadow row read 28 fps there, 40 here), and the
phone's opaque rows are given as the range over both runs.

GPU is `gpuFrameExecMsPerFrame` (on Android the compositor's queue-to-fence
span, so it includes the swap throttle; read the fps column first). The
phone's Mali clock was sampled once per run (202-510 MHz, not pinned), so
its rows are not same-clock figures: the 47 fps run sat at 251 MHz, the
glow run that held 60 at 202 MHz. Skin temperature stayed at status 0
throughout (the tablet reports only its battery sensor, 26.5 C; the TV has
no thermal service).

## What it says

- **A transparent shadow caster is off the table for anything repeated.**
  Its blurred shadow cannot take an ancestor's opacity in place, so the
  runtime composites it in a save layer of its own (the 2026-09-27 fix for
  the dropped group opacity); one layer per shadow, two per row, is a
  full-tile resolve each on a tiled GPU. Every device fell to 8-19 fps.
  Cast shadows from the opaque fill rect (as `glow` already does), or from
  an opaque rect hidden under the fill; keep transparent casters for the
  few surfaces that are actually see-through (glass overlays).
- **Opaque blurred shadows cost per shadow per frame** about 0.9 ms on
  the TV, 0.3-0.5 ms on the tablet and 0.06 ms on the phone. Two per row
  puts the tablet at 20 fps (36 flat) and the TV at 27 fps (49 flat); one
  per row, 28-40 and 43 fps. The phone holds 55-60 with one shadow and
  sits at 33-47 with two, clock-dependent.
- **The glow adds little once the shadows are opaque** (tablet 20 to 20
  fps, TV 27 to 26), it is one more blur of the same kind.
- **The flat baseline is the tablet's problem, not the model's:** the
  SM-T500 scrolls a plain 28-row list at 36 fps with per-row boundaries
  (359 frames, 237 missed presents), so anything the lit look adds lands
  on a device already under its refresh. The TV and the phone hold theirs.
- Per-row repaint boundaries change little during a full-speed scroll
  (first run: tablet 191 vs 178 frames with and without, phone 467 vs
  516): every frame moves every row, so the cached rasters are
  recomposited rather than saved.

## Consequences for the package move

- Elevation is a per-role default with **Item flat** (or one shadow at
  most), raised for the few surfaces per screen (Card, Button, the
  overlays). A list of raised items is an app's deliberate choice, and
  the numbers above are its price.
- Surface casts shadows from opaque rects; the transparent-caster path is
  the glass overlay's only.
- Lit stays a preset. The flat look remains the stock default on the
  measured targets.
