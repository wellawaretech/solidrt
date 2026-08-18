---
title: Name the flux plugin layers after what they marshal
description: plugins/modules/ and plugins/gui/ were named for their JS-facing shape while standards/ was named for its contract, so placement was a judgment call; the layers are now crate-level siblings standards_plugins/, forge_plugins/ and alloy_plugins/, which makes it mechanical.
created: 2026-08-18
completed: 2026-08-18
---

# Name the flux plugin layers after what they marshal

`flux/src/plugins/` had three directories whose names came from two different
axes:

- `standards/` - named for the JS contract (a web standard).
- `modules/` - named for the JS shape (a `flux:*` import).
- `gui/` - named for the domain, and gated by the `gui` cargo feature.

Deciding where a new plugin went therefore needed a judgment call, and the
rule ("put new flux-specific modules in `modules/`") answered it by default
rather than by principle. In fact `gui/` was never "the GUI": `flux:audio`,
`flux:camera` and `flux:microphone` lived there and are `flux:*` modules like
everything in `modules/`; the only thing separating them was that they need
alloy. The backing was close to total: every file in `modules/` reached into
forge and every file in `gui/` into alloy.

## What was done

The layers moved up to crate level and are named for the crate they marshal,
with a `_plugins` suffix so the module names do not collide with the `forge`
and `alloy` crates (a `plugins/forge/` submodule would shadow the crate inside
`plugins/mod.rs`, so `forge::X` there would resolve to the sibling):

```
plugins/standards/  ->  standards_plugins/
plugins/modules/    ->  forge_plugins/
plugins/gui/        ->  alloy_plugins/
plugins/            stays: the shared toolkit (js_error, marshal, value,
                    seekable) and mod.rs (init_context, layer registration)
```

The rule it buys:

1. Is the JS surface a web standard? `standards_plugins/`, whatever backs it.
2. Otherwise, the crate it marshals: `forge_plugins/` or `alloy_plugins/`.

Step 1 is deliberate: `standards/` is genuinely on the other axis and is not
forge-free (`fetch`, `request`, `body`, `websocket` marshal forge cores). A
pure "file it under its backing crate" rule would move `fetch` into
`forge_plugins/`, which is wrong: a standard's identity is its contract, not
its implementation.

## Deliberately unchanged

- **The `gui` cargo feature keeps its name**, and the public path stays
  `flux::gui` (`pub use alloy_plugins as gui;`): the feature gates a
  capability set, lattice and the packaging name it, and pairing the public
  module with the feature name is right. Lattice was not touched.
- **`packages/flux-types` does not mirror the rename.** Its `modules/`,
  `standards/` and `gui/` directories describe the JS surface a reader sees
  (an import, a global, the GUI) and the website generates the Runtime
  reference groups from them. Reader-facing where the plugin directories are
  maintainer-facing; flux/CLAUDE.md says so.
- `docs/_old/` and `okf/done/` keep the old paths as history.
