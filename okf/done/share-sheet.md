---
title: Hand a file or text to the OS share sheet
description: "navigator.share({ title, text, url, files }) opens the platform's share sheet, Android first (ACTION_SEND through a chooser, a FileProvider over app storage in the packed APK): an ore facility marshalled in standards_plugins, files as flux:fs handles, a cancel rejecting with AbortError, defined and listed as \"share\" only where a sheet exists, recorded instead of shown under sol test and readable by an agent."
created: 2026-11-03
completed: 2026-11-04
---

# Hand a file or text to the OS share sheet

## Problem

An app can make an image of its own content: `captureSnapshot` renders a
node to pixels, `encodeImage` encodes a PNG, and `flux:fs` writes it to
app storage. It cannot hand the result to another app. There is no share
call and no `navigator.share`. The only way out today is `saveFile()`, and
the user then shares the file from a gallery or file manager. On mobile
the share sheet exists to avoid that detour.

## Done looks like

- In app code, on Android:

  ```ts
  let png = file("share/board.png")
  await png.write(encodeImage(await captureSnapshot(boardNode)))
  try {
    await navigator.share({ text: "My board", files: [png] })
  } catch (e) {
    if (e.name !== "AbortError") throw e
  }
  ```

  It resolves once the platform's sheet accepted the share; on Android
  that is when the user picked a target app, or used one of the sheet's
  own actions (Copy). It rejects with `AbortError`
  when the user closed the sheet without picking, where the platform
  reports a dismissal (Android does), and with `InvalidStateError` while
  another sheet is open. It says nothing about whether the target app
  finished the send. The wording leaves room for a platform that reports
  no dismissal (Windows' share UI is expected to be one): there the call
  resolves once the sheet took the data, and a dismissal goes unreported.
- A shared file must stay in place after the call resolves: the target
  app gets a reference, not the bytes, and reads them when it likes (a
  messenger uploads in the background, a mail app reads at send). The
  doc comment says so: reuse a fixed name the next share overwrites, or
  clean up much later, never right after the `await`.
- `navigator.share` is defined, and `"share"` is in `Flux.capabilities`,
  only where the platform can show a sheet: Android, for now. Elsewhere
  `navigator.share` is undefined, so the web's own check (`if
  (navigator.share)`) and the capability agree. The runtime does NOT fall
  back to something else (a save dialog, the clipboard): the fallback is
  the app's UX decision.
- Under `sol test` no sheet shows: the share is recorded and resolves at
  once, the test engine lists `"share"` and defines `navigator.share`
  whatever its platform, and `app.requestedShares()` says what the app
  shared. A dev client shows the sheet and records the share too, for an
  agent to read (`get_requested_shares`).
- A packed APK carries the FileProvider under its own authority, and two
  packed apps install side by side.
- Verified on the tablet: a PNG and a text reach a target app, a dismissed
  sheet rejects with `AbortError`, and a document picked with
  `flux:dialog` is passed on.

## Decisions

- **Android first** (2026-11-04). Sharing is a mobile habit. Desktop has
  it only partly: macOS has a real share menu (`NSSharingServicePicker`),
  Windows a little-used one (`DataTransferManager`), Linux none. Both
  desktop sheets also want a parent window, which ore does not have yet.
  They are additive later (the capability gates them), or never.
- **Placement: ore** (2026-11-04, okf/design/platform-layering.md, the
  placement order). The sheet is an OS facility: ore starts the chooser
  through the activity (JNI with the JavaVM and activity alloy publishes
  in ndk-context, no SDL). The Java half sits beside the dialogs in
  `SolidRTActivity`; its result comes back over a JNI symbol the embedder
  exports (lattice, as `nativeDialogResult` does) and forwards to ore.
  Nothing goes in alloy or forge.
- **The web's shape: `navigator.share({ title?, text?, url?, files? })`**
  (2026-11-04), marshalled in `standards_plugins/` and backed by ore, as
  `navigator.languages` is (the SolidRT lens: standard names and shapes,
  simplified semantics). Simplified: no transient-activation rule (one
  known app), no `canShare()` (the capability says it), `navigator.share`
  defined only where `"share"` is listed. Data with none of the four
  fields is a TypeError, as on the web.
- **A cancel is the web's `AbortError`** (2026-11-04). The dialog rule (a
  cancel is a value) belongs to `flux:dialog`'s own names; keeping the
  web's name means keeping its contract, so web-trained code runs
  unchanged. A second call while a sheet is open rejects with
  `InvalidStateError`, as on the web. The contract is worded per
  platform from the start ("resolves once the sheet accepted the share",
  "a dismissal rejects where the platform reports one"), so a desktop
  sheet that cannot report a dismissal adds a line to the docs, not a
  behavior change. macOS's picker reports both outcomes.
- **`files` holds `flux:fs` handles only** (2026-11-04), the runtime's
  one file model (flux has no `File` or `Blob`). An app writes its bytes
  with `flux:fs` first, then shares the handle. The handle's `name` is
  the shared name; Android's `MimeTypeMap` gives the MIME type from the
  extension (the dialogs' lookup). A file in app storage goes out through
  the FileProvider; a `content://` document the app picked with
  `flux:dialog` is passed on with the grant it holds. Anything else (a
  path outside app storage) is a TypeError. Only text travels inside the
  intent (`EXTRA_TEXT`); a file goes as a `content://` URI with a read
  grant, since an intent crosses processes in one transaction of about
  1 MB, and the target streams the bytes from our provider, later.
- **The capability means "can show a sheet"** (2026-11-04), the dialog
  rule. Android's chooser always shows, even with no matching app, and
  closing it is then a cancel. A `queryIntentActivities` probe changes as
  apps install, so a capability read once at startup would go stale.
- **Tests and agents take the platform's place through a session**, the
  shape of `ore::link::Session`: every share is logged (the last 64), one
  sheet at a time, a record-only session shows no sheet and resolves at
  once. A read for each host: `@solidrt/test` next to `requestedLinks`, a
  control endpoint and an MCP tool.
- **androidx.core for the provider and the intent** (2026-11-04).
  `FileProvider` serves our files to the target app (the URI to file
  mapping, the path-traversal checks, the name, size and type queries,
  all of them easy to get wrong in a hand-written provider), and
  `ShareCompat.IntentBuilder` builds the send intent with its per-URI
  grants. It is the project's first Gradle dependency, pinned to an exact
  version. It would come in anyway: every Android library the test set
  points to (Firebase Messaging, WorkManager, Play Billing, Play Review,
  biometric, media3) is built on it. Like everything, it goes into the
  stock runner and so into every packed app; what a custom runtime cannot
  change on the Java side is its own item,
  okf/backlog/android-per-app-libraries.md. The whole artifact ships,
  its unreachable classes (`NotificationCompat`, `ShortcutManagerCompat`,
  ...) and its small androidx dependencies included: Gradle adds whole
  libraries, and only R8 removes what is unreachable. R8 stays off for
  now; the measured size (work step 3) says whether a library-only
  shrink (keep rules over all of `com.solidrt.app` and
  `org.libsdl.app`, no renaming) earns its own item; it did:
  okf/backlog/android-r8-library-shrink.md. A hand-written
  provider was rejected: androidx comes in with the next platform
  features anyway.
- **Not in scope:** answering a share with a cancel under test (an app's
  `AbortError` branch is untested until then; additive, as `app.dialog`
  answers are); desktop sheets; knowing which target app was picked (the
  chooser reports it; nothing asks for it yet).

## Work

1. **Spike on the tablet: the chooser's result.** A throwaway intent from
   `SolidRTActivity`: `createChooser` with an `IntentSender` (an explicit
   `PendingIntent`, `setPackage` and `FLAG_MUTABLE`, as targetSdk 34+
   requires for a mutable one) and `startActivityForResult`. Record the
   order and timing of the chosen-target broadcast and `onActivityResult`
   for a pick, a BACK on the sheet, and a pick then BACK in the target.
   Done 2026-11-04; the answer is in Findings: the share settles on the
   broadcast (resolve) or on the activity result (resolve on OK, reject
   on a cancel with no broadcast before it), never by waiting for the
   user to come back.
2. **ore: `ore::share`.** The request (`title`, `text`, `url`, files as
   `{ location, name }`), the outcome (`Shared`, or `Cancelled`, `Busy`,
   `Unavailable`, `Failed(String)`), `available()` (Android only), and the
   `Session` (the log, record-only, one pending share). The Android half:
   the JNI call into `SolidRTActivity.share` through ndk-context, and
   `result_raw` for the embedder's symbol. ore takes `jni` and
   `ndk-context` on Android, alloy's exact pins. Tests in
   `src/tests/share.rs` for the session.
3. **Android.** `androidx.core:core` in `lattice/android/app/build.gradle`,
   an exact version; the APK size it adds, measured on the runner and the
   Player release builds, goes in Findings. `SolidRTActivity.share(id,
   paths, uris, names, title, text)`: the send intent from
   `ShareCompat.IntentBuilder` (`ACTION_SEND` or `ACTION_SEND_MULTIPLE`,
   the streams, the text then the url, the title as the subject, the read
   grants), the files in app storage through
   `FileProvider.getUriForFile`, the chooser from step 1. The
   chosen-target broadcast calls `nativeShareResult(id, true)` at once;
   the `onActivityResult` branch calls `nativeShareResult(id, resultCode
   == RESULT_OK)` only when no broadcast came first, and a result after
   the broadcast is ignored. The
   manifest declares `androidx.core.content.FileProvider` (authority
   `${applicationId}` itself, not exported, `grantUriPermissions`) with
   `res/xml/share_paths.xml` over the files dir (`<files-path
   path="."/>`; SDL's pref path, the app storage root, is
   `getFilesDir()`).
4. **lattice.** Export `Java_com_solidrt_app_SolidRTActivity_nativeShareResult`
   and forward it to ore, beside `nativeDialogResult`.
5. **sol pack.** The runner APK's manifest carries the runner's provider
   authority; without a rename, two packed apps conflict on install
   (`INSTALL_FAILED_CONFLICTING_PROVIDER`). The authority is the
   application id spelled identically, so the pool holds it once and
   `patchManifest`'s package rename covers it, as it does the app's own
   link scheme. The libraries' `${applicationId}.<suffix>` names need more:
   every pool string that starts with the runner's id and a dot is
   renamed too (Findings). A pack test for the rename.
6. **flux.** `standards_plugins/share.rs`: `navigator.share` installed on
   the navigator only where `ore::share::available()`, or when the session
   is set to record only; `"share"` in the capabilities on the same rule.
   Validation throws a TypeError (no field set, a field of the wrong type,
   a file that is not a `flux:fs` handle or not in app storage or a
   picked document); the outcome maps to resolve, `AbortError`,
   `InvalidStateError`, `NotSupportedError`. The call holds the engine
   `in_flight("share")` until the sheet closes, as a dialog does. Tests in
   `flux/tests/share.test.ts` on the test host.
7. **Recording reads.** The stepped engines and the `flux --test` host set
   the session to record only (beside `link::set_record_only`).
   `sol:test` `requestedShares()`, `@solidrt/test` `app.requestedShares()`
   (`{ title, text, url, files: [{ path, name }] }[]`, oldest first),
   `GET /__control__/requested-shares`, the MCP tool
   `get_requested_shares`; their docs (packages/test/AGENTS.md,
   packages/cli/agents/testing.md and debugging.md). The debugging note
   says what it says for dialogs: on a visible client a share shows the
   sheet, and only the user can close it.
8. **Types and docs.** `packages/flux-types/standards/share.d.ts`
   (`Navigator.share?`, the data and the simplified contract), the
   `docs/runtime/index.md` globals list, a paragraph in
   `packages/core/docs/index.md` next to `openLink`.
9. **Verify on the tablet**: the PNG and text share, a text-only share, a
   BACK on the sheet, a picked document passed on, and a packed APK
   installed beside Player.dev.

## Findings

- The chooser's result on the SM-T500 (Android 12, Samsung's share panel
  over the platform `ChooserActivity`), from a throwaway text share with
  an `IntentSender` (explicit, `FLAG_MUTABLE`) and
  `startActivityForResult`:
  - BACK on the sheet: `onActivityResult` with `RESULT_CANCELED` and no
    data at once; no chosen-target broadcast.
  - A target picked (Calendar): the broadcast arrives at the pick, with
    the target's component. `onActivityResult` does not; it arrives only
    when the user comes back to the app, here 38 s later, and it is
    `RESULT_CANCELED` even so. Waiting for the result would hold the
    share for as long as the user stays in the target, or forever.
  - The sheet's own Copy action: `onActivityResult` with `RESULT_OK` at
    once, no broadcast.
  - So the outcome is: the broadcast resolves; an OK result resolves; a
    cancelled result with no broadcast before it rejects (`AbortError`);
    a result after the broadcast is ignored. The sheet is closed at the
    broadcast, so a second share can start while the target is still in
    front.
- Samsung's panel opens with a "Quick Share" tooltip over it: the first
  BACK closes the tooltip, not the sheet. Worth knowing when driving the
  sheet with `adb` in verification.
- androidx.core 1.19 needs `compileSdk` 37 (its AAR metadata says so, and
  Gradle's `checkAarMetadata` refuses the build); 1.18.0 is the newest for
  the project's 36. The pin moves with the compile SDK.
- androidx.core is not small: it pulls the Kotlin standard library,
  kotlinx-coroutines, lifecycle, collection and a few more. Measured on
  the Player debug build, the APK's dex went from 2.67 MB to 6.53 MB
  uncompressed (2.36 MB compressed after). The release runner carries
  5.16 MB of dex, 1.69 MB compressed, in a 29.0 MB APK. With R8 off, all
  of it ships.
- The library manifests derive names from `${applicationId}` that the
  package rename did not touch: androidx's startup provider authority
  (`<id>.androidx-startup`) and its receiver permission
  (`<id>.DYNAMIC_RECEIVER_NOT_EXPORTED_PERMISSION`). Left at the runner's
  id, two packed apps would declare the same provider
  (`INSTALL_FAILED_CONFLICTING_PROVIDER`) and, under different signing
  keys, the same permission (`INSTALL_FAILED_DUPLICATE_PERMISSION`).
  `sol pack` now renames every pool string that starts with the runner's
  id and a dot (`applicationIdRenames` in
  packages/cli/src/pack/android/strings.ts), which covers the next library
  that does the same (Firebase's init provider). The share provider's own
  authority is the id itself, spelled identically, so the pool holds it
  once with the package.
- Verified on the SM-T500: a text and url share shows both on the sheet
  (the url on its own line) and a BACK rejects with `AbortError`; a PNG
  written to app storage shows its thumbnail on the sheet, resolves at the
  pick of Drive, and Drive's dialog reads its name and pixels through the
  provider; a second share while a sheet is open rejects with
  `InvalidStateError`; a photo picked with `flux:dialog` is passed on with
  its grant and Drive reads it; `/__control__/requested-shares` lists what
  was shared. Two packed apps (`com.solidrt.sharecheck.a` and `.b`, both
  with the provider) install beside Player.dev and the Player and run.
