---
title: Gamepad rumble
description: gamepads() is a read-only snapshot; there is no path from the app back to the pad, so a collision, a landing or an engine can be seen and heard but not felt. SDL3 has SDL_RumbleGamepad (low/high frequency motors, duration) and trigger rumble on pads that support it; one call to plumb, keyed by the pad's slot.
created: 2026-08-30
---

# Gamepad rumble

## Symptom

`gamepads()` (`packages/core/src/gamepad.ts`) mirrors the runtime's
sticky "gamepads" event: names, buttons, axes, `mapped`. Nothing goes the
other way. Every console-shaped game uses the pad's motors - a hit, a
landing, an engine under load, a rumble strip - and it is the one
feedback channel the platform owns outright (a touch device's vibration
is the same call on a different device, a later sibling).

SDL3 exposes it directly: `SDL_RumbleGamepad(pad, low, high, ms)` (two
motor intensities 0..65535 and a duration, replacing any rumble in
progress; 0/0 stops it) and `SDL_RumbleGamepadTriggers` for pads with
trigger motors. The sdl3 crate wraps the first; check the second, and add
the `sdl_utils.rs` wrapper if it is missing.

## Shape

- Runtime: a command from the main thread to the SDL/gamepad owner
  (`alloy/src/gamepad.rs`), looked up by the pad's instance id from the
  snapshot; a pad that disconnected in between is a silent no-op, the
  same tolerance the snapshot has for reconnects.
- JS: `rumble(slot, { low, high, durationMs })` beside `gamepads()` in
  core, intensities 0..1 (the web Gamepad Haptics `playEffect`
  "dual-rumble" vocabulary: `strongMagnitude`/`weakMagnitude` map to
  low/high; keep the standard names or ours, decide once against the
  one-API-shape rule), `triggers?: { left, right }` when supported;
  `rumble(slot, null)` stops. Marshalled through the events/input plugin
  that carries the gamepad snapshot; `flux-types` parity.
- Validation: throw on an out-of-range magnitude, a slot with no pad is
  a no-op (the pad may have just left).

## Done looks like

A test app rumbles the pad on a button press with intensity from the
trigger axis; a pad without motors (or a raw HID joystick) accepts the
call and does nothing; `examples/` or a demo gains one line where a
collision already plays a sound.

## Not in this item

Custom haptic waveforms (SDL_haptic effects), phone vibration, force
feedback on wheels.
