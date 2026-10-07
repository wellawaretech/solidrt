---
title: How GPUI renders text, against our text layer
description: Zed's GPUI shapes and rasterizes with each OS's own stack (Core Text, DirectWrite) and with cosmic-text plus swash on Linux and the web, into one bitmap atlas drawn as instanced quads every frame; the same family as our engine, ahead on LCD AA, a proven coverage-to-color recipe and hinting, with no distance fields and different pixels per platform.
created: 2026-10-07
---

# How GPUI renders text, against our text layer

Checked against the Zed source, `main` at c3ab556 (2026-10-06). The
platform code lives in crates of its own: `gpui_macos` with `gpui_apple`
(Metal), `gpui_windows`, `gpui_linux`, `gpui_wgpu`, `gpui_web`.

## Per platform

| Platform | Shaping and layout | Rasterization | Renderer | Default AA |
|---|---|---|---|---|
| macOS | Core Text (`CTLine`); font-kit (Zed's fork) loads and matches fonts | Core Graphics `draw_glyphs` into a bitmap context, unhinted | Metal | grayscale |
| Windows | DirectWrite (`CreateTextLayout`) | DirectWrite `CreateGlyphRunAnalysis`, natural symmetric mode, the font's grid fit | Direct3D 11 | ClearType when the system has it on, else grayscale |
| Linux | cosmic-text 0.19 (fontdb discovery, harfrust shaping) | swash, hinting on | wgpu 29 | LCD subpixel |
| Web | cosmic-text, the browser's canvas text for glyphs the loaded fonts lack | swash, canvas for that fallback | wgpu (WebGPU or WebGL) | - |

## The shared GPU path

- Glyphs are rasterized on the CPU into a texture atlas keyed by
  `RenderGlyphParams`: font, glyph, font size, scale factor, subpixel
  variant, emoji, LCD on or off, dilation level. A new size or scale is a
  new rasterization; there is no distance-field tier.
- Three atlas kinds: `Monochrome` (alpha), `Polychrome` (color emoji),
  `Subpixel` (per-channel coverage).
- Subpixel positioning: `SUBPIXEL_VARIANTS_X = 4`,
  `SUBPIXEL_VARIANTS_Y = 1`. One vertical variant means every baseline is
  rounded to a device pixel row: vertical scrolling in Zed moves text in
  whole device pixels.
- Every glyph is an instanced quad drawn straight into the frame, every
  frame, at its rounded pixel plus its variant. Glyph pixels are never
  resampled, and there is no layer caching.
- Coverage to color on Linux and Windows: the mask shader applies
  DirectWrite's enhanced contrast and gamma alpha correction, ported from
  Windows Terminal's `dwrite.hlsl` (MIT). The contrast boost fades out as
  the text color brightens; the alpha correction then works from the text
  brightness against a set of gamma ratios.
- Coverage to color on macOS: when the user allows font smoothing, GPUI
  reproduces Core Graphics' stem thickening, one of five dilation levels
  chosen from the text color's luminance (lighter text, heavier stems),
  the level part of the atlas key.
- LCD AA blends with dual-source blending (`@blend_src` in
  `shaders_subpixel.wgsl`) against the frame it lands in.
- `TextRenderingMode` (`PlatformDefault`, `Subpixel`, `Grayscale`) is a
  user setting over the platform default.

## Against ours

Our engine (`alloy/src/rendertree/text/glyphs/`, the layer in
`rendertree/text/mod.rs`, the pass in `alloy/src/gpu/glyphs.rs`) is
GPUI's Linux path in most respects: harfrust shaping, swash coverage masks
at horizontal subpixel phases, one shared atlas, instanced quads, coverage
to color in the fragment shader. Three phases against GPUI's four is a
worst-case placement error of 1/6 px against 1/8, not visible.

Where GPUI differs:

- **LCD AA.** The Linux default, per system setting on Windows. It cannot
  ride our text layer: a layer is a transparent texture composited later,
  with no destination to blend per channel against, and dual-source
  blending is an extension on GLES 3.0 (`EXT_blend_func_extended`), not
  core. Only 1x desktop panels would gain; the TV, phones and 2x screens
  would not.
- **Native rasterizers on macOS and Windows**: the platform's own look,
  at the price of different pixels per platform. Ours is swash
  everywhere, identical pixels on every platform, which the headless
  snapshot tests rely on.
- **Hinting** on Linux (swash) and Windows (DirectWrite grid fit); our
  cells are unhinted. Hinting matters at 1x and hardly at 2x.
- **One fixed coverage-to-color recipe**, DirectWrite's, where ours is
  `CoveragePolicy`, a uniform with four modes (`Naive`, `LinearLight`,
  `PolarityRemap`, `PolarityLinear`) and a gamma. The Windows Terminal
  formula is a fifth, battle-tested candidate.
- **Color emoji and system-font fallback** work; ours resolves packaged
  fonts only.
- **No layer**, so none of the layer's traps under motion
  ([text-layer-motion](../backlog/text-layer-motion.md)). GPUI pays the
  glyph pass every frame; the draw bench of 2026-10-06 found that pass
  alone (one instanced draw of 1622 glyph quads) costs nothing visible on
  any of our devices, the armv7 TV included. Drawing glyphs straight into
  the frame is not open to us while Impeller composites it.

What GPUI has no counterpart for: distance-field (MTSDF) cells for text
drawn at arbitrary scale, and transform-scaled text at all. Zed's zoom
changes the font size and lays out again.
