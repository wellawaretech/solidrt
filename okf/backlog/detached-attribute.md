---
title: Replace the d-* tags with a set-once detached attribute
description: The seven d-* intrinsics (d-view, d-rect, d-oval, d-line, d-path, d-texture, d-text) double the tag set for one bit that never changes after creation; a static `detached` attribute on the plain tag (`<view detached>`) says the same thing, and createElement already receives static props inline, so the bit is known when the native node is built.
created: 2026-11-10
---

# Replace the d-* tags with a set-once detached attribute

## Problem

Every element with a layout form has a second tag for its detached form:
`view`/`d-view`, `rect`/`d-rect`, `oval`/`d-oval`, `line`/`d-line`,
`path`/`d-path`, `texture`/`d-texture`, `text`/`d-text`. The pair differs in
one bit: whether the node has a Taffy node (`with_layout()` vs
`no_layout()` in `Element::from_kind`, `alloy/src/rendertree/mod.rs`). The
bit is fixed for the node's life. Seven extra intrinsics, a `d-` prefix that
has to be explained ("detached from the layout engine"), and a tag rename
whenever an element moves between the two worlds.

`<view detached>` states the bit as what it is: a property of the element,
set once.

## Why it is cheap at the creation point

The universal JSX template passes static props to `createElement` as its
second argument (`packages/core/src/renderer.ts`, `createElement`), before
`tree.createNode` returns. So a literal `detached` is known at the moment the
native node is built, and the native side never has to convert a node
between the two forms.

## Shape

- JS: `createElement(type, props)` reads `props.detached` and creates the
  detached form; `applyProp` skips the name there. `detached` arriving by
  any later path (a dynamic `detached={x()}`, a spread, a `setProp`) throws:
  "detached is set once, as a static attribute". That is the whole set-once
  contract, enforced in one place.
- Native: `create_node(id, kind, detached: bool)` instead of the `d-` kind
  strings, so `from_kind` loses its seven `d-` arms. The render tree already
  reports `detached: bool` in inspect (`tree/inspect.rs`); MCP output keeps
  that field and the kind becomes the plain name.
- Types (`packages/core/jsx-runtime.d.ts`): each paired tag becomes a union
  discriminated on the literal, e.g.
  `view: (ViewProps & { detached?: false }) | (ViewOwnProps & { detached: true })`,
  so the layout-only and geometry-only props stay where they are today
  (`x1`/`y1` only on a detached line, flex props only on a laid-out one).
- The nesting rule stays as it is (a detached subtree is entirely detached).
  The error names the attribute: `<view> cannot be a child of <view
  detached>`.
- Migration: about 300 `<d-*` occurrences in about 130 files (examples,
  packages, tests, docs, AGENTS.md files). A mechanical rewrite.

## Decisions (2026-11-10)

- The union type is fine: an earlier test (on another machine) showed
  TypeScript keeps the error messages readable for a union discriminated on
  a literal `detached`.
- `detached` through a spread (`<view {...props}>`) cannot work and should
  not: the spread reaches the node after creation, so it throws like any
  other late `detached`. The attribute is written at the tag.
- The `d-*` tags stay for a while as aliases: `createElement("d-view", p)`
  is `createElement("view", { ...p, detached: true })`, and the `d-*`
  entries in `IntrinsicElements` alias the detached branch of the union.
  New code, docs and AGENTS.md use `detached`; removing the aliases is a
  later item, once nothing in the repo uses them.

## Related

- [detached-nesting-static-check](detached-nesting-static-check.md): the
  bundler-pass check keys on the static `d-*` tag today; with this change it
  keys on the static `detached` attribute instead, which the set-once rule
  guarantees is a literal at the tag.
