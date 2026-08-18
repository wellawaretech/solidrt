---
title: Decide the shape of the transform props against CSS, all at once
description: scale/scaleX/scaleY, x/y, rotate and originX/originY each landed on their own; CSS gives per-axis tuples for scale and translate but nothing for rotation, so the set should be settled together rather than one prop at a time.
created: 2026-08-14
---

# Decide the shape of the transform props against CSS, all at once

`TransformProps` ([packages/core/src/types.d.ts:165](../../packages/core/src/types.d.ts))
is decoded in [flux/src/alloy_plugins/properties/view.rs](../../flux/src/alloy_plugins/properties/view.rs)
and composed in [alloy/src/rendertree/kinds/view.rs](../../alloy/src/rendertree/kinds/view.rs).
Every prop in it was shaped on its own, and the set no longer answers one
question consistently: what would a CSS author expect to type?

What we have today, against CSS:

| ours | CSS |
| --- | --- |
| `rotate` (scalar, z) | matches - `rotate: 10deg` |
| `scale`, `scaleX`, `scaleY` | `scaleX`/`scaleY` are not CSS properties; the real one is `scale` taking 1-3 values |
| `rotateX`, `rotateY`, `perspective` | bespoke 3D, no single CSS property mirrors it, and more expressive than CSS `rotate` since X and Y combine |
| `x`, `y` | our flattening of `translate: 10px 20px` |
| `originX`, `originY` | CSS has `transform-origin` as ONE property; there are no `-x`/`-y` longhands (`background-position-x`/`-y` is the only weak precedent) |

The tension is real and not resolvable prop-by-prop: CSS gives multi-value
per-axis syntax for `scale` and `translate`, but deliberately gives rotation
none, because rotation is order-dependent and is expressed as axis+angle
(`rotate: y 45deg`) or as separate `transform:` functions. So "make everything
a tuple" is not the answer either.

## Candidate direction (discussed, not decided)

- reshape `scale` to `number | [sx, sy]` and drop `scaleX`/`scaleY` - they are
  unused in the codebase today, and the uniform case stays a bare number
- possibly fold `x`/`y` into a `translate` tuple
- leave `rotate` scalar, and leave `rotateX`/`rotateY`/`perspective` as the
  bespoke 3D escape hatch

Caveat found while discussing it: `scale` is composed arithmetically - Button
multiplies a caller's scale by a press factor - so a `number | tuple` union
forces every consumer to narrow before composing. That is a legitimate reason
`scale` and `origin` could differ in shape: one is a composed factor, the other
is set once and positional.

## Why this exists as an item

`pct()` and the `originX`/`originY` split were added for internal consistency
with the engine's x/y prop convention, and the larger question was deferred at
that moment. This note exists so the origin split is not later mistaken for a
settled answer. Decide the whole set before churning any of it - each of these
is a breaking change for app code, and doing them one release apart spends the
breakage twice.

Source: root TODO.md, migrated 2026-08-14.
