---
type: backlog-item
title: Cross-platform GPU usage attribution (is the client burning GPU, and on what)
status: deferred
timestamp: 2026-07-19T00:00:00Z
---

# Cross-platform GPU usage attribution

Motivation (crushy, 2026-07-19): the user saw 30-50% system GPU while the
game looked idle, and every in-app number was innocent (jsMs ~0.1, paintMs
~0.4, layout ~0). The agent answered it with an ad-hoc Linux-only probe:
sample /proc/*/fdinfo twice, diff per-client `drm-engine-render` busy-ns,
attribute per process. That attribution (compositor ~half, solidrt-go ~40%
while "idle") is what unraveled the real bugs - a standing onFrame forcing
render+present every vsync, and later that the runner re-presents unchanged
frames (reusedPerSec 60, skippedPerSec 0) even with zero app activity. It
also quantified the knock-on cost: every client present makes the OS
compositor recomposite, so the app's idle burn showed up twice.

The probe worked but is not shippable guidance: it is Linux/DRM-specific
(and even there fdinfo needs dedupe by drm-client-id and overcounts with
multiple GL contexts - only ratios are trustworthy). macOS, Windows, and
Android all need different answers, so this stays out of the scaffolded
AGENTS.md until there is one story.

Open design question - two complementary shapes, pick one or both:

- Self-measurement in the engine (preferred, fully portable): the client
  reports its OWN GPU cost per frame - GPU pass timings via timer queries
  (GLES disjoint timer / Metal counters, whatever impellers can surface),
  presents/sec, and the existing skipped/reused counters - surfaced through
  get_stats. This answers the question agents actually need ("is MY app
  burning GPU while idle, and which pass") without any OS-specific code,
  and would have caught the present-every-vsync burn directly.
- System attribution (who else): per-process GPU busy shares like the
  fdinfo probe, so compositor knock-on and other-process noise are visible.
  Inherently per-OS (Linux fdinfo, macOS IOKit/powermetrics, Windows DXGI
  QueryVideoMemoryInfo/ETW, Android gpuwork/atrace); probably a documented
  per-platform recipe or a small `srt doctor`-style helper rather than an
  engine feature.

Related: production-diagnostics-surface.md (where latched counters get
read), mcp-agent-loop-improvements.md (interaction-performance visibility
wants slow-frame warnings and stats high-water marks - GPU ms per frame
belongs in that same family).
