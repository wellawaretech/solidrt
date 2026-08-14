---
title: Why offscreen rasters are multisampled
description: Offscreen MSAA exists for one case - gradient emoji drawn through the svg path into a snapshot boundary - so that case is the regression test for any sample-count change, not a corner case.
created: 2026-08-13
---

# Why offscreen rasters are multisampled

`MSAA_SAMPLES` (alloy/src/gl.rs) is 4 for every offscreen raster, and the
reason is narrower than "offscreen should match the window". It was added for
gradient emoji drawn through the `<svg>` primitive into a snapshot boundary:
`alloy/src/rendertree/kinds/svg.rs` parses with usvg and emits
`builder.draw_path`, so Impeller fills it stencil-then-cover, and without a
multisampled target the result is hard-edged.

So any future work on sample counts has to keep that case at 4x. It is the
regression test, not a corner case - which is the whole reason the cheaper
default was rejected.

What does not need it: text is atlas-sampled, so its AA is already baked into
the atlas, and axis-aligned rects and textures look identical at one sample.
That is what `repaintBoundary="snapshot-no-aa"` exists for, and the name is
deliberately unattractive so it reads as "something was given up here" to
whoever later finds a panel looking jaggy.

Rejected once, and worth not re-proposing without an audit: making no-AA the
default and MSAA opt-in. It is cheaper for the common panel and silently
regresses the emoji case - hard edges, no error, easy to miss on the
developing machine. Perf costs announce themselves; visual regressions do not.

Source: [snapshot-offscreen-rig-churn](../done/snapshot-offscreen-rig-churn.md).
A one-line doc comment on `MSAA_SAMPLES` would be a better home than this note.
