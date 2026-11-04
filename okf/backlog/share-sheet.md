---
title: Hand a file or text to the OS share sheet
description: "An app cannot give a file it made (a PNG from captureSnapshot plus encodeImage) or a text to another app: wanted a share call that opens the platform's share sheet (Android ACTION_SEND through a chooser and a FileProvider in the packed APK), listed as \"share\" in Flux.capabilities only where a sheet exists, so the app chooses its own fallback."
created: 2026-11-03
---

# Hand a file or text to the OS share sheet

An app can make an image of its own content: `captureSnapshot` renders a
node to pixels, `encodeImage` encodes a PNG, and `flux:fs` writes it to
app storage. It cannot hand the result to another app. There is no share
call and no `navigator.share`. The only way out today is `saveFile()`, and
the user then shares the file from a gallery or file manager. On mobile
the share sheet exists to avoid that detour.

## What done looks like

One call opens the platform's share sheet with a file (bytes plus a MIME
type and a name) and an optional text, and it settles when the sheet
closes.

- `"share"` is in `Flux.capabilities` only where the platform can show a
  sheet. A call elsewhere rejects with `NotSupportedError`, as in
  [flux:dialog](../../packages/flux-types/gui/dialog.d.ts). The runtime
  does NOT fall back to something else (a save dialog, the clipboard):
  the fallback is the app's UX decision, and the capability is how it
  knows to make it.
- One entry is enough. Every platform with a sheet takes files and text
  alike, and the platforms without one have neither, so support splits by
  platform, not by payload. If a platform turns up that shares text but
  not files, a `"share-files"` entry is added then; that is additive.
  There is no `navigator.canShare()`.

## What it involves

**Android.** `Intent.ACTION_SEND` with `EXTRA_STREAM` (and `EXTRA_TEXT`)
wrapped in `Intent.createChooser`. The bytes are written to a cache
directory that a `FileProvider` exposes, and the intent carries
`FLAG_GRANT_READ_URI_PERMISSION`. A `file://` URI does not work: the
platform throws `FileUriExposedException`. `sol pack` declares the
provider in the manifest (and its paths resource), next to the activity
in `lattice/android/app/src/main/AndroidManifest.xml`. Knowing which
target the user picked needs the `createChooser` variant with an
`IntentSender` callback. No variant says whether the target app finished
the send.

**Desktop.** macOS has `NSSharingServicePicker` and Windows has
`DataTransferManager`. Linux has no share portal and no common share
sheet, so Linux does not list the capability.

**Tests and agents.** As with dialogs, the test engine lists `"share"`
whatever its platform, and the share is recorded and answered by the test
(or by an agent over the control API) instead of a sheet showing.

## Open questions

- Cancel: a value or an error. Web `navigator.share` rejects with
  `AbortError` when the user cancels. Our dialog rule is that a cancel is
  a value, never an error. Keeping the web name means keeping its
  rejection; a `flux:share` module can resolve `{ shared: boolean }`
  instead. The web shape also brings `File` and `canShare`, which flux
  does not have or does not want.
- Capability meaning: "can show a sheet" (the dialog rule; an empty sheet
  on a device with no matching app is a cancel), or "a target exists" (a
  `queryIntentActivities` probe, whose answer changes as apps install).
- First platforms: Android alone first, with macOS and Windows later, or
  all three in one design.
- Placement: the chooser needs the activity, and on desktop a parent
  window, so the sheet is alloy. Writing the bytes to the provider's
  directory could be forge; whether that split earns its keep is open.
- A file already in app storage: accept a `flux:fs` file as well as bytes,
  or only bytes.
