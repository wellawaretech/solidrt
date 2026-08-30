---
title: flattenArray drops needsUnwrap when a fragment follows an accessor
description: In @solidjs/signals flattenArray assigns the nested call's result to needsUnwrap instead of OR-ing it, so an accessor followed by a function-free array child is returned raw; the DOM renderer tolerates it, a universal renderer passes the memo to insertNode and crashes.
project: "@solidjs/signals (github.com/solidjs/solid, packages/signals)"
versions: 2.0.0-rc.3 and rc.4; same line on `next` at d6a4a52f (2026-08-30)
status: filed
link: https://github.com/solidjs/solid/issues/3133
created: 2026-08-30
---

# flattenArray drops needsUnwrap when a fragment follows an accessor

Found 2026-08-30 in external feedback on a dashboard demo (the 54 project):
every 20-45 s the app died with

```
TypeError: Error converting from js 'undefined' into type 'f64'
    at insertNode (@solidrt/core/src/renderer.ts:275)
    at reconcileArrays (@solidjs/universal/dist/dev.js:148)
```

because `insertNode` received a bound memo accessor instead of a proxy node,
and `node.id` crossed the FFI as `undefined`. The reporter traced it to
`flattenArray` but could not name the exact input; the trip condition is
below.

## The defect

`packages/signals/src/boundaries.ts`, the array branch of `flattenArray`
(rc.4 dist: `@solidjs/signals/dist/dev.js:11509`):

```ts
if (Array.isArray(child)) {
  needsUnwrap = flattenArray(child, results, options);   // assigns, does not OR
}
```

Under `doNotUnwrap` a function child sets the flag and is pushed raw; a
later array child at the same level that holds no functions (a fragment)
returns `false` and overwrites it. `flatten` then returns the plain results
array with the accessor still inside instead of the resolving wrapper.

Trip condition, exactly: in one children array, an accessor (`<For>`,
`<Repeat>`, any memo) followed later at the same level by a function-free
array. Order matters: the LAST child decides the final flag, so
`[For, Chip-fragment]` trips and `[Chip-fragment, For]` does not. That is
why the crash looked intermittent to the reporter: which panel mounts, and
which branch of a fragment exists at that moment, changes the shape.

`@solidjs/web` is immune (its `insertExpression` has a function branch).
`@solidjs/universal`'s does not: `insert(parent, () => props.children, null)`
takes the array branch, `reconcileArrays` -> host `insertNode` gets the
function. Verified on rc.4 with a bare `createRenderer` (repro in the issue):

```
accessor then fragment -> [ [Function: read], "fragment" ]
insertNode received: [ "#text()", "FUNCTION (bug)", "frag-a", "frag-b" ]
```

Fix: `needsUnwrap = flattenArray(child, results, options) || needsUnwrap;`

## Upstream

Checked 2026-08-30: not known. No issue, PR or discussion on solidjs/solid
mentions it (searched flattenArray / needsUnwrap / doNotUnwrap / flatten /
universal insertNode); #2956 (Promise-valued JSX holes) is the only
`flattenArray` hit and is a different bug. The signals commit log between
rc.4 (2026-08-28) and `next` head does not touch `flatten`. Filed as #3133.

## On our side

App-level workarounds, both already what the scaffold AGENTS.md recommends:
resolve `props.children` through `children()` (it calls `flatten` without
`doNotUnwrap`, so the list is fully resolved), and have components return
one root instead of a fragment.

Done 2026-08-30, independently of the upstream fix: core `insertNode`
(packages/core/src/renderer.ts) throws a message naming the cause when the
value has no `id` (an accessor reached the renderer), instead of the FFI
`'undefined' into type 'f64'` error that cost the reporter most of a session.
Verified with scratch/flatten-guard/index.tsx (`FAIL` switch): the guard
fires under `<view>` and the leak sentinel follows it, since the throwing
insert leaves the fragment's nodes parentless; with `children()` in the
panel all five texts mount. Direct host children never form the array (the
compiler inserts them one by one); only a component's `props.children`
does, which is why it showed up inside a `Panel`. When the fix lands in our
tree the guard stays (cheap, and still the right message for any future
path); nothing to remove.

Found while verifying: `@solidjs/babel-plugin` (universal output) hoists a
JSX element's `createElement` + `insert` calls to the top of the enclosing
function, so `if (flag) return <view>{props.children}</view>` runs its
insert (and reads the element-valued prop, building a subtree) even when
`flag` is false. The `srt bundle` output shows `insert(_el$5, () =>
props.children)` before the `if`. Not written up separately yet.
