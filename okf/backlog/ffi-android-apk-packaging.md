---
type: feature-proposal
title: APK packaging for flux:ffi libraries
description: Ship an app's ffi libraries in an asset folder, packaged into the APK's native-lib dir and opened by path automatically, since byte-loading is blocked by Android W^X policy.
status: deferred
tags: [flux, ffi, android, packaging]
timestamp: 2026-07-15T00:00:00Z
---

# APK packaging for flux:ffi libraries

flux:ffi's byte-loading route (`FfiLibrary::open_bytes`: write bytes to a temp
file, dlopen it) is impossible on Android by OS policy: SELinux forbids
untrusted apps (targetSdk 29+) from executing native code out of any
app-writable storage - the same W^X wall as data-dir subprocess exec. Since
2026-07-15 `open_bytes` fails fast on Android with a clear error instead of a
confusing EACCES. The only sanctioned route is `dlopen` by path from the APK's
`nativeLibraryDir` (how subprocess binaries already ship, as `lib*.so`).

This also means the generic dev-client APK can never run app-specific native
libraries delivered over the dev server; on Android the dev-time answer for
downloaded code is flux:wasm.

# Plan (when APK packaging work starts)

- An app project keeps its ffi libraries in a designated asset folder,
  organized per ABI.
- The packaging step copies them into the APK as `jniLibs` so they land in
  `nativeLibraryDir` at install time.
- At runtime, flux:ffi library resolution picks them up automatically by
  path (name lookup against `nativeLibraryDir`), so app code does not need
  platform-specific paths.
