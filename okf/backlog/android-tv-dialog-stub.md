---
title: The TV reports file dialogs as available and answers every request with a cancel
description: On the Philips TPM171E (Android TV 8) ACTION_OPEN_DOCUMENT resolves to the TV framework's stub package, so dialogsAvailable reads true, the capability is listed, and showDialog launches a picker that returns a cancel at once - the app sees an empty pick instead of NotSupportedError.
created: 2026-11-12
---

# The TV reports file dialogs as available and answers every request with a cancel

Seen 2026-11-12 while verifying step 3 of okf/done/ore-pump-and-window.md
on the TV (TPM171E, Android TV 8, the 32-bit build):

- `SolidRTActivity.dialogsAvailable` resolves `ACTION_OPEN_DOCUMENT` with
  `CATEGORY_OPENABLE` and type `*/*`, and the TV resolves it: logcat shows
  the intent starting in `com.google.android.tv.frameworkpackagestubs`,
  the stub package Android TV ships for intents it has no app for.
- So the client lists the "dialog" capability, `ore::dialog::available()`
  is true, and an `openFiles` call shows nothing: the stub finishes with
  a cancel at once and the app gets an empty list.
- The probe moved to ore unchanged in that step; the behavior is the
  same as before it.

The design wants false here ("an Android without a document picker (a
TV)", ore/src/dialog.rs), so an app can hide its Open and Save buttons.
Done looks like: `dialogsAvailable` treats a resolution to the stubs
package as no picker (match the resolved package name against the known
stub, or ask the resolved activity whether it is exported and enabled with
a real label), and the TV then lists no "dialog" capability while the
tablet keeps it. Verify on the TV and on the tablet through the dev
server's `/dialog` and the capability list.
