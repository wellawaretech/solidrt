---
title: Transition writes before the first frame anchor at clock 0
description: The animation clock is stamped once per frame, so a transition target written at module-eval time (before any frame ran) starts its track at clock 0 and the first stamped advance fast-forwards the whole startup latency - a 300ms spring can be most of the way done when the first frame paints. Element and node transitions share the artifact; spatial is more exposed because writing initial targets during scene setup is a natural pattern. An install-time (or first-JS-entry) clock stamp is the likely few-line fix.
created: 2026-08-24
---

# Transition writes before the first frame anchor at clock 0

## Symptom

`set_transition_now` is stamped in the runtime's frame path, so the first
stamp happens with the first frame. A transition target written earlier -
module evaluation, scene setup, anything before the app's first paint -
starts its track with `now_ms` still at the default 0. The first advance
then reads the real app-time stamp and integrates the whole gap at once:
in the spatial-node-transitions stage-2 probe
([done](../done/spatial-node-transitions.md)), a 300 ms spring written at
mount was visibly most of the way to its target at an 80 ms wall-clock
sample, and a 200 ms tween effectively skipped its first half.

Element transitions carry the same artifact (the stamp site and semantics
are shared), but rarely show it: element targets mostly come from event
handlers and effects, which run inside stamped ticks and are at most one
frame period stale. Spatial node transitions invite mount-time writes -
"create the scene, declare springs, write where things should be" - which
is exactly the exposed window.

## What done looks like

A target written before the first frame animates its full duration from
the first painted frame, for elements and nodes alike. The likely fix is
stamping both clocks once at install (or on the first JS entry) with the
app timeline's current time, so clock 0 never leaks into a track;
record/playback determinism must survive (the stamp must come from the
paced timeline, not wall time). A test: write a target at mount, advance
the first frame at a late stamp, assert the track is at its beginning,
not fast-forwarded.
