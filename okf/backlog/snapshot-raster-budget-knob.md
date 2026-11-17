---
title: An app's knob for the snapshot raster budget
description: The runtime defers snapshot re-rasters made for density past a time budget of 4 ms per frame, which lattice sets and an app cannot change; a budget decides when a tile turns sharp, so by the API design rule an app needs a way to set it or turn it off, and its shape is not decided.
created: 2026-11-17
---

# An app's knob for the snapshot raster budget

## Problem

Snapshots under one ancestor scale pass the motion bound, or come to
rest, in the same frame. The runtime spreads their re-rasters over
frames: each paint walk re-rasters at least one, and as many as fit a
time budget, the most stretched first
([boundary-cache-under-js-written-scale](../done/boundary-cache-under-js-written-scale.md),
"Part 3 as decided").

Alloy holds the mechanism only
(`PlatformContext::set_snapshot_raster_budget`, unset by default).
Lattice sets the default, `SNAPSHOT_RASTER_BUDGET` of 4 ms, in
`apply_paint_policy`. The observable is there: `snapshotsDeferred` in
the paint stats, the overlay and the control API's stats.

What is missing is the app's knob. A budget changes when a tile turns
sharp after a zoom, and it trades a steady frame rate against one long
frame. That is behaviour an app reasons about. The API design rule says
a runtime behaviour never ships as a hidden policy alone.

Measured on a desktop release client, 40 tiles with 200 text labels
each, under a scale written every frame: without a budget 40 fps with
frames of up to 367 ms, with 4 ms a steady 31.6 fps with no frame over
50 ms. An app that prefers the first cannot ask for it today.

## Proposed

A prop on `<window>`, in milliseconds: absent is the runtime's default,
`null` is no budget (every snapshot re-rasters in the frame it is due),
`0` is one per frame. The budget is per paint walk of a window, so the
window is where it belongs. The flux arm forwards to
`set_snapshot_raster_budget`; lattice's default applies when the prop is
absent.

Not decided: the name, and whether the knob is a duration at all or a
choice between two behaviours (spread, or all at once).

## Done looks like

- An app can set the budget and turn it off, and the typings say what
  each value does.
- `packages/core/agents/performance.md` rule 5 names the knob where it
  describes the budget.
- A flux property test for the prop, and a lattice test that the default
  applies when the prop is absent.

## Involves

The window props in `flux/src/alloy_plugins/properties/`, `apply_paint_policy`
in `lattice/src/lib.rs`, the core typings.
