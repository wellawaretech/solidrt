---
title: OS reduce-motion preference is ignored
description: The components' built-in motion is gated on policy.motion, but nothing reads the OS "reduce motion" accessibility setting, so users who asked their system for less motion still get springs and slides unless the app calls setPolicy by hand.
tags: [components, policy, accessibility, alloy]
created: 2026-09-02
---

# OS reduce-motion preference is ignored

## Symptom

A user with the OS reduce-motion accessibility setting enabled still sees
the components' full motion (press springs, knob/indicator travel, popup
fades). `policy.motion` exists and the components honor it, but its
resolver always answers "normal": no platform fact feeds it.

## What done looks like

`defaultPolicyResolver` answers `motion: "reduced"` when the OS preference
is set, live on change like `systemTheme`. Apps keep the override path
(`setPolicy({ motion })`) for their own toggles.

## What it involves

- Alloy reports the fact. SDL has no API for it, so per platform:
  GTK/portal setting on Linux,
  `NSWorkspace.accessibilityDisplayShouldReduceMotion` on macOS,
  `SPI_GETCLIENTAREAANIMATION` on Windows,
  `Settings.Global ANIMATOR_DURATION_SCALE` on Android,
  `UIAccessibility.isReduceMotionEnabled` on iOS.
- Core exposes it next to `systemTheme` in environment.ts.
- The components' policy resolver consumes it; the component layer already
  degrades under "reduced" (fades kept, travel/scale snapped - see
  motion.tsx), so no component work remains.
