---
title: Device haptics - feedback by intent
description: An app cannot make the device's actuator respond, so a tap, a toggle, an error or a hit can be seen and heard but not felt. Feedback named by intent plus a support query, platform-neutral, built on Android now through our own JNI and shaped so iOS, macOS and Windows slot in later without changing the API. Waveforms and present-aligned playback are their own items.
created: 2026-11-12
---

# Device haptics - feedback by intent

## Symptom

Nothing in core or flux reaches the device's haptic actuator. A phone app
uses it constantly for small feedback (a switch that flipped, a list that
snapped, a long press that took, a rejected input) and a game uses it for
hits. The project config has a `vibration` capability
(`packages/cli/src/lib/project.ts`), and `sol pack` maps it to
`android.permission.VIBRATE`, but no API uses it.

Android is the platform built now. iOS has to follow (with the port,
okf/backlog/ios-port-readiness.md), and the desktops where they have
hardware. The API is designed against all of them now, so a later
platform is a new backend, never a new shape.

This is the first of three items on the device's own actuator:

- this one: feedback by intent, the support query, the facility in ore
  and its dev recording;
- [haptic-waveforms](haptic-waveforms.md): envelopes for games and
  alarms, and `navigator.vibrate`;
- [haptics-at-present](haptics-at-present.md): playing with the frame
  that shows the change.

Gamepad rumble is a separate facility, [gamepad-haptics](gamepad-haptics.md):
two motors keyed by a pad, through SDL. The two keep one vocabulary where
they overlap (intensity 0..1, durations in ms).

## Why not SDL

SDL's haptic API does list the Android device motor (SDL 3.4.18:
`SDLHapticHandler.pollHapticDevices()` adds it as `VIBRATOR_SERVICE`, the
only haptic device on Android), but it can only play a one-shot at one
amplitude. The system effects, the user's touch feedback setting and
usage routing are out of reach. The sdl3 crate (0.20.0) cannot open the
device anyway: its one open path is `SDL_OpenHapticFromJoystick`, which
is always false on Android. So this is the case where SDL stops and ore
uses our own native code: JNI to `Vibrator` and `View`, Objective-C later
on Apple platforms.

## The model

The app says what happened, not how it should feel. Each platform plays
its own native effect for the intent, follows the user's system
settings, and has a documented nearest effect or a no-op where it has
none. No platform's constants appear in the API.

Two rules for the set:

- An intent is in when Android or iOS has a native effect for it that
  differs from the effects of the other intents on that platform.
- An intent is a name. A parameter exists only where the value is a
  weight or continuous (`impact`); a two-sided event is two names
  (`toggleOn`, `toggleOff`).

Proposed set (final names are the first decision):

| Intent | Meaning | Android | iOS | macOS trackpad |
|---|---|---|---|---|
| `selection` | moved to another discrete choice (picker, slider step) | `SEGMENT_TICK` (34), else `CLOCK_TICK` | `UISelectionFeedbackGenerator` | `alignment` |
| `alignment` | snapped to a guide or an edge | as `selection` | `UICanvasFeedbackGenerator` alignment (17.5), else selection | `alignment` |
| `thresholdOn`, `thresholdOff` | a drag crossed the point where release acts, or crossed back (pull to refresh) | `GESTURE_THRESHOLD_ACTIVATE` / `_DEACTIVATE` (34), else `CLOCK_TICK` | impact, light | `levelChange` |
| `toggleOn`, `toggleOff` | a switch changed | `TOGGLE_ON` / `TOGGLE_OFF` (34), else `CLOCK_TICK` | impact, light | `generic` |
| `longPress` | a long press triggered an action | `LONG_PRESS` | impact, medium | none |
| `dragStart` | an item was picked up | `DRAG_START` (34), else `LONG_PRESS` | impact, light | none |
| `success`, `error` | an action finished, failed | `CONFIRM` / `REJECT` (30), else the predefined effects below | `UINotificationFeedbackGenerator` | `generic` |
| `warning` | an action needs care | predefined `EFFECT_DOUBLE_CLICK` (29) | `UINotificationFeedbackGenerator` | `generic` |
| `impact` (weight: light, medium, heavy; intensity 0..1) | a collision, a landing, a physical hit | see below | `UIImpactFeedbackGenerator` with intensity | `generic` |

`alignment` earns its place on iOS only; on Android it is `selection`
under another name, and support reports it `approximate` there.

Apple's SwiftUI `SensoryFeedback` (iOS 17) is the closest thing to a
cross-platform intent vocabulary; check the set against it before fixing
the names. Windows haptic pens (`SimpleHapticsController` named
waveforms) map the same way when that backend comes.

### Android paths

Two paths, by whether the intent has a `HapticFeedbackConstants`
constant on the running SDK level.

**With a constant: `View.performHapticFeedback` on the decor view.** It
honours the user's touch feedback setting and (to confirm) needs no
`VIBRATE` permission. It must run on the UI thread: a static helper on
`SolidRTActivity`, posted with `runOnUiThread` (the `share` and video
plane precedent). Measure the hop's latency and record it;
[haptics-at-present](haptics-at-present.md) needs the number.

**Without one: `Vibrator`, usage `TOUCH`.** This covers `warning`,
`impact`, and the fallbacks of the first path below the API level of
their constant. It needs `VIBRATE`.

- API 29 and up: `VibrationEffect.createPredefined`. `impact` light,
  medium, heavy is `EFFECT_TICK`, `EFFECT_CLICK`, `EFFECT_HEAVY_CLICK`;
  `success` and `error` below 30 are `EFFECT_CLICK` and
  `EFFECT_DOUBLE_CLICK`.
- `impact` intensity needs composition primitives (API 30) and a device
  that has them (`Vibrator.arePrimitivesSupported`): the primitive for
  the weight, with the intensity as its scale. Without them the
  predefined effect plays and the intensity is ignored.
- API 26 to 28: `VibrationEffect.createOneShot` with a named duration
  per effect, and a named amplitude where the device has amplitude
  control.

To confirm, and it decides a rule below: that the `Vibrator` path
follows the touch feedback setting on every SDK level. From API 33 the
usage is a `VibrationAttributes` usage; before that it is an audio
usage, and how far the system applies the setting there is not checked.

### Options

None in this item. Feedback is touch feedback: it follows the touch
feedback setting. Two options were considered for the first shape and
left out, because an optional field with a default is additive and can
arrive with the item that needs it:

- `purpose` (how the system routes the user's settings) arrives with
  [haptic-waveforms](haptic-waveforms.md). Whether `impact` takes it,
  so a game's hits follow the media setting, is decided there.
- `at` (play with the frame) is [haptics-at-present](haptics-at-present.md).

So the second argument of `feedback` is an object from the start
(`feedback("impact", { weight, intensity })`).

### Support

A synchronous fact the app reads once:

- whether the device has an actuator;
- for each intent, whether it is `native`, `approximate` or `none`;
- whether `impact` intensity is honoured (Android: the device has the
  primitives).

Every call is still accepted where nothing plays, so app code needs no
platform branches; the support query is for apps that design around it
(a game that uses audio cues instead).

Support reports hardware and build facts, not the user's settings: an
intent the user has silenced is still `native`.

### Without the capability

An app that switches the capability off has no `VIBRATE`. The intents on
the `performHapticFeedback` path still play. The intents on the
`Vibrator` path report `none`, their calls are no-ops, and the first
such call logs one warning that names the capability. ore checks the
permission once; it never lets `Vibrator` throw.

### Rules on every platform

The user's settings always win: no flag overrides them, even where a
platform still has one. Everything stops when the app goes to the
background: ore's haptics module cancels in its own handling of the
lifecycle watch (`ore/src/lifecycle.rs`), as `ore/src/audio.rs` does
for the playback device.

## Shape

- ore: `ore/src/haptics.rs`, the facility (`support`, `feedback`,
  `stop`) with the Android backend in JNI, and the helper on
  `SolidRTActivity` for the UI-thread work. Every other platform reports
  no actuator. Read okf/design/platform-layering.md first.
- flux: `flux:haptics` in `flux/src/ore_plugins/` (`feedback`, `stop`,
  `support`), a thin marshal over ore. `flux-types` parity.
- core: re-exports the module's functions, as `packages/core/src/window.ts`
  does for `flux:link`.
- Capability: rename `vibration` to `haptics` (project config, scaffold
  `package.json`, the pack permission map and its docs). It still maps
  to `VIBRATE`. This breaks existing project configs: the parser rejects
  an unknown capability name, so the error names the new one.
- Validation: throw on an unknown intent, an unknown weight, an
  intensity outside 0..1 or not finite.
- Dev recording, after the link precedent (`ore/src/link.rs`): ore keeps
  a bounded log of every accepted call with its time, and a record-only
  session plays nothing. lattice reads it back for the control API and
  for `@solidrt/test` (`requestedHaptics()`, beside `requestedLinks()`),
  and an MCP tool reads the control API. A headless test verifies
  haptics on a machine with no actuator.
- Docs and the core AGENTS topic: the intent set with its meaning, so
  app authors pick by meaning, not by feel.

## Done looks like

On an Android phone, a test app plays every intent, and `impact` at
three weights and several intensities. Turning off touch feedback in the
system settings silences all of them, on a device at API 33 or above and
on one below it. Going to the background stops a playing effect. On a
device below API 34 the fallbacks play; on one below API 29 if one is at
hand. With the capability switched off, the `Vibrator` intents report
`none`, play nothing and warn once, and the others still play. On a
device without an actuator and on desktop, `support` says so and every
call is a silent no-op. Headless tests read the recorded calls. The
renamed capability packs, and the old name is refused with a message
that names the new one. The UI-thread hop's latency is measured and
written into [haptics-at-present](haptics-at-present.md). The mapping
table for iOS, macOS and Windows is checked against those platforms'
docs, so their backends need no API change.

## Not in this item

- Waveforms, `purpose` and `navigator.vibrate`
  ([haptic-waveforms](haptic-waveforms.md)).
- Playing with the frame ([haptics-at-present](haptics-at-present.md)).
- The iOS, macOS and Windows backends: designed here, built with each
  platform.
- Gamepad rumble ([gamepad-haptics](gamepad-haptics.md)).
- Feedback that core's own gestures and components play by default (a
  long press that took, a scroll that snapped). An app calls `feedback`
  itself; the defaults are a line in `okf/ideas.md`.
- Linux (no app-facing haptics).
