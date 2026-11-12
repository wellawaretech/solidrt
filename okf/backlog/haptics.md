---
title: Device haptics
description: An app cannot make the device's actuator respond, so a tap, a toggle, an error or a game hit can be seen and heard but not felt. One platform-neutral model (feedback named by intent, waveforms as intensity/sharpness envelopes, a support query) built on Android now through our own JNI, shaped so iOS, macOS and Windows slot in later without changing the API.
created: 2026-11-12
---

# Device haptics

## Symptom

Nothing in core or flux reaches the device's haptic actuator. A phone app
uses it constantly for small feedback (a switch that flipped, a list that
snapped, a long press that took, a rejected input) and a game uses it for
hits, engines and alarms. The project config has a `vibration` capability
(`packages/cli/src/lib/project.ts`), and `sol pack` maps it to
`android.permission.VIBRATE`, but no API uses it.

Android is the platform built now. iOS has to follow (with the port,
okf/backlog/ios-port-readiness.md), and the desktops where they have
hardware. The API is designed against all of them now, so a later
platform is a new backend, never a new shape.

Gamepad rumble is a separate item, [gamepad-haptics](gamepad-haptics.md):
two motors keyed by a pad, through SDL. This item is the device's own
actuator. The two keep one vocabulary where they overlap (intensity 0..1,
durations in ms).

## Why not SDL

SDL's haptic API does list the Android device motor (SDL 3.4.18:
`SDLHapticHandler.pollHapticDevices()` adds it as `VIBRATOR_SERVICE`, the
only haptic device on Android), but it can only play a one-shot at one
amplitude. That is too little for an engine: patterns would be timed
one-shots from our own thread, and the system effects, the user's touch
feedback setting and usage routing are out of reach. The sdl3 crate
(0.20.0) cannot open the device anyway: its one open path is
`SDL_OpenHapticFromJoystick`, which is always false on Android. So this
is the case where SDL stops and ore uses our own native code: JNI to
`Vibrator` and `View`, Objective-C later on Apple platforms.

## The model

Two kinds of haptics, plus a support query. Every platform maps onto
these; no platform's constants appear in the API.

### Feedback, named by intent

The app says what happened, not how it should feel. Each platform plays
its own native effect for the intent, follows the user's system
settings, and has a documented nearest effect or a no-op where it has
none. The rule for the set: an intent is in when Android or iOS has a
native effect for it. Proposed set (final names are the first decision):

| Intent | Meaning | Android | iOS | macOS trackpad |
|---|---|---|---|---|
| `selection` | moved to another discrete choice (picker, slider step) | `SEGMENT_TICK` (34), else `CLOCK_TICK` | `UISelectionFeedbackGenerator` | `alignment` |
| `alignment` | snapped to a guide or an edge | `SEGMENT_TICK` (34), else `CLOCK_TICK` | `UICanvasFeedbackGenerator` alignment (17.5) | `alignment` |
| `threshold` | a drag crossed the point where release acts (pull to refresh) | `GESTURE_THRESHOLD_ACTIVATE` / `_DEACTIVATE` (34) | impact, light | `levelChange` |
| `toggleOn`, `toggleOff` | a switch changed | `TOGGLE_ON` / `TOGGLE_OFF` (34) | impact, light | `generic` |
| `longPress` | a long press triggered an action | `LONG_PRESS` | impact, medium | none |
| `dragStart` | an item was picked up | `DRAG_START` (34) | impact, light | none |
| `success`, `warning`, `error` | an action finished, needs care, failed | `CONFIRM` / composition / `REJECT` (30) | `UINotificationFeedbackGenerator` | `generic` |
| `impact` (weight: light, medium, heavy; intensity 0..1) | a collision, a landing, a physical hit | predefined `EFFECT_TICK` / `CLICK` / `HEAVY_CLICK` (29), primitives with a scale (30) | `UIImpactFeedbackGenerator` with intensity | `generic` |

Apple's SwiftUI `SensoryFeedback` (iOS 17) is the closest thing to a
cross-platform intent vocabulary; check the set against it before fixing
the names. Windows haptic pens (`SimpleHapticsController` named
waveforms) map the same way when that backend comes.

Android detail: the intents with a `HapticFeedbackConstants` constant go
through `View.performHapticFeedback` on the decor view. That honours the
user's touch feedback setting and (to confirm) needs no `VIBRATE`
permission. It must run on the UI thread: a static helper on
`SolidRTActivity`, posted with `runOnUiThread` (the video plane
precedent), and the hop's latency measured. Below the API level where a
constant exists, ore uses the documented fallback for that SDK level.
The intents with no constant (`impact`, `warning`) go through `Vibrator`
with predefined or composed effects, usage `TOUCH`.

### Waveforms, as envelopes

For games, alarms and anything with a duration: an envelope of control
points. Each point has a target intensity (0..1), an optional target
sharpness (0..1, crisp against smooth) and the transition time to reach
it. The envelope starts at zero intensity and must end at zero. This is
where the platforms converge:

- Android 16 (API 36) has exactly this: `VibrationEffect.BasicEnvelopeBuilder`
  (`addControlPoint(intensity, sharpness, durationMillis)`, smooth
  transitions, must end at zero, sharpness mapped to the device's
  frequency range). Use it when the device supports envelope effects.
- Below that, with amplitude control (`Vibrator.hasAmplitudeControl`):
  `VibrationEffect.createWaveform(timings, amplitudes, -1)` (API 26, our
  minSdk), the ramps sampled at a named step and sharpness ignored. The
  whole pattern is played natively, never sequenced from our side.
- Without amplitude control: the same waveform, on/off at a named
  intensity threshold.
- iOS later: a Core Haptics continuous event with intensity and
  sharpness parameter curves, the same data.

A step (an instant change) is a point with transition 0; ore maps it to
the platform's minimum control point duration. A new waveform replaces
the one in progress; `stop()` cancels. The argument is an object
(`{ points, initialSharpness? }`), so transient taps inside a pattern
(Core Haptics transient events, Android composition primitives) can be
added later as another field, not a new function.

`navigator.vibrate(pattern)` (the web standard, in
`flux/src/standards_plugins/`) is a thin layer over this: the on/off
pattern becomes steps at full intensity, `0` or `[]` stops, and it
returns whether the device has an actuator. Simplified against the web:
no user-activation rule (a single known app does not need it).

### Options on both

- `purpose`: `touch` (default for feedback), `media` (default for
  waveforms; games), `notification`, `alarm`. Android routes the user's
  settings by it (`VibrationAttributes` usage from API 33, audio usage
  before); platforms without the idea ignore it. It is in the shape now
  because adding it later would change every call.
- `at`: `now` (default) or `present`, play with the frame that shows the
  change. A haptic that lands a frame before or after the visual reads
  as wrong. lattice owns the scheduling (a policy over alloy's present
  facts, accurate as okf/backlog/presentation-feedback.md makes them);
  macOS has it natively (`drawCompleted`), and on iOS it is where the
  generators' `prepare()` goes.

### Support

A synchronous fact the app reads once: whether the device has an
actuator, for each intent whether it is `native`, `approximate` or
`none`, and whether intensity and sharpness are controllable. Every call
is still accepted where nothing plays, so app code needs no platform
branches; the support query is for apps that design around it (a game
that uses audio cues instead).

### Rules on every platform

The user's settings always win: no flag overrides them, even where a
platform still has one. Everything stops when the app goes to the
background; ore cancels on the pause it already observes
(`ore/src/lifecycle.rs`).

## Shape

- ore: `ore/src/haptics.rs`, the facility (support, feedback, play,
  stop) with the Android backend in JNI, and helpers on
  `SolidRTActivity` for the UI-thread work. Every other platform reports
  no actuator. Read okf/design/platform-layering.md first.
- flux: `flux:haptics` in `flux/src/ore_plugins/` (`feedback`, `play`,
  `stop`, `support`) and `navigator.vibrate` in
  `flux/src/standards_plugins/`, both thin marshals over ore.
  `flux-types` parity. How core surfaces it follows how the other ore
  facilities reach apps.
- lattice: the `at: "present"` scheduling.
- Capability: rename `vibration` to `haptics` (project config, scaffold
  `package.json`, the pack permission map and its docs). It maps to
  `VIBRATE`, which `Vibrator` needs.
- Validation: throw on an unknown intent, a value outside 0..1, a
  negative or non-finite duration, an envelope that does not end at
  zero. A pattern longer than a named maximum is clamped, as the web
  does.
- Dev: the control API records every accepted call with its time (the
  `get_requested_links` precedent) and an MCP tool reads them back, so a
  headless test verifies haptics on a machine with no actuator.
- Docs and the core AGENTS topic: the intent set with its meaning, so
  app authors pick by meaning, not by feel.

## Done looks like

On an Android phone, a test app plays every intent and an envelope with
ramps and steps; a second envelope replaces the first; going to the
background stops it; turning off touch feedback in the system settings
silences the `touch` intents. On an older device (below API 34, and
without amplitude control if one is at hand) the fallbacks play. On a
device without an actuator and on desktop, `support` says so and every
call is a silent no-op. Headless tests read the recorded calls. The
mapping tables for iOS, macOS and Windows in this item are checked
against those platforms' docs, so their backends need no API change.

## Not in this item

The iOS, macOS and Windows backends (designed here, built with each
platform), gamepad rumble ([gamepad-haptics](gamepad-haptics.md)),
transient events inside a waveform (the shape leaves room), audio-coupled
haptics (Core Haptics audio events, Android `HapticGenerator`), Linux
(no app-facing haptics).
