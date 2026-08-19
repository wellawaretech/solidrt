---
title: CSS color parsing in Rust; drop colord
description: Move CSS color-string parsing from the JS renderer (colord) into alloy (csscolorparser), so color strings cross the FFI raw and one side owns color understanding; parseColor/mixColors/brightness become flux:rendertree bindings backed by the same oklab math the transitions use, and @solidrt/core loses its only runtime npm dependency.
created: 2026-08-19
---

# CSS color parsing in Rust; drop colord

colord backed three core exports (parseColor, mixColors, brightness) and a
per-write parse in the renderer's applyProp. Rust already owns color math
(oklab transition interpolation, gradient stop decoding), which made two
color pipelines with a packed-u32 wire between them. Single ownership:
alloy parses and mixes; JS forwards strings.

- alloy/src/color.rs: parse_css (csscolorparser crate: hex, rgb()/hsl()/
  hwb(), named colors), oklab conversions (moved from transitions.rs),
  mix (oklab), brightness (YIQ luma, colord-compatible).
- flux decode_color accepts a string as well as the packed 0xRRGGBBAA
  number; the transition intercept parses string color targets, so
  `color` transitions work from raw CSS strings.
- flux:rendertree exports parseColor/mixColors/brightness; core re-wraps
  them with the existing signatures. parseColor still returns the packed
  u32 and still throws on an invalid string.
- The renderer's applyProp color branches are gone (color values cross
  untouched; gradients are branded objects the decoder recognizes).
- Behavior notes: mixColors now mixes in oklab (was CIE LAB) - theme
  tones shift imperceptibly; grammar is csscolorparser's (no exotic
  colord plugins were in use).
