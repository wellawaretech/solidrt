---
title: Haptic waveforms - envelopes and navigator.vibrate
description: Feedback by intent covers a tap or a toggle, but an engine, an alarm or a pattern with a duration has no API. A waveform is an envelope of intensity and sharpness control points, played natively on Android (the envelope builder on 16, a sampled waveform below), with the routing purpose as an option and the web's navigator.vibrate as a thin layer over it.
created: 2026-11-13
---

# Haptic waveforms - envelopes and navigator.vibrate

## Symptom

[haptics](haptics.md) plays a short native effect for an intent. A game
needs more: an engine under load, a rumble strip, an alarm, a charge
that builds. Nothing with a duration or a shape can be played, and code
written for the web that calls `navigator.vibrate` finds no such
function.

This item builds on the facility of [haptics](haptics.md) (ore module,
`flux:haptics`, the capability, the dev recording) and needs it first.

## The model

### An envelope of control points

Each point has a target intensity (0..1), an optional target sharpness
(0..1, crisp against smooth) and the transition time to reach it. The
envelope starts at zero intensity. This is where the platforms converge:

- Android 16 (API 36) has exactly this: `VibrationEffect.BasicEnvelopeBuilder`
  (`addControlPoint(intensity, sharpness, durationMillis)`, smooth
  transitions, must end at zero, sharpness mapped to the device's
  frequency range). Use it when the device supports envelope effects
  (`Vibrator.areEnvelopeEffectsSupported`).
- Below that, with amplitude control (`Vibrator.hasAmplitudeControl`):
  `VibrationEffect.createWaveform(timings, amplitudes, -1)` (API 26, our
  minSdk), the ramps sampled at a named step and sharpness ignored. The
  whole pattern is played natively, never sequenced from our side.
- Without amplitude control: the same waveform, on/off at a named
  intensity threshold.
- iOS later: a Core Haptics continuous event with intensity and
  sharpness parameter curves, the same data.

A step (an instant change) is a point with transition 0; ore maps it to
the platform's minimum control point duration.

The app does not have to end at zero. Where the last point is above
zero, ore appends the release (a step to zero), because Android's
builder requires it and a caller who forgets it should not get an
error for what has one obvious meaning.

A new waveform replaces the one in progress; `stop()` cancels. The
argument is an object (`{ points, initialSharpness? }`), so transient
taps inside a pattern (Core Haptics transient events, Android
composition primitives) can be added later as another field, not a new
function.

### Repeat (the first decision)

An engine or an alarm runs until the app stops it. Without a repeat the
app re-issues the waveform, and since a new waveform replaces the one in
progress, each re-issue is a gap that can be felt. So the shape needs
`repeat`, and what it means is open:

- Android: `createWaveform` takes a repeat index; API 36 has
  `VibrationEffect.createRepeatingEffect` for the envelope path (to
  confirm, with its rules).
- iOS: `CHHapticAdvancedPatternPlayer.loopEnabled`.
- The envelope starts and ends at zero, so a loop of the whole envelope
  pulses. A steady hum needs a loop section that does not pass through
  zero (an attack, then a repeating body). Whether Android's envelope
  path allows that decides between `repeat: true` (the whole envelope)
  and a repeat-from index.

A repeating waveform stops on `stop()`, on a replacing waveform and when
the app goes to the background.

### Purpose

`purpose` says how the system routes the user's settings: `media`
(default for waveforms; games) or `touch`. Android routes by it
(`VibrationAttributes` usage from API 33, audio usage before); platforms
without the idea ignore it. To confirm on a device below API 33: that
the setting for each purpose is applied there.

To decide here: whether `feedback("impact", ..)` takes `purpose` too, so
a game's hits follow the media setting and not the touch feedback
setting. The other intents stay touch feedback.

`notification` and `alarm` are not in the set: everything stops when the
app goes to the background, so a foreground-only API has no use for
them. They arrive with background work, as an additive value.

### navigator.vibrate

`navigator.vibrate(pattern)` (the web standard, in
`flux/src/standards_plugins/`) is a thin layer over the envelope: the
on/off pattern becomes steps at full intensity, purpose `media`, and
`0` or `[]` stops. The standard's shape stays:

- It returns `true` when the pattern is accepted, with or without an
  actuator, as the web does. An app that needs the hardware fact reads
  `support`.
- It does not throw on a long pattern or a long entry: it truncates and
  clamps at named maxima, as the web does. A value that is not a number
  is a `TypeError`.

Simplified against the web: no user-activation rule and no hidden-page
rule (a single known app does not need the first, and the background
rule covers the second).

### Support

The support fact of [haptics](haptics.md) gains: whether waveform
intensity is controllable (Android: amplitude control) and whether
sharpness is (Android: envelope effects).

## Shape

- ore: `play` and the three Android paths in `ore/src/haptics.rs`.
- flux: `play` on `flux:haptics`; `navigator.vibrate` in
  `flux/src/standards_plugins/navigator.rs`. Both thin marshals.
  `flux-types` parity.
- Validation on `play`: throw on a value outside 0..1, a negative or
  non-finite duration, an empty point list, an unknown purpose. An
  envelope longer than a named maximum is clamped.
- Without the capability: `play` and `navigator.vibrate` are no-ops with
  the one warning of [haptics](haptics.md); support reports no waveform
  control.
- Dev recording: `play`, `stop` and `navigator.vibrate` land in the same
  log.
- Docs and the core AGENTS topic.

## Done looks like

On an Android phone, a test app plays an envelope with ramps and steps;
a second envelope replaces the first; `stop()` cancels; a repeating
waveform runs until stopped, without a gap that can be felt; going to
the background stops it. On an Android 16 device the envelope builder
path plays and sharpness can be felt; below it the sampled waveform
plays; without amplitude control (if such a device is at hand) the
on/off form plays. The media vibration setting silences a `media`
waveform and the touch feedback setting a `touch` one.
`navigator.vibrate([200, 100, 200])` plays and `navigator.vibrate(0)`
stops. On desktop every call is a silent no-op and `navigator.vibrate`
still returns `true`. Headless tests read the recorded calls.

## Not in this item

Transient events inside a waveform (the shape leaves room),
audio-coupled haptics (Core Haptics audio events, Android
`HapticGenerator`), the `notification` and `alarm` purposes, playing
with the frame ([haptics-at-present](haptics-at-present.md)), the other
platforms' backends.
