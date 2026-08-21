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

## Resolution (2026-08-21)

Implemented, and widened beyond the proposal on review: null resets EVERY
prop class that has an unset state, not just the numeric decoders, because
the rendertree already modeled "unset" as `Option` fields and a plugin-side
default could not express some resets at all (there is no number that means
"w = fill the box" on a d-rect).

- The alloy setters for numeric/transform/geometry props take `Option<f32>`
  (`Option<[f32; 4]>` for radius/clipRadius, `Option<OriginCoord>` for
  origin, `Option<(f32, f32)>` for viewBox): None restores the unset state,
  so `needs_matrix()` and the translation-only bounds path come back too.
  Non-Option-backed fields (strokeWidth/strokeMiter, fontSize/lineHeight/
  maxLines/textIndent) reset to consts shared with their `Default` impls.
  Span numeric overrides clear to inherit-from-paragraph.
- The plugin decoders grew `opt_f32`/`opt_radius` (null -> None, same error
  otherwise); `f32_of` stays for required interior numbers (gradient fields,
  viewBox entries, shader.outset).
- Layout props reset too: null restores the field from the KIND's initial
  style (`Element::initial_style`, extracted from each kind's `with_layout`),
  so a view's `flexDirection` goes back to column and a rect's `display` to
  block - taffy's defaults would have been wrong. `position` null resets to
  relative through `set_position`, keeping the positioning-context flag
  consistent.
- Native transitions needed no change: a null write never parses as a track
  target, so `transition_write` cancels the running track and the write
  falls through to the (now resetting) normal path - clear-during-animation
  snaps to the default.
- Documented in docs/20-core/30-reference/index.md ("Clearing a prop");
  tests in flux/src/tests/properties.rs (null_resets_props_to_defaults) and
  alloy/src/tests/view.rs (none_resets_transform_props_to_unset).

Extended same-day on review to the enum props and `color`, so the rule is
uniform: every styling prop resets on null. Enum setters take
`Option<Enum>` (None -> the Default value); the plugin wraps their decodes
in a shared `opt` combinator. `color` null resets the whole fill (solid +
gradient) to the default paint on shapes and text, and on a span drops the
paint OVERRIDE (`Span::clear_paint_override`) so the run inherits the
paragraph's color - resetting the override to the default paint would have
pinned gray, which is why color was initially deferred. fontWeight resets
too (numeric on the JS surface; DEFAULT_FONT_WEIGHT const shared with
Default).

Still required, deliberately: the content props (`text` on a span, `d` on a
path) and interior fields of composite objects (gradient stops, shader
fields) - null there is not a styling reset. Window `title`/`fullscreen`
also keep requiring values.
