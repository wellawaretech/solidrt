---
title: rquickjs Opt rejects an explicitly passed undefined
description: Opt<T> treats only an ABSENT argument as None; an explicit undefined (ordinary JS for "not given", and what any wrapper forwarding its own optional parameter produces) is still converted into T and fails. Proposal is an undefined/null-tolerant optional param type, or Opt doing it natively.
project: rquickjs (github.com/DelSkayn/rquickjs)
versions: rquickjs 0.12.1
status: unfiled
link:
created: 2026-08-13
---

# rquickjs: Opt rejects an explicitly passed undefined

`Opt<T>`'s `FromParam` impl converts whenever an argument slot is filled, so
only a truly absent argument becomes `None`. But in JS, passing `undefined`
IS the idiom for "not given": WebIDL treats `undefined` as "use the default"
for every optional argument, and any wrapper that declares `function f(x,
opts?) { binding(x, opts) }` forwards `undefined` verbatim. Against such a
caller, `fn(x: String, opts: Opt<Object>)` throws `Error converting from js
'undefined' into type 'object'` even though the binding author declared the
argument optional.

Minimal reproduction:

```rust
let f = Function::new(ctx.clone(), |opts: Opt<Object>| opts.0.is_some())?;
ctx.globals().set("f", f)?;
ctx.eval::<bool, _>("f()")?;          // Ok(false)
ctx.eval::<bool, _>("f(undefined)")?; // Err: converting from js 'undefined' into type 'object'
```

`Opt<Option<T>>` works today (`Option`'s `FromJs` maps undefined/null to
`None`), but it is non-obvious, easy to forget, and the double unwrap
(`opts.0.flatten()`) reads poorly - in practice binding authors write
`Opt<T>` and ship the bug.

## Proposal for upstream

Either of:

1. A `FromParam` wrapper alongside `Opt` - say `Undefinedable<T>` or
   `OptArg<T>` - that yields `None` for absent, `undefined`, and (arguably)
   `null`, else converts to `T`. Mirrors how PR #317's `Nullable` grew a
   value-level wrapper for null.
2. `Opt<T>` doing this natively: peek the value and treat `undefined` as
   absent. Semantically this matches WebIDL optional arguments, but it is a
   behavior change for anyone relying on `Opt<Value>` distinguishing
   `f(undefined)` from `f()` - so a new type (option 1) is likely the safer
   sell.

## Prior art upstream (checked 2026-08-13)

- No existing issue covers this. Searches over the issue tracker for
  "Opt undefined", "optional argument", "FromParam" surface nothing on point.
- #361 "Optional argument support?" (closed) was a user who had missed that
  `Opt` exists; self-resolved, does not touch explicit undefined.
- PR #317 "Add Null support for function" (open, stalled 2024-06) is the
  nearest neighbor: a `Null`/`Nullable` wrapper for null-tolerant params, with
  the maintainer suggesting it become a `FromJs`/`IntoJs` type named
  `Nullable`. Shows openness to exactly this kind of wrapper; undefined is the
  missing sibling.

## Our side

Fixed locally by `flux/src/plugins/marshal.rs` `OptArg<T>`, a `FromParam`
newtype with the option-1 semantics, used for every optional binding argument
(see okf/done/binding-optional-arg-undefined.md). If upstream adopts an
equivalent, `OptArg` can become an alias for it or be deleted.
