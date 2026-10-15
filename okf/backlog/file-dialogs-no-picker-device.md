---
title: File dialogs on an Android without a document picker
description: On a device with no DocumentsUI (the TV) Flux.capabilities should omit "dialog" and a flux:dialog call should reject with a NotSupportedError; built that way, not yet seen on such a device.
created: 2026-10-15
---

# File dialogs on an Android without a document picker

What it looks like when hit: on the TV an app calling `openFiles()` hangs
with no picker and no rejection, or `Flux.capabilities` lists `"dialog"`
there although nothing can show one.

okf/done/file-dialogs.md verified the Android path on the tablet. The
other half of the availability contract, a device where
`SolidRTActivity.dialogsAvailable()` finds no activity for
`ACTION_OPEN_DOCUMENT`, has not been seen: the TV was off on 2026-10-15.

## Done looks like

On the dev client on the TV (armeabi-v7a, see the device loop notes):

- `Flux.capabilities` omits `"dialog"` and `/dialog` reports `available:
  false`.
- `openFiles()` rejects at once with an error named `NotSupportedError`.
- Should the TV turn out to have a picker after all, the tablet checks
  apply there instead and this item closes as a non-issue.

## What it involves

`make android-client ANDROID_ABI=armeabi-v7a CARGO_BUILD_JOBS=8`, the APK
steps of `apk-run` in lattice/Makefile.android for that ABI, `adb connect`
to the TV, a probe app with debug commands that start a dialog, driven
over the control API.
