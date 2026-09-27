---
title: Animate shadow as one value
description: `shadow` joins the native transitions - offset, blur, spread and color move together on one track, an unset shadow counts as none, so elevation changes animate without cross-faded shadow layers.
created: 2026-09-27
completed: 2026-09-27
---

# Animate shadow as one value

`shadow` was not in the animatable set, so an elevation change (a press
sinking, a card lifting on hover, a dialog rising) snapped, and the lit
components prototype cross-faded whole shadow-carrying rects instead: two
casters per surface and a second opacity group per state.

Built: `AnimProp::Shadow` in alloy/src/rendertree/transitions.rs, an
eight-lane track (dx, dy, blur, spread, then the color's oklab L/a/b and
alpha, so `Lanes` widened from four to eight for every kind), read and
written on rect, oval and path. CSS box-shadow rules: an unset shadow reads
as `ShadowState::NONE` (zero, transparent), so a shadow fades in from
nothing; a write of `null` animates out and a track settling on NONE clears
the element's shadow rather than leaving an invisible one. `from` and `exit`
take a shadow object or `null`. A path's spread rule holds through the
animated path: flux decodes the target as the property path would (spread
refused on a path, so the write falls to that path's error), and a rect
endpoint's spread reaching a path through a shared declaration is dropped.
Tests in alloy/src/tests/transitions.rs; the JSX name in flux
transition.rs; the core types and docs on `ShadowProps`,
`TransitionPropName` and the endpoints.

Not done here: the lit prototype still cross-fades (it runs against the
installed client); the switch to one shadow rect per surface is part of
[lit-components-package](../done/lit-components-package.md).
