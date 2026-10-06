# Typography helpers

`typeStyle(variant)` resolves a theme type-scale role (`caption`/`label`/`body`/`title`/`heading`) to font props ready to spread onto a `<text>` or `d-text`: `fontSize` carries `policy.textScale`, and `fontWeight` carries the low-DPI weight compensation. Reactive when called inside a tracked scope, like any theme/policy read. `Text` applies it for you; reach for the helpers when building custom text out of core primitives.

`typeScaleStyles()` is the type scale as font styles (every role in both polarities' weights, plus the mono family at body size), and `warmTypeScale()` hands them to core's `warmText`, which makes their glyph cells ahead of the first screen on the text engine's worker. `Window` does this on mount and again when the theme or the text policy changes; an app that draws its own window calls it once at startup.

The compensation exists because the renderer rasterizes glyphs unhinted and composites in nonlinear sRGB, which thins light-on-dark text on low-DPI displays as glyphs shrink. `typeWeight(weight, size, onDark?)` adds `policy.textWeightDelta` (0 on high-DPI displays) plus one extra step below 16px; dark-on-light text passes through untouched. `lightOnDark(text, fill)` computes the polarity for a known pair of colors (Button uses it for its fills); omitted, the theme's own palette polarity is used.
