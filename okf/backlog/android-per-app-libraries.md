---
title: Android Java libraries for platform features ship in the stock runner
description: "The model is set: the stock Player and runner carry everything, and a developer who wants less builds a custom runtime. That covers native code, but a custom runtime's APK is the stock APK with its .so files swapped in, so the Java libraries the platform features need (Firebase Messaging, Play Billing, WorkManager, ...) can be neither dropped nor added per runtime; open: what that costs each app, and how per-app configuration such as a Firebase project reaches the stock code."
created: 2026-11-04
---

# Android Java libraries for platform features ship in the stock runner

## The model

The stock runtime includes everything: the Player and the fat-LTO runtime
carry every capability, so any app runs on them unchanged. A developer
who wants a smaller or different build specifies and builds a custom
runtime, which the scaffold provisions (`sol init --runtime`,
packages/cli/runtime/runtime.mk). Platform features that need an Android
library follow the same model: the library goes into the stock runner,
and every packed app carries it. androidx.core is the first
(okf/done/share-sheet.md).

## What does not fit

On Android a custom runtime does not build its own APK: sol makes its APKs
from the stock ones and puts the runtime's `.so` files into them
(docs/runtime/index.md, "On Android, you do not supply the APKs"), and
`sol pack` then patches the packed app's values into a copy. Nothing at
that point runs Gradle (okf/backlog/play-store-aab.md). So the custom
runtime route slims and extends native code only:

- A Java library in the stock runner stays in every custom runtime's APK:
  a custom runtime built without push still carries Firebase Messaging.
- A Java library the stock runner lacks cannot be added: a vendor SDK
  that ships as an Android library (an AAR) has no way in, while one that
  ships as a `.so` does.

What it looks like when hit: a developer builds a custom runtime to drop
a feature, and the APK keeps the feature's Java library; or a developer
wants a vendor SDK that is an AAR and finds no place to put it.

The features in the design note's test set that need a library:

| Feature | Library | Per-app configuration |
|---|---|---|
| Push notifications | Firebase Messaging | the app's Firebase project |
| In-app purchases | Play Billing | none in the APK (products live in Play Console) |
| Review prompt | Play Review | none |
| Background sync | WorkManager (androidx.work) | none |
| Biometrics | androidx.biometric | none |
| Media session | androidx.media3 or androidx.media | none |

## Done looks like

- Each library's release APK cost measured as it lands, so "everything in
  the stock runner" stays a measured choice. androidx.core 1.18.0 (with
  the Kotlin standard library and kotlinx-coroutines it pulls): the dex
  grew from 2.67 MB to 6.53 MB uncompressed on the Player debug build;
  the release runner now carries 5.16 MB of dex, 1.69 MB compressed, in a
  29.0 MB APK (okf/done/share-sheet.md, Findings).
- The per-app configuration route decided before push: the stock runner
  cannot carry one app's `google-services.json`, so the project values
  come from the app at run time (`FirebaseApp.initializeApp(context,
  options)` takes them in code; to confirm it covers what Messaging
  needs).
- A stated answer for the two gaps above: accepted as a limit of the
  custom runtime (documented in docs/runtime), or a custom runtime that
  can build its own runner with Gradle when it needs to change the Java
  side.

## Notes

- R8 (`minifyEnabled`) is off in `lattice/android/app/build.gradle`, so
  each library ships whole. Turning it on would shrink the Java side to
  what is used, but every Java method native code calls over JNI needs a
  keep rule (`proguard-rules.pro` has SDL's, not ours), or the call fails
  at run time with `NoSuchMethodError`. Worth weighing once the libraries
  add up; against the native libraries, the Java side is small.
