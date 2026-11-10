---
title: Statically detect layout elements inside detached views
description: A <view> nested inside a <view detached> typechecks and fails only at runtime, and TypeScript cannot catch it (every JSX expression is the one Element type, so children cannot be constrained per tag); the place to prevent this coding error is the bundler's Babel pass that resolves the detached attribute, where tags and the literal attribute are static.
created: 2026-08-02
---

# Statically detect layout elements inside detached views

Source: the animated-explainer demo feedback (2026-08-02), which assumed
the types could enforce it since ViewProps and ViewOwnProps are already
distinct. Checked 2026-08-02: they cannot. TypeScript gives every JSX
expression the single JSX.Element type (jsx-runtime.d.ts maps all tags to
one CoreElement), per-tag element types are not expressible, so a detached
view's children prop can never reject a `<view>` at the type level.

Where it IS decidable: the bundler compiles the JSX, and tags are static
there. The check belongs in the same Babel pass that resolves the
attribute (`detachedAttribute` in `packages/cli/src/bundle/detached.ts`).
There a literal `detached` at the tag (or a deprecated `d-*` tag)
identifies a detached parent; the pass already rejects any other value, so
the attribute is always a literal. The check errors on a layout intrinsic
(`view`, `rect`, `text`, ... with no `detached`) directly nested under
such a parent - the common case, caught at compile time with a real file
and line.

The gap: a component boundary hides the tag. A component returning
`<view>` used inside a `<view detached>` escapes any static check, so the
runtime error stays the backstop for composed trees - the static check
narrows the class, it cannot close it.

The attribute it keys on: [detached-attribute](../done/detached-attribute.md).
