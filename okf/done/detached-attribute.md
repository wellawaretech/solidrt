---
title: Replace the d-* tags with a set-once detached attribute
description: The seven d-* intrinsics (d-view, d-rect, d-oval, d-line, d-path, d-texture, d-text) double the tag set for one bit that never changes after creation; a literal `detached` attribute on the plain tag (`<view detached>`) says the same thing, and the bundler's JSX pass resolves it to the detached form at compile time, so the runtime and the native tree do not change.
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

## Why it is resolved at compile time

The first shape read `props.detached` in `createElement`, because the
universal JSX template passes static props there inline. That holds only for
an element with no spread. Any spread on the element, whatever it carries,
makes the compiler stop inlining:

```js
// <path detached {...draw} />     draw carries d, fill, ...: no detached
var el = createElement("path")     // built laid-out, no props
setProp(el, "detached", true)      // the literal arrives late
spread(el, draw, false)
```

Static children are inserted before that `setProp` as well, and a `ref`
fires before it. The form is common: about 15 sites today, among them the
documented SVG idiom `<d-path {...draw} />` (`packages/core/src/svg.ts`) and
`<d-text {...font()}>` in the components text fields.

The tag name is the one thing that always reaches `createElement`, so the
bit travels in it. A visitor in the bundler's JSX pass rewrites a literal
`detached` to the `d-*` tag before the solid plugin runs. `d-*` stops being
something an author writes and stays as the wire kind between the compiled
output and `Element::from_kind`.

## Shape

- Compiler (`solidPlugin` in `packages/cli/src/bundle/bundler.ts`, the one
  place the solid plugin is configured): a Babel plugin listed ahead of the
  solid plugin, traversing from `Program.enter` so it sees the JSX before
  the solid transform does. On a paired tag:
  - `detached` or `detached={true}`: the tag becomes `d-<tag>` and the
    attribute is dropped.
  - `detached={false}`: the attribute is dropped.
  - any other value: a compile error at the attribute, "Detached is set
    once, as a literal attribute".

  On another host tag (`window`, `span`) it is a compile error, "<span> has
  no detached form". On a component `detached` is an ordinary prop and is
  left alone.
- Runtime: `createElement`, `create_node` and `from_kind` do not change.
  Code with no JSX (the error window in `packages/core/src/renderer.ts`)
  keeps `createElement("d-rect", ...)`. One addition: `applyProp` throws on
  the name `detached`, "Detached is set once, as a literal attribute at the
  tag". Reaching it means a spread carried the name; today that is an
  unknown-property warning.
- Messages name the element as the author wrote it, `<view detached>` and
  never `<d-view>`: the nesting error (`alloy/src/rendertree/tree/mod.rs`),
  the "Detached-only property" rejection
  (`flux/src/alloy_plugins/properties/mod.rs`), and the renderer warnings
  that print `<${elementType}>` (rejected property, leak sentinel).
- Inspect and MCP: nothing to do. `tree/inspect.rs` already reports the
  plain kind name and `detached: bool`.
- Types (`packages/core/jsx-runtime.d.ts`): each paired tag becomes a union
  discriminated on the literal. The laid-out branch also declares the
  geometry-only names as `?: never`:
  `rect: (RectProps & LayoutProps & { detached?: false } & Never<GeometryProps>) | (RectProps & GeometryProps & { detached: true })`
  with `type Never<T> = { [K in keyof T]?: never }`. `view` needs no guard
  (`ViewOwnProps` is a subset of `ViewProps`). Name the branches (a
  `RectLaidOut`/`RectDetached` pair per tag) so an error prints the alias
  and not the expanded union.
- The nesting rule stays as it is (a detached subtree is entirely detached).
  The error names the attribute: `<view> cannot be a child of <view
  detached>`.
- Migration: 306 `<d-*` tags in 138 files across the repo (examples,
  packages, apps, tests, docs, AGENTS.md files; about 20 of those files are
  `okf/` records, and `done/` stays as written). A mechanical rewrite, the
  spread sites included. The prose that teaches the tags moves with it:
  `docs/`, the AGENTS.md and `agents/` files, the scaffold, the doc comments
  in `types.d.ts`. Rust keeps its `"d-*"` kind strings (47 in alloy tests
  and examples): they are the wire name.

## Findings (2026-11-10)

- Prototype of the pass, run against the solid plugin: `<path detached
  {...draw} />` compiles to `createElement("d-path")` plus the spread;
  `<view {...p} detached>` with static children builds the `d-view` before
  its children are inserted; `<Card detached>` keeps the prop; a dynamic
  value and `<span detached>` fail with a code frame.
- The plain union (no `never` guard) accepts `<rect x={3} color="red" />`
  and `<line x1={0} y1={0} color="red" />`, which are errors today: with no
  discriminant written, the excess-property check accepts a name found in
  any member. With the guard the error reads "Property 'detached' is
  missing ... but required in type '{ detached: true }'", which says what to
  do. No geometry name collides with a laid-out name on any of the six
  guarded tags, so the guard hides nothing.
- The union cannot reject a non-literal value: `detached={flag}` with a
  `boolean` typechecks (TypeScript decomposes the boolean across the
  discriminant). The compile error is what enforces the literal.

## Decisions (2026-11-10)

- Compile-time rewrite, not a runtime read. Rejected: "detached must be the
  first attribute", accepting the late `setProp`. It puts `detached` ahead
  of the other props but not ahead of the static children or the `ref`, so
  it needs a native conversion of a node that already holds children, a
  touched flag on every proxy node, and a positional rule that only matters
  when the element has a spread.
- The union type is fine with the `never` guard: the messages stay readable
  for a union discriminated on a literal `detached`.
- `detached` inside a spread (`<view {...props}>` with `props.detached`)
  cannot work and should not: it throws at `applyProp`. The attribute is
  written at the tag.
- The `d-*` tags stay for a while. They are the wire form, so `<d-view>`
  written in source keeps compiling at no cost; their `IntrinsicElements`
  entries alias the detached branch of the union and are marked deprecated.
  New code, docs and AGENTS.md use `detached`. Removing them is a later
  item, once nothing in the repo writes them: delete the entries and make
  the pass reject a `d-*` tag in source.
- Not in this item: `create_node(id, kind, detached: bool)` in place of the
  `d-` kind strings, which would drop the seven `d-` arms of `from_kind`.
  It changes no behavior and can follow on its own.

## Related

- [detached-nesting-static-check](../backlog/detached-nesting-static-check.md): the
  bundler-pass check belongs in the same visitor. It keys on the literal
  `detached` attribute (or a `d-*` tag while those remain), which the
  compile error guarantees is a literal at the tag.
