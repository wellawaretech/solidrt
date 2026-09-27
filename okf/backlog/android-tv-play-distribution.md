---
title: Android TV on Google Play
description: Play offers an app on TVs only after a form-factor opt-in and a TV quality review, not by app category, and every packed app already declares TV support; what fails that review today is the banner (the runner's SolidRT logo, without the app's name, on every packed app) plus the bundle, 32/64-bit and 16 KB rules the AAB work has to meet.
created: 2026-09-27
---

# Android TV on Google Play

Packed apps run on Android TV today by sideloading. Reaching TVs through Play
is a separate gate from reaching phones, and unlike Android Auto
(`okf/backlog/android-auto-parked-games.md`) no app category is involved: any
app may be a TV app. Three things decide it:

- The manifest declares TV support. The runner does, for every packed app:
  the `LEANBACK_LAUNCHER` intent filter, `android.software.leanback` and
  `android.hardware.touchscreen` not required, and `android:banner`.
- The developer opts the app in to the Android TV form factor in Play
  Console (Setup > Advanced settings > Form factors), on the same package
  and bundle as the phone app, or on a dedicated TV track.
- Play reviews it against the TV app quality guidelines. Only an approved
  app is discoverable on TVs.

Because the opt-in is the gate, the TV entries cost nothing for an app that
never targets TV. The runner keeps declaring them for every app, and
package.json gets no `tv` key.

Done looks like: a packed app shows its own banner, with its name, on the TV
home screen; a game sits in the launcher's Games row; its bundle installs
from Play on a TV with a 32-bit userspace and on an arm64 one; and it passes
the TV review.

## What fails review today

- **The banner (TV-LB, TV-BN).** The runner's `android:banner` is
  `drawable-xhdpi/tv_banner.png`, the SolidRT logo with no name, and
  `srt pack` patches the launcher icon's slots (`patchIcon` in
  `packages/cli/src/pack/android/apk.ts`) but not the banner. Every packed
  app shows SolidRT's logo on the TV home screen, and TV-BN requires the
  app's name in its banner. The fix follows the icon: the prod overlay
  points `android:banner` at a patchable slot (the go client keeps the
  SolidRT banner), and pack swaps its bytes for the app's PNG, 320x180 at
  xhdpi. Config: a top-level `banner` next to `icon`, since it is an app
  identity asset like the icon (a future tvOS packer wants one too), null
  meaning the placeholder. Without one, pack says so, the way it reports
  the icon.
- **The camera (TV-MT).** A packed app with the camera capability on (the
  default) is filtered off every device without a camera, TVs included:
  Play infers the camera as required from the permission. Filed in
  `okf/tiny.md` (Runtime).
- **The bundle (TV-G1).** App bundles are mandatory for TV apps:
  `okf/backlog/play-store-aab.md`.
- **32/64-bit and 16 KB pages (TV-G6).** In force since 1 August 2026. The
  AAB item's default `android.abis` (`["arm64-v8a", "armeabi-v7a"]`) meets
  the first half. The second needs every shipped runner library 16 KB
  aligned; SDL was the weak point, so check the released libs with
  `readelf -lW` before the first TV submission.
- **The Games row (TV-LG).** A game shows there through
  `android:appCategory="game"`: the `category: "game"` key and patcher edit
  the Android Auto item proposes. Not a failure for a non-game.

## Runtime checks for the review

- TV-DP: everything navigable with a five-way D-pad. That is the app's work
  on top of focus navigation, but a remote whose select button SDL's
  auto-mapping drops fails it regardless:
  `okf/backlog/remote-control-input-device.md`.
- TV-DB: back at the app's root leads to the TV home screen. The runtime
  backgrounds the task at the root (`okf/done/exit-to-launcher.md`); confirm
  it on a packed app, not only the go client.
- TV-PC / TV-PP, for video and music apps: D-pad center and the play/pause
  key toggle playback. The key reaches the app as `MediaPlayPause`
  (`alloy/src/keymap.rs`); the wiring is the app's, or a player
  component's.
- TV-NP (system media controls for audio that keeps playing after home)
  does not apply: SDL pauses audio when the activity pauses.
- Already met: TV-PS (`minSdk` 26, the rule is 31 or lower), and TV-MT once
  the camera line is done.

## Play Console

At least one TV screenshot, a TV banner graphic in the store listing, and
"Android TV" mentioned in the description; then the form-factor opt-in. The
review status shows under Android TV on the app's Pricing and distribution
page.

## Open questions

- Whether pack should derive a banner when none is given (icon on
  `iconBackground` plus the display name). Rendering the name needs a text
  rasterizer at pack time, which pack does not have.

## References

- [Distribute to Android TV](https://developer.android.com/training/tv/publishing/distribute)
- [TV app quality guidelines](https://developer.android.com/docs/quality-guidelines/tv-app-quality)
- [64-bit app compatibility for Google TV and Android TV](https://android-developers.googleblog.com/2025/08/64-bit-app-compatibility-for-google-tv-android-tv.html)
