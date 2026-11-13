---
title: Haptics at present - play with the frame that shows the change
description: A haptic call plays when it is made, which is before the frame that shows its cause is on the display; a haptic that lands a frame early or late against the visual reads as wrong. An `at` option that holds the call until that frame is swapped. Whether it is worth building depends on a measurement - the latency from the call to the actuator moving - which comes first.
created: 2026-11-13
---

# Haptics at present - play with the frame that shows the change

## Symptom

`feedback` and `play` ([haptics](haptics.md),
[haptic-waveforms](haptic-waveforms.md)) act when they are called. The
app calls them in the handler that also changes the scene, and that
change reaches the display some refreshes later. Platforms treat the
alignment as worth an API: macOS has a performance time of
`drawCompleted` on its feedback performer, and iOS has `prepare()` on
its generators to take the latency out of the moment that matters.

Needs [haptics](haptics.md) first.

## Measure first

Whether this is worth building is a number, not an opinion. On the
phone, from the call to the actuator moving there is the hop to the UI
thread (`runOnUiThread`, measured in [haptics](haptics.md)) and the
actuator's own start. From the call to the change being on the display
there is the rest of the frame and the pipeline's depth.

- If the haptic already lands within a refresh of the visual, there is
  nothing to fix: close this item with the numbers in a note.
- If the haptic is early, holding it until the frame is swapped fixes
  it. That is the shape below.
- If the haptic is late (the actuator's latency is longer than the
  pipeline's), holding it makes it worse. Then the fix is the other
  direction, and it is `prepare()` on iOS and nothing on Android.

The measurement needs the actuator's motion against the display: a
high-speed camera on the phone, or a microphone for the actuator
against a photodiode. Record the method with the numbers.

## Shape, if the measurement asks for it

- The option: `at: "now"` (default) or `"present"`, on `feedback` and
  `play`. Additive: a call without it does what it did.
- The simple form needs no presentation fact: the call is held, and
  plays when the frame that carries the handler's changes is swapped.
- Scheduling against the display's grid, if the simple form is not
  close enough: on Android under VsyncLocked the frame signal is
  released on the Choreographer vsync instant, so the grid is exact
  today (`okf/design/frame-timing.md` D8). The number of refreshes
  between that signal and the frame on the display is a named constant
  until the display present time of `EGL_ANDROID_get_frame_timestamps`
  measures it (okf/backlog/presentation-feedback.md, first in its
  order). Presentation feedback itself arrives after the frame is shown,
  so it calibrates this and can never trigger it.
- Ownership: choosing when to play is a policy over alloy's frame facts,
  so it is lattice's. The facility is ore's and reaches JS through flux
  `ore_plugins/`, never through lattice. To settle before building: how
  a held call gets from the flux plugin to the point where lattice sees
  the swap. A plain value both sides meet on (a queue of held calls ore
  owns, which lattice drains at the swap) fits the layering; a trait
  lattice implements for ore does not.
- A held call whose frame never comes (the app is at rest, nothing is
  drawn) plays after a named timeout and does not wait for ever.
- Dev recording: the log entry carries `at` and the time it played, so
  a headless test with a frozen and stepped clock can check that a held
  call plays on the step that presents.
- macOS later: `drawCompleted` is the native form. iOS later: this is
  where `prepare()` goes.

## Done looks like

The numbers are recorded: call to actuator, call to display, on the
Pixel 7 and one slower device. Then one of two ends. Either the item
closes on the numbers, with a note that says why `at` is not needed.
Or `at: "present"` exists, a held call plays on the swap of its frame
(checked headless against the stepped clock), and the measured gap
between haptic and visual is smaller with it than without.

## Not in this item

Compensating the actuator's latency by playing before the frame
(prediction), audio alignment.
