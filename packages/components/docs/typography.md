# Typography helpers

`typeStyle(variant)` resolves a theme type-scale role (`caption`/`label`/`body`/`title`/`heading`) to font props ready to spread onto a `<text>` or `d-text`: `fontSize` carries `policy.textScale`, `fontWeight` is the role's own. Reactive when called inside a tracked scope, like any theme/policy read. `Text` applies it for you; reach for the helpers when building custom text out of core primitives.

`typeScaleStyles()` is the type scale as font styles (every role, plus the mono family at body size), and `warmTypeScale()` hands them to core's `warmText`, which makes their glyph cells ahead of the first screen on the text engine's worker. `Window` does this on mount and again when the theme or the text policy changes; an app that draws its own window calls it once at startup.

There is no weight compensation: the text engine blends glyph coverage with DirectWrite's gamma-aware recipe, so light-on-dark text keeps its stems at every display scale.
