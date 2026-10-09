---
title: How GPUI renders and lays out text, against ours
description: Zed's GPUI shapes and rasterizes with each OS's own stack (Core Text, DirectWrite) and with cosmic-text plus swash on Linux and the web, into one bitmap atlas drawn as instanced quads every frame, and lays out by shaping whole lines; the same rasterizing family as our engine with the same DirectWrite blend, ahead on LCD AA, color emoji and system fallback, with no layer, no distance fields, different pixels per platform, and no primitive for breaking text a line at a time at varying widths.
created: 2026-10-07
---

# How GPUI renders and lays out text, against ours

Rendering checked against the Zed source, `main` at c3ab556 (2026-10-06),
layout against `main` at b01d137 (2026-10-09). The platform code lives in
crates of its own: `gpui_macos` with `gpui_apple` (Metal), `gpui_windows`,
`gpui_linux`, `gpui_wgpu`, `gpui_web`.

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

## Layout

The API is in `gpui/src/text_system.rs`, `text_system/line_layout.rs` and
`text_system/line_wrapper.rs`.

- The unit of shaping is the whole line. `WindowTextSystem::shape_line(text,
  font_size, runs, force_width)` (one line, no newlines) and `layout_line`
  shape a paragraph with its style runs (`TextRun`) through the platform
  shaper in one call. The `LineLayout` is public: `runs` of `ShapedRun {
  font_id, glyphs }`, each `ShapedGlyph { id, position, index, is_emoji }`,
  with `width`, `ascent`, `descent`, and `index_for_x`,
  `closest_index_for_x` and `x_for_index` for hit testing and carets.
- Wrapping: `shape_text(text, font_size, runs, wrap_width, line_clamp)`
  shapes each paragraph as one line and walks its glyph positions
  (`compute_wrap_boundaries`): a break candidate after a space before a
  word character, or at any other non-word character; past `wrap_width`
  the line breaks at the last candidate, else at the current glyph. Greedy,
  exact by construction (the string was shaped whole, so cross-word
  kerning and ligatures are in the positions), one width for every line,
  no cursor.
- `LineWrapper` (`TextSystem::line_wrapper(font, font_size)`), what Zed's
  editor soft wrap uses, sums per-character widths (`width_for_char`, each
  character measured alone, cached in a 128-entry ASCII array and a map for
  the rest): no kerning or ligatures, fit for a monospace editor and
  approximate for proportional text. `wrap_line(fragments, wrap_width,
  indent_adjustment)` yields byte-index boundaries; `truncate_line`
  truncates from the start, the end or the middle (`TruncateFrom`).
- The cache: `LineLayoutCache` holds two frames of shaped and wrapped
  lines. A lookup checks the current frame, then the previous one
  (promoting a hit), else shapes; `finish_frame` swaps and clears, so a
  line unused for one frame is gone. The `*_by_hash` variants key on a
  caller's text hash so a hit never materializes the string; `add_fonts`
  clears it.

## Against ours: rendering

Our engine (`alloy/src/rendertree/text/glyphs/`, the layer in
`rendertree/text/mod.rs`, the pass in `alloy/src/gl/glyphs.rs`) is
GPUI's Linux path in most respects: harfrust shaping, coverage masks at
horizontal subpixel phases (skrifa outlines rasterized by zeno), one shared
atlas, instanced quads, coverage to color in the fragment shader. Three
phases against GPUI's four is a worst-case placement error of 1/6 px
against 1/8, not visible. The coverage recipe is the same as well: our
default, `CoverageMode::DirectWrite`, is the same Windows Terminal port
(gamma 1.8, contrast 1). The other modes stay behind `setTextRendering` as
a live setting, where GPUI's recipe is fixed (DirectWrite's on Linux and
Windows, Core Graphics' dilation on macOS).

Where GPUI differs:

- **LCD AA.** The Linux default, per system setting on Windows. It cannot
  ride our text layer: a layer is a transparent texture composited later,
  with no destination to blend per channel against, and dual-source
  blending is an extension on GLES 3.0 (`EXT_blend_func_extended`), not
  core. Only 1x desktop panels would gain; the TV, phones and 2x screens
  would not.
- **Native rasterizers on macOS and Windows**: the platform's own look,
  at the price of different pixels per platform. Ours is one stack
  everywhere, identical pixels on every platform, which the headless
  snapshot tests rely on.
- **Hinting by platform**: on Linux (swash) and Windows (DirectWrite grid
  fit), not on macOS. Ours forces skrifa's autohinter on below 2x (light:
  rows snapped, x left to the phases), with full (the mono target, stems
  on whole pixels) per app or per `flux:font` atlas. swash's hint flag
  does nothing on an uninstructed variable font like the shipped Notos
  (its default engine sends a TrueType font with a non-empty `prep` to the
  interpreter, which has nothing to run), so how much GPUI's Linux path
  hints depends on the font.
- **Color emoji and system-font fallback** work; ours resolves registered
  fonts only.
- **No layer.** GPUI draws every glyph into the frame every frame at its
  rounded pixel. Our layer's blur at fractional offsets is gone since the
  paint walk snaps every box to the device grid
  ([pixel-snapped-paint-boxes](../done/pixel-snapped-paint-boxes.md)), the
  same rounding GPUI does vertically; the scale animation trap (a layer
  remade at each intermediate size, glyphs missing on the TV until the
  worker catches up) is open
  ([text-layer-motion](../backlog/text-layer-motion.md)). The draw bench
  of 2026-10-06 found the glyph pass alone (one instanced draw of 1622
  glyph quads) costs nothing visible on any of our devices, the armv7 TV
  included. Drawing glyphs straight into the frame is not open to us while
  Impeller composites it.

What GPUI has no counterpart for: distance-field (MTSDF) cells for text
drawn at arbitrary scale, and transform-scaled text at all. Zed's zoom
changes the font size and lays out again.

## Against ours: layout

Ours is the pretext split
([text-layout-owned](../done/text-layout-owned.md),
[text-layout-primitives](../done/text-layout-primitives.md)): a text is
segmented into wrap units (UAX 14), each shaped once through a word cache
shared by every text, and line breaking is arithmetic over the cached
advances and ink widths. Apps get it as plain data: `prepareText(text, {
runs, carets })` returns the units (`advance`, `width`, `ascent`,
`descent`, `hardBreak`, `glue`, `run`, optional kerned `carets`), and
`layoutNextLine(prepared, cursor, width)` in `@solidrt/core` breaks one
line from a cursor at a width of its own; `examples/text-flow` flows two
columns with cursor handoff around a moving circle and a drop cap.
`<text>` runs on the same arithmetic (floats, `textIndent`, atoms,
`textWrap` balance and pretty, `maxLines` with ellipsis), and
`flux:font`'s `prepareText` reports glyph ids for its own atlas.

- **Lines of varying width** are a primitive here and app work there.
  GPUI's wrap takes one width for every line and has no cursor, so text
  around an obstacle or across columns means a breaker of one's own,
  re-shaping candidate substrings with `shape_line` or summing
  `LineWrapper`'s per-character widths, which see no kerning. Ours breaks
  on the widths the drawn text has.
- **Line choice beyond greedy**: `<text>`'s `textWrap` balance and pretty
  are extra passes over the same cached widths; GPUI's wrap is greedy.
- **The caches differ in grain and life**: a word shaped anywhere here is
  a hit in any later layout, in any frame; GPUI keeps whole lines for two
  frames, so the same paragraph reflowed into another shape is another
  line.
- **Exactness**: GPUI shapes the paragraph as one string, so its positions
  are exact by construction. Ours shape per unit and draw a line's
  same-style run joined (`Text::line_runs`), relying on the units'
  advances summing to the joined shape: the corpus baseline holds it,
  nothing guarantees it (kerning against a space, a ligature across one).
- **Glyph-level access**: GPUI exposes glyph ids and positions for any
  line; the `<text>` path gives caret positions and no glyph ids (only
  `flux:font` does, for its own atlas).
- **Truncation**: GPUI truncates from the start, the middle or the end;
  `<text>` ellipsizes at the end of the last line (`maxLines`,
  `textOverflow`).
