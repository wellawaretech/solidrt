---
type: backlog-item
title: Runtime policies - tracked, app-readable, app-overridable
description: The runtime is accumulating behavior policies it selects on the app's behalf from device facts (frame pacing being the first with real consequences). That is implicit magic unless the policies are enumerable, their chosen values readable, and the choice overridable by the app. Track them here until a policy surface exists.
status: open
timestamp: 2026-08-13T00:00:00Z
---

# Runtime policies - tracked, app-readable, app-overridable

Trigger (2026-08-13): the frame-pacing fix ([[frame-pacing-fluency]])
made lattice pick `FramePacing` from the touch fact - SwapPaced on
touchless devices, VsyncLocked on touch. The right default by
measurement, but the app cannot see which mode it got or say "I am a
drawing app on a TV, give me the low-latency mode anyway". That is the
implicit magic the API philosophy forbids; the accepted escape is to make
policies first-class rather than to stop having defaults.

Principle: a runtime policy is any point where the runtime picks behavior
on the app's behalf from device facts or heuristics. Every such policy
should be
- **named and tracked** (this note, until a real surface exists),
- **readable** by the app (which value is in effect, and why - the fact
  it was derived from),
- **overridable** where an override is coherent (the app knows its own
  nature better than any device heuristic),
- fact-based, never target-special-cased (no "if TV then ..."; the fact
  is "no touch input", not the marketing category).

## Current policies (2026-08-13)

- **Frame pacing** (lattice, from the touch fact): SwapPaced (fluency,
  ~1-2 frames extra input latency) vs VsyncLocked (input-to-glass
  latency). The first policy with a measured, user-visible consequence in
  both directions - the natural first candidate for an override surface.
- **Dev/prod validation** ([[dev-prod-validation-policy]]): throw in dev,
  warn in prod; the signal itself is still missing.
- **Superseded-frame load shedding**: interactive mode drops stale frame
  signals under GPU-bound presents; capture/playback draws every frame.
  Mode-selected today, not app-selectable.
- **Idle tick gating**: per-frame JS keeps running at most one refresh
  period behind when no frames are produced; suppressed under raster
  backlog. Pure engine self-protection - probably never app-facing.
- **Pointer-move resampling**: one position per pointer per frame slot.
  Producer-side rule; app-facing only in that apps must not expect every
  hardware move.
- **Typography / font fallbacks and text-scale policy**: taste defaults
  above alloy ([[../plans/website|design-system notes]] territory);
  OS text-scale handling still pending.

## Surface sketch (deliberately unresolved)

Through the solidrt lens the read side belongs next to the other
environment facts (the `Flux.capabilities`-style "ask by name" shape
wears well); the write side is app manifest or top-level API, not
per-frame. One decision to make deliberately: whether overrides are
declarative (manifest/window prop, applies before first frame) or
imperative (switchable at runtime - frame pacing genuinely could be:
a drawing surface could request VsyncLocked only while a stylus is
down). Do not design this in passing; it wants the backlog-rework
session's attention.
