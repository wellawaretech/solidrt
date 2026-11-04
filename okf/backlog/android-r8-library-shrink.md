---
title: Shrink the Android libraries' Java code with R8
description: "Every APK ships androidx.core and what it pulls (the Kotlin standard library, kotlinx-coroutines, lifecycle, ...) whole, about 3.9 MB of dex, because R8 is off (minifyEnabled = false, inherited from SDL's Android project template, never a decision): turn it on for release builds as a library-only shrink, with keep rules over all of our own Java and SDL's and no renaming, so only unreachable library code goes."
created: 2026-11-04
---

# Shrink the Android libraries' Java code with R8

What it looks like: every APK, the Player and every packed app alike,
carries androidx.core and its dependencies whole, though the app reaches a
few of their classes (`FileProvider`, `ShareCompat` and what they call).
Measured when androidx.core 1.18.0 came in for the share sheet
(okf/done/share-sheet.md, Findings): the Player debug build's dex went from
2.67 MB to 6.53 MB uncompressed, and the release runner carries 5.16 MB of
dex, 1.69 MB compressed, in a 29.0 MB APK. Each further library the
platform features bring (okf/backlog/android-per-app-libraries.md) adds to
it the same way.

The cause is one line, `minifyEnabled = false` in the release build type of
`lattice/android/app/build.gradle`, with the `proguardFiles` line under it
wired but inactive. Both came with SDL's Android project template
(`SDL/android-project/app/build.gradle` has the same two lines) in the
first Android commit, and nothing decided them since.

## Done looks like

- R8 on for the release build type (the runner and the published Player),
  as a library-only shrink: keep rules over all of `com.solidrt.app.**` and
  `org.libsdl.app.**`, renaming off (`-dontobfuscate`), so our own Java
  and SDL's stay exactly as written and only unreachable library code is
  removed. Debug builds unchanged.
- The release runner's dex size before and after, recorded here.
- Every Java path native code reaches tested once on a device on a
  release build (the runner through a packed app): the dialogs, the
  content-provider helpers, the share sheet, the video plane, the
  keyboard and touch callbacks, the audio and controller paths SDL drives.

## What it involves

**The trap: R8 only sees references in Java and Kotlin code.** A method
reached by name from native code looks unused, and R8 removes or renames
it. The build succeeds; the app fails with `NoSuchMethodError` (or an
unbound `native` method) the first time the call is made. The two kinds:

- Java methods native code calls by name (`jni_str!` in alloy, forge and
  ore): `showDialog`, `dialogsAvailable`, `share`, `contentOpen`,
  `contentStat`, `contentDisplayName`, `contentChildren`,
  `contentDelete`, `createVideoPlane`, `destroyVideoPlane`, `surface`,
  `refreshPeriodNs`, `redrawPresented`, and the platform methods
  `getPackageManager`, `hasSystemFeature`, `getAssets`,
  `getApplicationInfo`, `sourceDir` (framework classes, which R8 does not
  touch). SDL's own native code calls into `SDLActivity`,
  `SDLAudioManager`, `SDLControllerManager` and the HID classes the same
  way; `proguard-rules.pro` lists SDL's methods, none of ours.
- `native` methods, bound to the exported `Java_com_solidrt_app_...`
  symbols by their Java names: `nativeDialogResult`,
  `nativeHardwareKeyboard`, `nativeKeyboardInset`, `nativeShareResult`,
  `nativeVideoPlaneLost`, `nativeVideoPlaneVsync`, `nativeRedrawNeeded`,
  `nativeTouch`. The stock `proguard-android-optimize.txt` keeps the names
  of classes with native methods, but a whole-package keep is simpler
  than relying on it.

Keeping both packages whole covers all of these, and any method added
later, without a list to maintain; the list above is what the device test
walks.

**Leave resources alone.** `shrinkResources` stays off: `sol pack`
patches the runner APK by resource entry name (the icon slots, the backup
rule files, the label placeholder; packages/cli/src/pack/android/apk.ts),
the same reason `android.enableResourceOptimizations=false` is set in
`gradle.properties`. R8 itself does not touch resources or the manifest,
so pack's manifest patching is unaffected; checking that a packed APK
still patches and installs is part of the device test.

**Custom runtimes** derive their APKs from the stock ones by swapping
`.so` files (docs/runtime/index.md), so they inherit the shrunk Java side
with nothing to do.

With renaming off, stack traces stay readable and no mapping file needs
keeping.
