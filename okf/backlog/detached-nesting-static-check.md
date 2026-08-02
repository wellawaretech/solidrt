---
type: backlog-item
title: Statically detect layout elements inside detached views
description: A <view> nested inside a <d-view> typechecks and fails only at runtime, and TypeScript cannot catch it (every JSX expression is the one Element type, so children cannot be constrained per tag); the place to prevent this coding error is the bundler's JSX pass, where tags are static.
status: open
timestamp: 2026-08-02T00:00:00Z
---

# Statically detect layout elements inside detached views

Source: the animated-explainer demo feedback (2026-08-02), which assumed
the types could enforce it since ViewProps and ViewOwnProps are already
distinct. Checked 2026-08-02: they cannot. TypeScript gives every JSX
expression the single JSX.Element type (jsx-runtime.d.ts maps all tags to
one CoreElement), per-tag element types are not expressible, so a d-view's
children prop can never reject a `<view>` at the type level.

Where it IS decidable: the bundler compiles the JSX, and tags are static
there. A check in the JSX transform (or an `srt check` companion pass) can
error on a layout intrinsic (`view`, `rect`, `text`, ...) directly nested
under a `d-*` intrinsic - the common case, caught at compile time with a
real file and line.

The gap: a component boundary hides the tag. A component returning
`<view>` used inside a `<d-view>` escapes any static check, so the runtime
error stays the backstop for composed trees - the static check narrows the
class, it cannot close it.
