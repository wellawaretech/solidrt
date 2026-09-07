---
title: A throwing onFrame callback needs a dev-mode banner, not just a log line
description: A throw inside an onFrame callback is caught, logged and repeated every frame while the app keeps presenting partial frames, so on screen it reads as a logic bug (entities drawn after the throwing line vanish) and the only trace is a log line the reader has to think to look for.
created: 2026-09-07
---

# A throwing onFrame callback needs a dev-mode banner

## Symptom

An `onFrame` callback throws part way through its draw (a destructure of
a missing array entry after a pool constant was raised). The runtime
catches it, logs `Error in onFrame callback: TypeError ... at draw
(index.tsx:NNN)`, keeps the subscription, and presents the frame with
everything written before the throw. Everything after it - every entity
the callback would have positioned - silently stops appearing, while the
backdrop and the HUD drawn earlier keep rendering perfectly. Frame after
frame: `get_logs` showed the line with a repeat count in the thousands.

On screen that is indistinguishable from a game-logic bug, and the
reporter went looking in the simulation first. The log had the answer
immediately and exactly; the cost was not finding it, it was knowing to
look.

Core's AGENTS.md now documents the containment (landed 2026-09-07: the
callback is abandoned mid-way, a partial frame is a live possibility,
check the logs before reasoning about a rendering symptom). The doc
shortens the hunt for a reader who has read it; the banner removes it.

## Done looks like

In a dev-connected client, a callback that has thrown on N consecutive
frames shows it on the overlay: one line, `onFrame threw 1200 frames
running: TypeError ... (index.tsx:NNN)`, cleared the first frame the
callback completes. Same idea as the browser's red error badge - the
error is not modal, the app keeps running, but it cannot be missed. A
production build shows nothing, as now.

## What it involves

- `packages/core/src/window.ts` `onFrame`: count consecutive throws per
  subscription; on the first and then at a coarse cadence (or only on
  transitions: started throwing, stopped throwing) report `{ frames,
  message, site }` to the runtime through the existing dev channel that
  carries stats/log state - not a console line per frame, which is
  already there and is the noise this replaces.
- The client badge line (okf/backlog/dev-overlays.md's overlay) or the
  stats HUD gains an error row while a report is live. Nothing on the
  raster side beyond another overlay line.
- Reset on reload (a fresh engine has no throw history).

Open: whether the same row should cover `Contained error` storms (an
element prop throwing every frame has the same shape: contained,
logged, repeated, invisible on screen). Probably yes - "something is
throwing every frame" is the fact the banner states, and the two
sources are one counter.
