---
title: Standalone APK for a packed app
description: An app can be packed into a native executable for every desktop platform but not into an installable Android app; the runtime has no Android boot path for a packed payload, and building the APK should not require an Android SDK on the developer's machine.
created: 2026-09-01
---

# Standalone APK for a packed app

`srt pack` produces a standalone executable for the platform it runs on.
Android has no equivalent: the only Android artifact is `solidrt-go.apk`, the
dev client, which hosts apps behind its player and dev-server connection. An
app cannot be handed to someone as an installable Android app.

Two independent halves, and only the second one is about zip files.

## Half 1: the runtime has no Android boot path for a packed app

On Android the native library is always `liblattice.so` built `--lib` with the
`go` feature, so `SDL_main` boots the player. The production payload loader
(`FactoryPayload`, `load_payload`) lives in `lattice/src/main.rs`, a `[[bin]]`
target that is never built for Android.

Decided shape: a real production Android runtime, not the go client with the
player suppressed. A packed APK must not carry the dev server, the player
or the BSOD.

The pieces this lands on:

- `SDL_main` (`lattice/src/lib.rs`) is gated on `target_os = "android"` only,
  not on the `go` feature - but its body calls the go-gated
  `embedded_fonts()`, so a `RUNTIME_FEATURES` cdylib did not actually compile
  for Android before the split. The split: `go` keeps the player path,
  `not(go)` loads the packed payload (and registers the payload's fonts).
- The payload loader moves out of `main.rs` into a module both targets share,
  parameterized by source instead of always `current_exe()`.
- `AssetsBase::Packed { exe, index }` (`forge/src/fs.rs`) resolves every asset
  read as a ranged read against a path, reopened per read. The APK itself can
  be that path: `ndk::asset::Asset::open_file_descriptor()` returns
  `(fd, offset, size)` for a stored (uncompressed) asset, and the APK path is
  `ApplicationInfo.sourceDir`. So the payload stays mmapped in place inside the
  APK with no extraction, and `MainActivity.extractAssets()` (which copies
  every asset into `filesDir` on every launch) never runs in this flavor.
- `forge::trailer::read` needs a `read_at(path, base, len, magic)` variant that
  rebases section offsets, since the `.srtapp` sits at an offset inside the APK.
- alloy already carries `ndk 0.9`, `ndk-context` and `jni`, and already makes
  this kind of JNI call (`sdl_utils.rs`, the touchscreen-feature probe), so the
  platform call belongs there and adds no dependency. For the asset fd path the
  `.srtapp` must live under `assets/` in the APK; an arbitrary zip entry is not
  visible to `AAssetManager`.
- `MainActivity.java` lives in `src/go` only. The prod flavor compiles but has
  no activity class, so it crashes at launch: the shared manifest declares
  `.MainActivity`. The activity splits: the shared parts (SDLActivity wiring,
  the keyboard-inset JNI) move to `src/main`, the go-only parts
  (`extractAssets`, the `srt_dev_server` intent extra) into a `src/go`
  subclass.
- `lattice/Makefile.android` needs a `android-runtime` target, and the `prod`
  flavor in `lattice/android/app/build.gradle` (declared, with an empty
  `src/prod`) needs its own `jniLibs.srcDir` so it picks up the runtime `.so`
  rather than the go one - and its own (empty) assets dir, since
  `sourceSets.main` points assets at `dist/assets`, which would pack the go
  player's assets into every runner APK as dead weight.

Exit already behaves correctly for a standalone app: player-less builds
background the activity instead of returning to a player.

## Half 2: building the APK without an Android SDK

An APK cannot be synthesized from a folder: `AndroidManifest.xml` is compiled
binary XML, resources are compiled, and Java must be dex. That stays a Gradle
job. But it only has to happen once: CI publishes a runner APK per ABI
alongside `solidrt-go.apk`, and `srt pack --apk` patches a copy of it in
pure TypeScript with no dependencies.

What the patch has to do, measured against the shipped
`packages/android-arm64-v8a/solidrt-go.apk` (31 entries):

- **Application id.** The AXML string pool has `com.solidrt.go` as a single
  entry (index 52). The activity is stored fully qualified
  (`com.solidrt.app.MainActivity`), so changing the id leaves the dex alone -
  the same reason `srt android` can `am start -n
  com.solidrt.go/com.solidrt.app.MainActivity` today. Source: `solidrt.appId`
  from package.json, which is already reverse-DNS and already keys storage.
  Android also wants an integer `versionCode` (and a display `versionName`),
  which package.json has no field for.
- **Label and icon.** `resources.arsc` is 2844 bytes; its global string pool is
  18 strings, index 0 being the label (`SolidRT Go`) and 1-17 resource file
  paths including `res/mipmap-anydpi-v26/ic_player.xml`. Both edits are the
  same primitive as the id edit: rewrite a `ResStringPool` chunk (rebuild
  offsets, fix enclosing chunk sizes). One module serves both files.
- **Alignment.** Native libs are STORED and padded through the extra field to
  16384-byte boundaries (`libmain.so` data lands at 10698752); `resources.arsc`
  is STORED and 4-aligned. A naive `zip -r` breaks installs on Android 15. Since
  the central directory gets rewritten for signing anyway, entry bytes are
  copied verbatim and the padding is recomputed, which is `zipalign -P 16`
  (uppercase: page size in KB; lowercase `-p` is the old 4096 paging).
- **Signing.** The shipped APK carries exactly one signature block,
  `0x7109871a` (Scheme v2), plus a padding block: no v1, no v3. v2 is also the
  minimum Android accepts for targetSdk 30+. Chunked SHA-256 over
  entries/central directory/EOCD plus one RSA signature, which `node:crypto`
  covers. A fixed certificate shipped in the CLI (same posture as the
  checked-in `debug.keystore`) makes sideloading work out of the box. To
  state the posture plainly: anyone holding the shipped key can sign an
  update to any app packed with it, so the fixed key is for handing builds
  around, never for store publishing. Distribution signing is deferred:
  when it lands it is package.json config (a PEM key + cert pair, which
  `node:crypto` reads natively - not a JKS/PKCS12 keystore, which would
  need an ASN.1 parser), never a flag; meanwhile pack prints a note that
  the shared dev key signed the APK.
- **Payload.** The `.srtapp` added as a STORED entry, which is what makes half
  1's file-descriptor path work at all.

This half is testable before the runner APK exists: patch today's
`solidrt-go.apk` to a different id and label, install it beside the real one,
launch it. That exercises alignment, AXML, arsc and signing with no Rust
involved.

## Open: download size

Native libs are packaged uncompressed so they can be mmapped from the APK
(`extractNativeLibs=false`), which puts a floor under the APK equal to the
libs themselves: arm64-only that is about 49 MB after AGP's strip
(`libmain.so` 38 MB, impeller 6.9 MB, SDL 2.4 MB, libc++ 1.4 MB). Dropping the
`go` feature takes some off `libmain.so`, but not the shape of the number.

Decided: the runner ships its libs deflated (`extractNativeLibs=true`, set
on the prod flavor at Gradle time). Stored-uncompressed is Play-delivery
logic - the store compresses the transfer, so stored costs no download and
saves the extracted copy - but this APK path is direct distribution by
definition (Play is the AAB item), where the wire is the file. Measured on
the first arm64 runner APK (2026-09-01): the four libs are 40.7 MB stored
(libmain 30.1 after dropping `go`, impeller 6.9, SDL 2.4, libc++ 1.4) and
deflate to 16.9 MB, so the APK drops from 40.8 to about 17 MB for a
one-time extracted copy on device. The 16 KB alignment work stays: the
payload and resources.arsc are still stored entries.

Stage-3 config rule: every packaging knob (versionCode, icon, permissions,
keystore, label) is controlled through package.json's `solidrt` config the
way fonts already are - nothing CLI-flag-only, nothing baked in.

Decided: runner APKs are per ABI, built for the target device - a shipped
app carries neither an emulator ABI nor a fat penalty (`solidrt-go.apk`, fat
arm64 + x86_64, is 101 MB against the 40.8 MB arm64 runner). CI publishes
one runner per ABI (arm64-v8a first; x86_64 only as a dev artifact for the
emulator). Play Store publishing is a separate future item and a different
artifact: an AAB carrying all ABIs (bundletool container, protobuf
manifest, Play signing, split per device at delivery), not a fat APK.

## Related

- `app-icons.md` says "Android is a non-issue ... apps run inside the client,
  so the APK's own icon is the client's". A standalone APK retires that: its
  icon and label are the app's, so Android joins that item's stage 3. The
  TypeScript CLI cannot rasterize SVG (`resvg` is a go-feature dep), so the
  first cut accepts a PNG (the `icon` config key naming one; an `.svg` keeps
  the placeholder with a pack-time note). Decided shape - stay adaptive, let
  Android do the geometry: every launcher masks icons (circle/squircle/
  rounded rect) and only the central 66/108 safe zone survives all masks, so
  a plain full-bleed PNG always risks getting cut. The runner bakes an
  adaptive icon with two patchable PNG slots: a 1x1 background pixel
  (stretched full-bleed; generated at pack time from an `iconBackground`
  color, default near-black) and a foreground wrapped in a 22.5% `<inset>`
  drawable, so any square PNG lands at 90% of the safe zone with no
  resampling in the patcher. The PNG must fill its bitmap edge-to-edge:
  padding belongs to the inset, never to the file (transparent margin in
  the file shrinks the mark, since the inset cannot see through it). The
  go client's foreground uses the same PNG-plus-22.5%-inset shape. Later,
  additively: an inset override for adaptive-aware full-bleed art, and a
  `<monochrome>` layer for themed icons.
- TV banner: on Android TV the launcher shows `android:banner` (160x90dp)
  instead of the icon, and it replaces the label entirely - a banner
  without text is an anonymous tile on the shelf, which is why Android TV
  guidance wants the app name in it. The go client's banner
  (`res/drawable-xhdpi/tv_banner.png`, the mark centered on the official
  near-black ground) is mark-only today, so "Player" should be rendered
  into it. Packed apps inherit that same static banner, so on a TV every
  packed app shows the SolidRT banner, not its own: a patchable banner
  slot (same mechanism as `app_icon_fg.png`) plus pack-time composition of
  the app's `displayName` over its icon would fix it - but text rendering
  at pack time has the same problem as SVG rasterization
  (`icon-svg-rasterization.md`), so the two probably share a solution.
- `ffi-android-apk-packaging.md` wants an app's ffi libraries copied into the
  APK as `jniLibs` so `dlopen` by path works. That is the same packaging step
  as half 2, one more thing to inject.
- Permissions: the camera permission is already go-flavor-only
  (`src/go/AndroidManifest.xml`), so the prod runner inherits only
  RECORD_AUDIO, VIBRATE and INTERNET from the shared manifest. That Gradle
  set is only the default the runner ships with, not a ceiling: the patcher
  can splice `<uses-permission>` element chunks out (or in - the
  `android:name` attribute is already in the resource map) per app at pack
  time. Element chunks are self-contained, so it is the same class of
  surgery as the id edit: chunk splicing plus the pool and size fixups.
