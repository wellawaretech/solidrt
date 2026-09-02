---
title: Measure backdropFilter cost on Android
description: A backdropFilter forces an offscreen capture-and-filter of the pixels beneath per panel; desktop holds 60 fps with four panels over live content, but tiler GPUs pay differently for mid-frame target reads - measure before treating the prop as casual on TV/phone.
created: 2026-09-02
---

# Measure backdropFilter cost on Android

`backdropFilter` (shipped in okf/done/impeller-effects.md) emits one
save_layer per panel whose backdrop argument captures and filters the
pixels already painted beneath. That is a mid-frame read of the current
target - the operation tile-based GPUs (every Android target, MediaTek TVs
especially) handle worst: it can force a tile flush/resolve at each panel.

Desktop Linux measurement (2026-09-02, release client,
probes/effects-probe.tsx): four glass panels re-filtering over a 20 Hz
animation behind them hold frameMs ~16.5 with missedPresents 0.

To do: run the same probe's glass column on an Android device (and ideally
a TV), read /stats frameMs + missedPresents + gpuFrameExecMs with the
`mover` debug command animating, at one panel and at four. If a single
frosted panel costs a large fraction of the frame budget there, the prop's
docs should say so explicitly (they already call it "a deliberate panel,
not a casual style").
