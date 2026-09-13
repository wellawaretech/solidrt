---
title: Remote control input device for the input map
description: A TV remote reaches the app as key events or as gamepad buttons depending on whether alloy opened it as a joystick, and Android's auto-mapping can drop buttons it sends; a core remoteControl() device for createInputMap should hide both so an app binds select, navigate and back once.
created: 2026-09-12
---

# Remote control input device

## Symptom

An app that wants "OK was pressed on the remote" has no source to bind, and
has to know how a remote travels through SDL on Android:

- Android reports a remote as a SOURCE_DPAD device, which SDL counts as a
  joystick. `SDLActivity.handleKeyEvent` hands its keys to
  `Android_OnPadDown` first. When the device is opened as a joystick (alloy
  opens every mapped pad, and a TV lists several: the remotes, platform
  input nodes, a virtual search device, even a USB keyboard/mouse
  receiver), DPAD_CENTER becomes gamepad button "south", the D-pad becomes
  the "dpad*" buttons and BACK becomes "back" (raw index 4). Only when the
  device is not opened does SDL send them as keyboard keys (DPAD_CENTER as
  code "Select", key "Unidentified").
- A mapped pad's buttons come from SDL's Android auto-mapping, built from
  `InputDevice.hasKeys` (`SDLControllerManager.getButtonMask`; DPAD_CENTER
  sets the "a" bit). Remotes routinely fail that probe for BACK, which is
  why alloy's `take_back_edge` reads raw button 4 on Android. A remote that
  fails it for DPAD_CENTER too delivers its OK press to neither a key event
  nor `gamepads()`: the app cannot see it at all.
- Nothing tells the app which connected pad is the remote. `gamepad()` with
  no slot sums every pad, which works but also admits presses from the
  platform input devices.

Measured on the Android TV test device (2026-09-13), with an app logging
every key, pointer and pad press: the remote's OK arrives as pad button
"south" on one of the remote devices, so its auto-mapping has "a" and the
key path does not fire there. The select gap is not seen on that device;
the back gap is.

Components' `uiBindings` binds both known paths (key "Select" and pad
"south") for focus navigation; an app outside components rebuilds that
knowledge by hand, and neither covers the missing-mapping gap.

## Done looks like

- `remoteControl()` in core: a device for `createInputMap` next to
  `keyboard()` and `gamepad()`, with the sources a remote has (dpad,
  select, and media keys where present), merging the key path and the pad
  path so a binding does not depend on whether alloy opened the device.
- The runtime reads past a missing auto-mapping entry for select the way
  `take_back_edge` does for back, once a device shows that gap.
- `uiBindings` takes the remote device instead of spelling out both paths.
- Core's AGENTS.md names it as the device to bind for TV apps.

## Open

- Whether the remote can be told apart from the other pads (names are
  device-arbitrary; source flags would need a surface through SDL's Java
  layer), or summing all pads is the contract.

## Not in this item

Back stays with `onBack` and its handler stack, as okf/backlog/input-map-stage-2.md
records; the remote device does not add a back action.
