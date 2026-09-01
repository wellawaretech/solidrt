---
title: Standalone APK implementation notes
description: Working notes for the srt pack --apk pipeline (backlog/standalone-android-apk.md): what shipped in the first three stages, the traps future edits must know, and how it was verified on a device.
created: 2026-09-01
---

# Standalone APK implementation notes

Companion to `backlog/standalone-android-apk.md` (design and open work).
Everything below shipped 2026-09-01 and was verified on a real arm64 device
(SM-T500, Android 12); the emulator is NOT a verification signal until
`backlog/go-client-emulator-launch-crash.md` is fixed.

## What shipped

- Patcher: `packages/cli/src/pack/android/` - zip.ts (central-directory
  parse, verbatim entry copy, zero-fill extra-field padding: 16384 for
  stored `.so`, 4 for other stored entries), strings.ts (ResStringPool
  rewrite for the manifest pool at offset 8 and the arsc pool at offset 12;
  `manifestInfo` parses element chunks for package/versionCode/versionName),
  sign.ts (Signature Scheme v2 only, RSA-PKCS1-SHA256, alg 0x0103), key.ts
  (checked-in dev key), icon.ts (hand-assembled 1x1 background PNG),
  apk.ts (`patchApk` orchestrator).
- Runtime: `not(go)` SDL_main split in lattice/src/lib.rs boots
  `assets/app.srtapp` in place via `alloy::sdl_utils::packed_asset_location`
  (JNI `sourceDir` + AAssetManager fd for offset/len) and
  `forge::trailer::read_at`; shared loader in lattice/src/payload.rs.
- Android: SolidRTActivity (src/main) + per-flavor MainActivity subclasses;
  per-flavor jniLibs/assets sourceSets; prod ships deflated libs
  (`useLegacyPackaging` via the variant API) and the patchable adaptive
  icon slots; `make android-runtime` stages
  `dist/android-runtime/<abi>/` + the runner APK (also into
  packages/android-arm64-v8a/); release.yml builds the arm64 runner.

## Traps

- The go flavor's Java and libmain.so must ship together: the keyboard JNI
  exports renamed to `Java_com_solidrt_app_SolidRTActivity_*`, so an old
  staged .so with the new Java throws UnsatisfiedLinkError at launch. Any
  `make android-client` / `android-dist` rebuilds the pair.
- versionCode is edited in place as a typed int and MUST happen before the
  string-pool rewrite recomputes the file (patchManifest does this; keep
  the order).
- The label is located by value: BASE_LABELS in apk.ts must match the
  runner's `app_name` ("SolidRT App", prod strings.xml) and the go
  client's ("Player"). Changing either string is a contract change.
- The icon slots are entry-name contracts too: `res/drawable/app_icon_fg.png`
  and `app_icon_bg.png`, referenced from ic_launcher_prod.xml. The patcher
  never resamples; Android scales via the 22.5% inset (90% of the safe
  zone, so art never touches the mask edge).
- jni-sys version skew: the `jni` crate (jni-sys 0.4) and `ndk-sys`
  (jni-sys 0.3) name the same ABI types in different crates; the pointer
  casts at the `AAssetManager_fromJava` boundary are deliberate.
- Android XML resource comments must not contain `--` (aapt2 rejects the
  file), so `srt pack --apk` cannot be written literally there.

## Verified on device

Console packed to a 22.9 MB APK: installs beside com.solidrt.go under its
own id/label/icon, boots the payload with no extraction (cwd anchored to
/data/data/<appId>/files/data), icon renders uncut under Samsung's squircle
mask, versionCode update 1 -> 7 installs and the downgrade is refused
(INSTALL_FAILED_VERSION_DOWNGRADE), back-at-root backgrounds the activity.
apksigner verify, `zipalign -c -P 16 4` and `aapt2 dump badging` all pass
on every produced APK. The CI runner-publishing step is untested until a
release runs.
