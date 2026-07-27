---
type: backlog-item
title: fontStretch / width axis
description: The bundled Noto variables carry a wdth axis the text API cannot reach; whether to expose a CSS-style font-stretch, pending an Impeller ParagraphStyle capability check.
status: open
timestamp: 2026-07-20T00:00:00Z
---

# fontStretch / width axis

Source: packaged-fonts size discussion 2026-07-20. The text API exposes
fontWeight (100-900, drives the variable wght axis, verified working by
alloy/examples/weight_axis.rs) and fontStyle, but no width/stretch
control - even though the bundled Noto variable fonts carry a full wdth
axis. CSS has font-stretch (ultra-condensed..ultra-expanded, or a
percentage mapping to wdth); Impeller/Skia paragraph styles would need
checking for what they expose (FontWeight and FontStyle exist in the
impellers crate surface; width/stretch support unverified).

Open questions when picked up:
- Does Impeller's ParagraphStyle expose width/stretch at all? If not,
  the axis is unreachable regardless of API design and this waits on
  upstream.
- API shape through the solidrt lens: CSS keyword set, bare percentage,
  or nothing until a real app wants condensed text.
- Weight interaction: condensed weights are where variable fonts shine
  (headline typography); worth a components/typography-system angle.

Tension to resolve with okf/plans/packaged-fonts.md: the size finding
that motivated this note is that the wdth axis is 30-40% of each Noto
file (gvar deltas) while being unreachable from the API - instancing
wdth out of the shipped defaults (2.0 -> 1.4 MB per font, zero runtime
cost) is the cheapest size win available. Exposing fontStretch and
keeping the instancing win are mutually exclusive FOR THE DEFAULT FONTS
(custom-packed fonts keep whatever axes they ship with). Decide the
default-font stance whichever lands second.

Status: deferred, discussion not started.
