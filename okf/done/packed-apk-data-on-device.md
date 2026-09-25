---
title: Keep a packed APK's data on the device, and let it declare permissions
description: Every APK from srt pack --apk was debuggable (the runner was a Gradle debug build, so adb run-as read the app's private files), backed up everything (allowBackup on, so Auto Backup copied data, fetch cache, logs and the p2p identity key to the cloud and to a new phone), and could not use the camera (permissions were the runner's fixed set); the runner is now a release build with backup rules that keep everything on the device, and package.json switches capabilities and the backup intention at the top level in platform-neutral terms, with an android group for versionCode and extra permissions.
created: 2026-09-25
completed: 2026-09-25
---

# Keep a packed APK's data on the device, and let it declare permissions

Every packed app inherited the runner's `<application>` flags and its
permission set:

- **`android:debuggable="true"`.** No manifest source said so: Gradle wrote
  it because `android-runtime` built the runner with `assembleProdDebug`.
  With it, `adb shell run-as <app-id>` is a shell as the app, so anyone with
  USB access to the unlocked phone could read its sqlite files, logs and
  identity key. Play refuses a debuggable upload, which
  [play-store-aab](../backlog/play-store-aab.md) avoids by planning
  `bundleProdRelease`; the sideloaded APK had no such guard.
- **`android:allowBackup="true"`**, from the shared
  `src/main/AndroidManifest.xml`. Auto Backup copied the app's files to the
  user's cloud backup and, on device-to-device transfer, to a new phone.
- **A fixed permission set.** The runner declared RECORD_AUDIO, VIBRATE and
  INTERNET; CAMERA was go-flavor-only. A packed app that opened the camera
  never got the permission prompt (SDL requests it at open time, but only a
  declared permission can be requested), and there was no way to declare
  one.

On Android SDL's pref path is `getFilesDir()` itself (org and app are
ignored there), so the packed-app tree in `lattice/src/storage.rs` sat
inside the backup whole: `data/`, and also `cache/`, `logs/` and
`identity/`. The last three are wrong to back up regardless of what the app
wants:

- `cache/` (the fetch disk cache) and `logs/` count toward the 25 MB
  per-app quota. Over it, Auto Backup stops backing up the app at all, so a
  large fetch cache silently turns off the backup of the data that matters.
- `identity/` holds the persisted iroh key. A device-to-device transfer
  leaves the old phone intact, so afterwards two devices share one p2p
  identity.

Whether `data/` is backed up is the app's call: the engine cannot know
whether it holds game settings or private notes.

## What landed (2026-09-25)

### Config: intentions at the top level, an `android` group for the rest

```json
"solidrt": {
  "appId": "com.example.app",
  "icon": "./assets/icon.svg",
  "iconBackground": "#1a1a1a",
  "capabilities": { "camera": true, "microphone": true, "vibration": true, "internet": true },
  "backup": false,
  "android": {
    "versionCode": 1,
    "permissions": ["com.android.vending.BILLING"]
  }
}
```

The line drawn: the top level says what the app is and intends, in
platform-neutral terms, and a platform that does not implement an intention
simply ignores it; a per-target group holds only what that target alone
needs. So:

- `iconBackground` moved from Android-only to the top level: it describes
  the artwork (the ground behind a transparent foreground), which Android
  adaptive icons, Windows tiles and Apple's layered icons all compose.
- `capabilities` is a map of platform-neutral names to switches, like
  `fonts`: `camera`, `microphone`, `vibration`, `internet` (everything the
  runtime can ask the user for; unknown names fail). All on by default and
  all listed in the scaffold, so a packed app does what it did in the
  player and dropping one is a one-line edit; nobody has to discover after
  the fact which permission a feature needed. The APK packer maps them to
  CAMERA, RECORD_AUDIO, VIBRATE, INTERNET; a later macOS or Windows packer
  maps the same map to usage descriptions or MSIX capabilities. The name
  went back and forth: "permissions" reads plainer, but it would then be
  used twice (the top-level map and the Android extras), and the collision
  with the `flux:*` capability modules was judged the lesser problem.
- `backup` is an intention every platform has (iOS: iCloud backup); Android
  is the first to implement it.
- `android.permissions` is the escape hatch for what no capability covers
  (billing, push, vendor), fully qualified names only: a bare `CAMERA` is
  refused, so nobody has to remember which namespace a name lives in.
- `android.versionCode` stays explicit (default 1). Deriving it from
  `version` was considered and rejected: a scheme that ignores part of the
  version (a prerelease tag) hands out the same code twice, and that is a
  footgun. A derivation without that hole is an open idea (ideas.md).

An unknown key fails, at the top level and inside the group alike, since a
misspelled knob (or a stale name from before a rename) would otherwise
silently ship a default. Breaking, no shim: `project.ts`, `pack/main.ts`,
the scaffold's nulls, `pack/docs.md`.

### Permissions: spliced into the compiled manifest

Every permission moved from the shared manifest to the go overlay: the dev
client hosts any app, so it declares everything; the prod runner declares
none, and a packed app's manifest carries exactly what package.json keeps
on.

`strings.ts` grew an element walker (the root-element parse `manifestInfo`
did by hand) and `addUsesPermissions`: the element name and the permission
strings are appended to the manifest's pool (new indices at the end; the
resource-map chunk only covers the first N, so nothing shifts), the
`android:name` attribute's name and namespace indices are taken from any
existing one (every activity has it), and the start/end chunk pair is
written from scratch in aapt2's layout and inserted before `<application>`.
Names already declared are skipped. The pool rewrite that appends the
strings also fixes the file size the splice grew.

### Runner: release build, nothing backed up by default

- `android-runtime` runs `assembleProdRelease` and stages
  `app-prod-release.apk` as `solidrt.apk`. The release build type already
  existed (checked-in debug keystore, minify off).
- The prod overlay sets `allowBackup="false"` (via `tools:replace`),
  `dataExtractionRules` (Android 12+) and `fullBackupContent` (8 to 11),
  and `<profileable android:shell="true"/>` so simpleperf and Perfetto keep
  working on a packed app; profileable grants no file access.
- Both rule files exclude every domain at its root. That is deliberate: a
  missing or empty section means "everything", and at targetSdk 31+
  `allowBackup="false"` disables cloud backup but, on some manufacturers'
  devices, not device-to-device transfer (Auto Backup docs), so the rules
  are what actually keeps a transfer empty.
- The runner also carries a data-only rule pair (`res/xml/backup_data.xml`,
  `extraction_data.xml`: include domain `file`, path `data`). With
  `backup: true` the patcher flips the `allowBackup` boolean in place (the
  same edit as `versionCode`) and overwrites the "none" entries' bytes with
  the data-only ones, so the manifest's resource references never change
  and no resource-id lookup is needed.

## Verification

- Patcher on the runner (done 2026-09-25): the compiled manifest shows the
  new `<uses-permission>` elements with pool indices past the old count, no
  moved references, duplicates skipped; Android's
  own `aapt2 dump badging` lists them, and `aapt2 dump xmltree` on the rule
  files shows the swap for `backup: true`.
- Release runner (done): no `debuggable` attribute, `allowBackup` false,
  both rule attributes present, the `profileable` element. AGP's release
  resource optimization DID shorten the `res/` entry names
  (`res/drawable/app_icon_fg.png` became `res/9n.png`), which would have
  broken the icon patch silently and the rule swap loudly;
  `android.enableResourceOptimizations=false` in `gradle.properties` keeps
  the names.
- Device: `dumpsys package` shows the listed permission requested and
  neither DEBUGGABLE nor ALLOW_BACKUP among the flags, `run-as` refuses,
  the camera opens in the app.

## Out of scope

- Moving `cache/` into `getCacheDir()`, where the OS excludes it and "Clear
  cache" reaches it: a storage-layout question
  ([app-storage-vendor-path](../backlog/app-storage-vendor-path.md)). The
  rules exclude it meanwhile.
- The implied-feature filter on Play (a CAMERA permission makes Play require
  a camera): noted in play-store-aab.
- The shared development signing key: play-store-aab, Signing.
- Encrypting data at rest.
