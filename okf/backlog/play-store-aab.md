---
title: Play Store publishing with srt pack --aab
description: A packed app can be sideloaded as an APK but not uploaded to Google Play, which only accepts an Android App Bundle signed with an app-specific upload key; patch a runner AAB skeleton the way the runner APK is patched, with the native libs lifted from the per-ABI runner APKs and packaging config grouped per target in package.json.
created: 2026-09-21
---

# Play Store publishing with srt pack --aab

`srt pack --apk` (`done`: `done/standalone-android-apk.md`, notes in
`notes/standalone-apk-implementation.md`) produces an installable APK for
handing around. Google Play refuses it twice over: new apps must be uploaded
as an Android App Bundle, and every APK today is signed with the shared
development key checked into `packages/cli/src/pack/android/key.ts`, which
must never become an app's upload key.

Done looks like: `srt pack --aab` writes `dist/<name>.aab` from the same
pack folder `--apk` uses, signed with the project's upload key, carrying the
ABIs the project lists, and Play Console accepts it for an internal-testing
track with no Android SDK on the developer's machine. The patch approach
stays: nothing at pack time runs Gradle, bundletool or Java.

## The bundle is a zip, so the patch approach carries over

An AAB is a zip with a fixed layout that Play feeds to bundletool, which
splits it into per-device APKs and signs those with the app signing key Play
holds. Play does not care how the bundle was built. So, as for the APK, CI
builds a runner bundle once with Gradle (`bundleProdRelease`) and `srt pack
--aab` patches a copy. What is patched is mostly new, because the bundle
stores its metadata as aapt2 protobuf, not binary XML:

| APK (today)                    | AAB                                          |
| ------------------------------ | -------------------------------------------- |
| `AndroidManifest.xml` (AXML)   | `base/manifest/AndroidManifest.xml` (proto)  |
| `resources.arsc`               | `base/resources.pb` (proto)                  |
| `lib/<abi>/*.so`               | `base/lib/<abi>/*.so` + `base/native.pb`     |
| `assets/app.srtapp`            | `base/assets/app.srtapp`                     |
| entry alignment (16 KB pages)  | not needed: bundletool builds the APKs       |
| Signature Scheme v2 block      | JAR (v1) signature: `META-INF/*.SF`, `*.RSA` |
| -                              | `BundleConfig.pb` (compression rules)        |

## Decided: a skeleton bundle, libs lifted from the runner APKs

The runner AAB ships without native libs: manifest, `resources.pb`, dex,
`BundleConfig.pb`, the patchable icon slots. About 1 MB. At pack time the
libs are copied out of the per-ABI runner APKs (`solidrt.apk` in
`@solidrt/android-<abi>`, which already ship) into `base/lib/<abi>/`. Why:

- no new multi-ABI package duplicating 40 MB of libs per ABI;
- the runner APK and the bundle cannot drift: the libs are byte-identical,
  built by the same CI job;
- the skeleton is small enough to ride inside `@solidrt/android-arm64-v8a`
  next to `solidrt.apk`. arm64 is the one ABI every bundle carries (below),
  so that package is always installed when `--aab` runs.

Build side: `lattice/Makefile.android` gains `android-bundle-skeleton`
(run `bundleProdRelease` with a single-ABI filter, strip `base/lib/**` and
`base/native.pb`, stage `solidrt.aab` into `packages/android-arm64-v8a/`);
`android-dist` for arm64 calls it; `release.yml` adds the file to the arm64
upload list. Gradle itself does not change: the compression rules the bundle
needs are written by the patcher (below), so they live in one place. One
check before relying on it: AGP must not write `extractNativeLibs` into the
bundle manifest (it should come from `BundleConfig.pb` only); if it does, the
patcher drops the attribute.

## ABIs

Play's rule (developer.android.com/google/play/requirements/64-bit): an app
must ship a 64-bit build, and for each 32-bit ABI it ships it must include
the 64-bit counterpart (armeabi-v7a needs arm64-v8a, x86 needs x86_64). An
arm64-only bundle is accepted. x86_64 is not required; it is recommended for
ChromeOS, where some x86 devices cannot translate arm64 and an arm64-only
app is simply not offered.

So: `android.abis` in package.json, default `["arm64-v8a", "armeabi-v7a"]`.
Pack enforces the pairing rule (a 32-bit ABI without its 64-bit partner is
refused) and requires the `@solidrt/android-<abi>` package for every listed
ABI. Adding `"x86_64"` opts into ChromeOS; that runner is already a CI
artifact. The bundle grows per ABI but user downloads do not: Play delivers
one ABI per device.

## Packaging config: grouped per target

Every packaging knob is package.json config (rule from the APK item), and a
flat namespace under `solidrt` does not survive the target list this repo
is heading for: Android (APK, AAB), Windows, macOS, Linux (several formats),
stores that span platforms (Steam). Platform is the wrong axis too: Steam
is not a platform, Flatpak is one Linux format of several.

Decided shape: one object per named group under `solidrt`, named after the
thing whose keys it holds, whether a platform (`android`, `windows`,
`macos`, `linux`), a format (`flatpak`, `msix`) or a store (`steam`,
`appstore`). Groups sit side by side, never nested by platform. The top
level keeps only what every target uses: `appId`, `displayName`, `icon`,
`fonts`, `entry`.

```json
"solidrt": {
  "appId": "com.example.app",
  "displayName": "Example",
  "icon": "./assets/icon.svg",
  "android": {
    "versionCode": 1,
    "iconBackground": "#1a1a1a",
    "abis": ["arm64-v8a", "armeabi-v7a"],
    "signingKey": "../keys/upload.key.pem",
    "signingCert": "../keys/upload.cert.pem"
  }
}
```

`--apk` and `--aab` both read `android`: same upload key, same versionCode,
icon background and (later) permissions; `abis` is read by `--aab` only,
the APK being single-ABI by decision. If one target ever needs a key the
other does not, it gets a sub-object (`android.aab`) then, additively.
`loadProject` validates a group the way it validates the top level; an
unknown key inside a group fails.

The group landed with
[packed-apk-data-on-device](../done/packed-apk-data-on-device.md)
(2026-09-25) with a sharper line than sketched above: `versionCode` and
`permissions` (fully qualified extras) are the group; `iconBackground`,
the `capabilities` map and `backup` are top-level intentions every packer
reads.
`patchBundle` applies the same mapping to the proto manifest.

## Signing

- The upload key is a PEM private key plus a PEM certificate, by path, so
  the key can live outside the repo. `node:crypto` reads both natively; a
  JKS/PKCS12 keystore would need an ASN.1 parser and stays out. An
  encrypted PEM takes its passphrase from an environment variable, since a
  secret cannot sit in package.json. `pack/docs.md` gets the `openssl`
  recipe for a fresh key and the `keytool` export path for an existing
  keystore.
- `--aab` refuses to run without a configured key. Play registers the
  upload key from the first upload; a bundle signed with the shared dev key
  would make that public key the app's upload key for good.
- `--apk` uses the configured key when present and otherwise falls back to
  the dev key with the existing note. `sign.ts` takes the key as a
  parameter instead of reading `DEV_KEY_PKCS8`.
- Bundle signing is a JAR signature: `META-INF/MANIFEST.MF` (SHA-256 per
  entry), `META-INF/<alias>.SF` (digests over the manifest), and
  `META-INF/<alias>.RSA`, a detached PKCS#7 SignedData over the `.SF`.
  `node:crypto` signs but does not emit PKCS#7, so `jarsign.ts` writes the
  SignedData DER from a fixed template around the certificate and
  signature. The skeleton's own `META-INF/` (Gradle's debug signature) is
  dropped before signing.

## Patcher work (`packages/cli/src/pack/android/`)

- `proto.ts`: a minimal protobuf wire codec, no dependency. Decodes a
  message into an ordered field list, rewrites the fields touched, copies
  everything else byte-for-byte and recomputes the enclosing lengths, the
  same shape as today's chunk-size fixups. Field numbers from aapt2's
  `Resources.proto` and bundletool's `config.proto` / `files.proto`.
- `bundle.ts` (`patchBundle`), in the order `patchApk` uses:
  - manifest: package, versionName, versionCode. A proto attribute carries
    the string value and a compiled typed value; both are rewritten;
  - label in `resources.pb`, found by resource name (`string/app_name`),
    not by value: the proto table carries names, so the `BASE_LABELS`
    value-matching trap does not apply here;
  - icon slots, same entry names with a `base/` prefix;
  - `base/lib/<abi>/*.so` from each runner APK, plus a generated
    `base/native.pb` with one directory entry per ABI;
  - `BundleConfig.pb`: uncompressed native libs on (right for Play delivery:
    the store compresses the transfer, stored libs save the extracted copy),
    and `assets/app.srtapp` added to the uncompressed globs. Without that
    glob bundletool deflates the payload and the in-place fd boot
    (`packed_asset_location`) fails on the device;
  - `base/assets/app.srtapp`;
  - drop `META-INF/`, JAR-sign.
- `pack/main.ts`: the Android-specific parts (`ANDROID_APP_ID` check, icon
  resolution, versionCode/versionName) move into one helper shared by
  `--apk` and `--aab`.

## Verification

1. `bundletool validate` and `jarsigner -verify` on the output. bundletool is
   Google's standalone jar (github.com/google/bundletool releases), not an
   SDK package; it needs a JRE and bundles its own aapt2. A local testing
   tool only, never a pack-time dependency.
2. `bundletool build-apks --connected-device` then `install-apks` on the
   arm64 device: the same split-and-install path Play takes. Confirm the
   base split stores `assets/app.srtapp` uncompressed and the app boots its
   payload. The emulator is not a signal until
   `backlog/go-client-emulator-launch-crash.md` is fixed.
3. Upload to a Play Console internal-testing track. Upload-key validation on
   the bundle can only be tested with a real upload.

## Out of scope

- Implied features: a CAMERA permission makes Play treat
  `android.hardware.camera` as required and hide the app from devices
  without one. A `<uses-feature required="false">` per permission that
  implies one is the same splice as the permission itself (a second
  attribute, boolean typed) once an app needs it; the go overlay shows the
  pair.
- Play Asset Delivery: the base module is capped at 200 MB of download. A
  payload above that would arrive as a separate split, and
  `packed_asset_location` reads only the base `sourceDir`. Wait for an app
  that needs it.
- Store listing, data-safety form, closed-testing requirement: Play Console
  paperwork, not tooling.
