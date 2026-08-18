---
title: Clearing a numeric or transform prop throws instead of resetting it
description: Binding scale/x/y/rotate/radius/strokeWidth to a value that flips back to undefined errors out, because the decoders accept only numbers; null should reset to the property default the way pointerEvents already does.
created: 2026-08-14
---

# Clearing a numeric or transform prop throws instead of resetting it

What it looks like when you hit it: an app binds a style prop through, e.g.
`scale={style()?.scale}` or `radius={style()?.borderRadius}`, animates it, then
clears the style. The prop goes from a number to `undefined`, and the write
fails with "scale must be a number, got null" instead of returning the element
to its default. Any author who animates a transform or radius and then clears
it hits this, not only our own components.

Cause: the shared decoders in
[flux/src/alloy_plugins/properties/mod.rs](../../flux/src/alloy_plugins/properties/mod.rs) -
`f32_of` (:173, used by `scale`, `x`, `y`, `rotate`, `strokeWidth`, and the
geometry props), `decode_color` (:242) and `decode_radius` (:256) - accept a
number and reject everything else, null included.

`pointerEvents` already does the right thing in the same file (:97): null
clears the local override rather than being an error. `decode_params` and
`decode_texture_bindings` also treat null as "clear" (:201, :225). So the
pattern exists; the numeric decoders just never adopted it.

Proposed fix: null resets to the property default - `scale` to 1, `x`/`y`/
`rotate` to 0, `radius` to 0, `strokeWidth` to 0 - in the shared decoders plus
the per-element adapters (view, paint, rectangle, oval, text) that call them. A
non-numeric non-null value stays an error.

Interacts with `backlog/dev-prod-validation-policy.md`: reset-on-null is not
validation relaxation, it is a defined value for a defined input, so it should
behave the same in dev and prod.

## History

Hit while building Button, worked around there by always emitting a numeric
scale (`(props.style?.scale ?? 1) * (s.pressed ? 0.97 : 1)`), which is why
`@solidrt/components` does not trip it today.

Originally filed as "the decoders panic on null", which was true when it was
written: the crash took down the UI thread. The decoders now return `Result`
and the failure surfaces as a JS error instead, so the severity dropped from
crash to throw. The design gap is unchanged.

Source: root TODO.md, migrated and re-verified 2026-08-14.
