---
title: Colour the Android splash from the project's iconBackground
description: A packed app's API 31+ splash shows on the runner theme's black windowBackground whatever the app looks like; give the theme a splash colour resource that pack patches from "solidrt": "iconBackground", so the splash, the icon and the first frame read as one surface.
created: 2026-11-01
completed: 2026-11-01
---

# Colour the Android splash from the project's iconBackground

What it looks like: a packed app launched on Android 12+ shows its icon on
a black splash, then its own first frame, whatever colour that frame is.
Since [[android-launch-home-flash]] the splash stays up until that first
frame, so the black-to-app change is the whole hand-over.

On API 31+ the splash takes the theme's `windowBackground` when that is
one colour; the runner's `AppTheme` (`Theme.NoTitleBar.Fullscreen`) gives
black. Pack swaps the adaptive icon's background as a PNG pixel
(`patchIcon` in packages/cli/src/pack/android/apk.ts), which a theme
attribute cannot use: `android:windowSplashScreenBackground` takes a
colour.

## Done looks like

- A packed app's splash background is its `iconBackground`, on API 31+.
- Below API 31 the legacy starting window shows the same colour, or stays
  as it is if that costs a second resource path (decide when building).
- Verified by recording a launch from the app drawer.

## What it involves

A `values-v31` style with `android:windowSplashScreenBackground` naming a
colour resource in the runner, and pack patching that colour's value in
`resources.arsc`, next to `patchLabel`.

## Outcome

Built as shaped. The runner's `values-v31` `AppTheme` names
`@color/splash_background`, a placeholder (`#c39d6f`) that `patchSplash`
(packages/cli/src/pack/android/splash.ts) finds by value in
`resources.arsc` and sets to `iconBackground`; pack fails on a runner
without it. Below API 31 the starting window stays as it was, by decision.
Verified on an SM-T500 (Android 12, One UI): from the app drawer, the
icon shows on its own ground and the splash fades into the first frame.
