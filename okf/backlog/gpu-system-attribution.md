---
title: Whole-system GPU attribution, per platform
description: Answering "who else is burning the GPU" needs a different mechanism on every OS, so it wants a documented per-platform recipe or an srt doctor helper rather than an engine feature.
created: 2026-08-13
---

# Whole-system GPU attribution, per platform

In-app counters can only ever answer "is MY app burning GPU". They cannot see
the compositor's knock-on cost, another process pinning the GPU, or a driver
misbehaving - and those were exactly the terms in which a real idle-burn
investigation had to be argued.

Each platform needs its own mechanism, which is why this never became one
feature: Linux `/proc` fdinfo, macOS IOKit or powermetrics, Windows DXGI
`QueryVideoMemoryInfo` / ETW, Android gpuwork or atrace.

Shape to decide: a documented per-platform recipe, or a small `srt doctor`-style
helper that runs the right one. Probably not an engine feature either way -
nothing here belongs in the runtime.

The Linux half is already written up and usable today, including the caveats
that make its absolute numbers untrustworthy:
[measuring which process burns the GPU](../notes/gpu-burn-attribution-linux.md).
The engine-side counterpart is
[gpu-timer-query-pass-timing](../done/gpu-timer-query-pass-timing.md).
