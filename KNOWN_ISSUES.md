# Known Issues

## Impeller font rendering is poor at low DPI and small sizes on dark backgrounds

Impeller uses grayscale-only antialiasing with no subpixel rendering support. On desktop monitors running at 1x scale (no HiDPI), small fonts (roughly 14px and below) have hairline strokes that are difficult to read, particularly on dark backgrounds where the thin grey antialiased edge bleeds into the background.

Mobile is unaffected because device pixel ratios of 2-3x mean the same logical font size is rasterized at 2-3x the physical pixels, producing visibly thicker strokes.

**Current workaround:** `font_weight` defaults to `Bold` in `Text::default()` to ensure readable stroke width on 1x desktop displays. The `fontWeight` TSX prop overrides this per element.

**Proper fix:** make the default font weight DPI-aware - use `Regular` when `display_scale >= 2.0`, heavier weights at lower scales. This requires passing `display_scale` into `PlatformContext` and reading it at text build time.

## Impeller fills paths as bounding rectangle for curves and self-intersecting polygons

`impellers::DisplayListBuilder::draw_path()` with `DrawStyle::Fill` renders the path's bounding rectangle instead of its interior when the path contains:

- Bezier curve segments (`C` cubic, `Q` quadratic, and their derived `S`/`T`/`A` SVG forms after arc-to-cubic conversion), or
- Self-intersecting line segments (e.g. a pentagram `M...L...L...Z`)

Simple non-self-intersecting polygons made of only `M`/`L`/`Z` fill correctly. Stroked rendering (`DrawStyle::Stroke`) is unaffected and works for all path shapes. The Impeller version is irrelevant: both `impellers` crate `0.4.1` and the unreleased git master point at the same prebuilt binary `a_0.5.12` (the `STATIC_MAJOR/MINOR/PATCH` constants in `build.rs` are identical across versions), so bumping the crate has no runtime effect.

The bug is in the prebuilt Flutter Impeller binary itself, under the OpenGL ES backend that `alloy` uses. Multiple historical Flutter issues describe similar symptoms in Impeller's filled-path tessellator:

- [flutter/flutter#126212](https://github.com/flutter/flutter/issues/126212) - Complex paths cause rendering errors (closed, but the symptom pattern persists in newer code)
- [flutter/flutter#136504](https://github.com/flutter/flutter/issues/136504) - Optimization renders path as rectangle
- [flutter/flutter#177873](https://github.com/flutter/flutter/issues/177873) - SVG/VG rendering broken on Impeller-OpenGL ES (open)

**Workaround:** In `lattice/src/rendertree/kinds/path.rs`, flatten curve segments to short line segments using `lyon` before submitting them to `impellers::PathBuilder`. Impeller then only sees `move_to` + many `line_to` + `close`, which fills correctly. This handles curves but does not fix self-intersecting polygons; even-odd fill on a self-intersecting path remains broken.

Real-world SVG icon libraries (Heroicons, Lucide, Tabler, Phosphor, Material Icons, Font Awesome, etc.) effectively never use self-intersecting fills, so the flattening workaround covers tier-2 icon support.