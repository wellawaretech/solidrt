---
title: Read a depth texture through the control API
description: A target's depth texture is sampler-only, so /texture refuses it and a shadow-map bug cannot be split into "the tile was not written" and "the receiver does not sample it" without instrumenting the runtime; a debug read that renders the depth through a copy pass would answer that in one call.
created: 2026-10-07
---

# Read a depth texture through the control API

## Symptom

`/texture?id=<depth id>` answers "texture N is target M's depth texture:
sampler-only, render it through a pass to read it". On the Android
cold-start shadow loss
([android-cold-start-shadow-loss](../done/android-cold-start-shadow-loss.md)) the
one read that would have split the problem in a minute was the shadow
atlas's depth: either the casters never landed in the tile, or the
receivers fail to sample what is there. Without it the hunt went through
an afternoon of app-side bisects. The atlas's colour attachment reads
fine and says nothing: the depth-only material and the tile clear both
write white.

## Done looks like

`/texture` (and `get_texture`) on a depth id renders the depth through a
copy pass into a readable target and answers it like any texture, as
8-bit grey or as raw floats (`format=raw` with a `depth` channel), with
the range mapped so the near and far planes read apart. Headless and
device alike.

## Involves

The raster side has the copy-pass machinery (`overwrite_with`, the
resolve programs); the read needs a depth-sampling program (a plain
`sampler2D` over a depth texture reads the depth as red on ES 3) and a
scratch target the size of the depth texture. The control API's texture
read then takes the depth-owner branch instead of refusing.
